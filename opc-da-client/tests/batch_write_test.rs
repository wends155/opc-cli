use opc_da_client::connector::GroupItemResult;
use opc_da_client::connector::mock::{MockServerConnector, MockState};
use opc_da_client::types::{ServerItemHandle, VarType, WriteResult};
use opc_da_client::{OpcDaClient, OpcError, OpcProvider, OpcServerEndpoint, OpcValue};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[tokio::test]
async fn test_opc_provider_batch_write_atomic() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());
    let client = OpcDaClient::builder()
        .server("Mock.Server.1")
        .with_connector(connector)
        .build()
        .expect("build client");

    let writes = vec![
        ("Tag1".to_string(), OpcValue::Int(10)),
        ("Tag2".to_string(), OpcValue::Int(20)),
        ("Tag3".to_string(), OpcValue::Int(30)),
    ];

    fn assert_provider<P: OpcProvider>(_p: &P) {}
    assert_provider(&client);
    let results = client
        .write_tag_batch("Mock.Server.1", writes)
        .await
        .expect("write_tag_batch should succeed");

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].tag_id, "Tag1");
    assert_eq!(results[1].tag_id, "Tag2");
    assert_eq!(results[2].tag_id, "Tag3");
    for res in &results {
        assert!(res.is_success());
    }

    assert_eq!(
        state.add_group_count.load(Ordering::Relaxed),
        1,
        "write_tag_batch must dispatch a single atomic batch write group, not N individual groups"
    );
}

#[tokio::test]
async fn test_client_write_tags_accepts_array_slice_and_vector() {
    let state = std::sync::Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());
    let client = OpcDaClient::new(connector)
        .expect("client must initialize")
        .bind(OpcServerEndpoint::local_prog_id("Mock.Server.BatchWrite"));

    // 1. Array of pairs
    let array_res = client
        .write_tags([("Tag.A", OpcValue::Int(1)), ("Tag.B", OpcValue::Int(2))])
        .await;
    assert!(array_res.is_ok());

    // 2. Slice of pairs
    let slice_data = [("Tag.C", OpcValue::Int(3)), ("Tag.D", OpcValue::Int(4))];
    let slice_res = client.write_tags(&slice_data[..]).await;
    assert!(slice_res.is_ok());

    // 3. Vector of pairs
    let vec_data = vec![("Tag.E".to_string(), OpcValue::Int(5))];
    let vec_res = client.write_tags(vec_data).await;
    assert!(vec_res.is_ok());
}

#[tokio::test]
async fn test_client_batch_write_partial_rejection_preserves_order() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone())
        .with_servers(vec!["Mock.Server.Order".to_string()])
        .with_add_items_fn(|items| {
            Ok(items
                .iter()
                .enumerate()
                .map(|(idx, item)| {
                    if item.item_id == "Tag.AddFail" {
                        GroupItemResult {
                            server_handle: ServerItemHandle::new(0),
                            canonical_type: VarType::EMPTY,
                            error: Some(OpcError::InvalidState(
                                "Tag.AddFail not configured".into(),
                            )),
                        }
                    } else {
                        GroupItemResult {
                            #[allow(clippy::cast_possible_truncation)]
                            server_handle: ServerItemHandle::new((idx + 1) as u32),
                            canonical_type: VarType::EMPTY,
                            error: None,
                        }
                    }
                })
                .collect())
        })
        .with_write_fn(|items| {
            Ok(items
                .iter()
                .map(|item| {
                    if item.handle.as_raw() == 3 {
                        // 3rd item is Tag.WriteFail
                        Err(OpcError::InvalidState("Tag.WriteFail write denied".into()))
                    } else {
                        Ok(())
                    }
                })
                .collect())
        });

    let client = OpcDaClient::builder()
        .server("Mock.Server.Order")
        .with_connector(connector)
        .build_bound()
        .expect("build bound client");

    let writes = vec![
        ("Tag.Success1".to_string(), OpcValue::Int(10)),
        ("Tag.AddFail".to_string(), OpcValue::Int(20)),
        ("Tag.WriteFail".to_string(), OpcValue::Int(30)),
        ("Tag.Success2".to_string(), OpcValue::Int(40)),
    ];

    let results = client
        .write_tags(writes)
        .await
        .expect("batch write should return partial result vector without aborting");

    assert_eq!(results.len(), 4);

    assert_eq!(results[0].tag_id, "Tag.Success1");
    assert!(results[0].is_success());

    assert_eq!(results[1].tag_id, "Tag.AddFail");
    assert!(results[1].is_error());
    assert!(
        results[1]
            .error()
            .unwrap()
            .to_string()
            .contains("Tag.AddFail not configured")
    );

    assert_eq!(results[2].tag_id, "Tag.WriteFail");
    assert!(results[2].is_error());
    assert!(
        results[2]
            .error()
            .unwrap()
            .to_string()
            .contains("Tag.WriteFail write denied")
    );

    assert_eq!(results[3].tag_id, "Tag.Success2");
    assert!(results[3].is_success());

    assert_eq!(
        state.add_group_count.load(Ordering::Relaxed),
        1,
        "Entire batch must execute in a single COM group"
    );
}

#[tokio::test]
async fn test_client_batch_write_interior_null_byte_isolated() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());
    let client = OpcDaClient::builder()
        .server("Mock.Server.NullTest")
        .with_connector(connector)
        .build_bound()
        .expect("build bound client");

    let writes = vec![
        ("Valid.Tag1".to_string(), OpcValue::Int(100)),
        ("Corrupt\0.Tag".to_string(), OpcValue::Int(200)),
        ("Valid.Tag2".to_string(), OpcValue::Int(300)),
    ];

    let results = client
        .write_tags(writes)
        .await
        .expect("batch write with contaminated tag must complete");

    assert_eq!(results.len(), 3);

    assert_eq!(results[0].tag_id, "Valid.Tag1");
    assert!(results[0].is_success());

    assert_eq!(results[1].tag_id, "Corrupt\0.Tag");
    assert!(results[1].is_error());
    assert!(
        results[1]
            .error()
            .unwrap()
            .to_string()
            .contains("illegal interior null byte")
    );

    assert_eq!(results[2].tag_id, "Valid.Tag2");
    assert!(results[2].is_success());

    assert_eq!(
        state.add_group_count.load(Ordering::Relaxed),
        1,
        "Exactly 1 COM group registered for the valid tags"
    );
}

#[tokio::test]
async fn test_client_batch_write_empty_short_circuits() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());
    let client = OpcDaClient::builder()
        .server("Mock.Server.Empty")
        .with_connector(connector)
        .build_bound()
        .expect("build bound client");

    let empty: Vec<(String, OpcValue)> = vec![];
    let results = client
        .write_tags(empty)
        .await
        .expect("empty batch must succeed");

    assert!(results.is_empty());
    assert_eq!(
        state.add_group_count.load(Ordering::Relaxed),
        0,
        "No COM groups must be allocated for empty batch"
    );
}

#[tokio::test]
async fn test_client_batch_write_all_rejected_skips_write() {
    let state = Arc::new(MockState::default());
    let write_called = Arc::new(AtomicBool::new(false));
    let write_called_clone = write_called.clone();

    let connector = MockServerConnector::with_state(state.clone())
        .with_servers(vec!["Mock.Server.AllReject".to_string()])
        .with_add_items_fn(|items| {
            Ok(items
                .iter()
                .map(|_| GroupItemResult {
                    server_handle: ServerItemHandle::new(0),
                    canonical_type: VarType::EMPTY,
                    error: Some(OpcError::InvalidState("Item rejected in add_items".into())),
                })
                .collect())
        })
        .with_write_fn(move |items| {
            write_called_clone.store(true, Ordering::Relaxed);
            Ok(items.iter().map(|_| Ok(())).collect())
        });

    let client = OpcDaClient::builder()
        .server("Mock.Server.AllReject")
        .with_connector(connector)
        .build_bound()
        .expect("build bound client");

    let writes = vec![
        ("Tag1".to_string(), OpcValue::Int(1)),
        ("Tag2".to_string(), OpcValue::Int(2)),
    ];

    let results = client
        .write_tags(writes)
        .await
        .expect("all-rejected registration must still return WriteResult vector");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].tag_id, "Tag1");
    assert!(results[0].is_error());
    assert!(
        results[0]
            .error()
            .unwrap()
            .to_string()
            .contains("Item rejected in add_items")
    );
    assert_eq!(results[1].tag_id, "Tag2");
    assert!(results[1].is_error());
    assert!(
        results[1]
            .error()
            .unwrap()
            .to_string()
            .contains("Item rejected in add_items")
    );
    assert!(
        !write_called.load(Ordering::Relaxed),
        "group.write() must not be invoked when all items are rejected during registration"
    );
}

#[tokio::test]
async fn test_client_batch_write_all_null_short_circuits() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());
    let client = OpcDaClient::builder()
        .server("Mock.Server.AllNull")
        .with_connector(connector)
        .build_bound()
        .expect("build bound client");

    let writes = vec![
        ("Bad\0Tag1".to_string(), OpcValue::Int(1)),
        ("Bad\0Tag2".to_string(), OpcValue::Int(2)),
    ];

    let results = client
        .write_tags(writes)
        .await
        .expect("all-null batch must return results without aborting");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].tag_id, "Bad\0Tag1");
    assert!(results[0].is_error());
    assert!(
        results[0]
            .error()
            .unwrap()
            .to_string()
            .contains("illegal interior null byte")
    );
    assert_eq!(results[1].tag_id, "Bad\0Tag2");
    assert!(results[1].is_error());
    assert!(
        results[1]
            .error()
            .unwrap()
            .to_string()
            .contains("illegal interior null byte")
    );
    assert_eq!(
        state.add_group_count.load(Ordering::Relaxed),
        0,
        "No COM groups must be allocated when all tags contain null bytes"
    );
}

#[test]
fn test_write_result_display_and_connection_error() {
    let ok = WriteResult::success("Plant.Pump1");
    assert!(!ok.is_connection_error());
    assert_eq!(format!("{ok}"), "Write 'Plant.Pump1': succeeded");

    let conn_err = WriteResult::failure(
        "Plant.Pump1",
        OpcError::Connection("Lost connection to PLC".into()),
    );
    assert!(conn_err.is_connection_error());
    assert_eq!(
        format!("{conn_err}"),
        "Write 'Plant.Pump1': failed (Connection failed: Lost connection to PLC)"
    );

    let config_err = WriteResult::failure(
        "Plant.Pump2",
        OpcError::InvalidState("Unknown tag name".into()),
    );
    assert!(!config_err.is_connection_error());
    assert_eq!(
        format!("{config_err}"),
        "Write 'Plant.Pump2': failed (Invalid state: Unknown tag name)"
    );
}
