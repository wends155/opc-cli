use opc_da_client::{MockServerConnector, MockState, OpcDaClient, OpcProvider, OpcValue};
use std::sync::Arc;
use std::sync::atomic::Ordering;

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
        .write_tag_batch("Mock.Server.1", writes.into())
        .await
        .expect("batch write should succeed");

    assert_eq!(results.len(), 3);
    for (i, res) in results.iter().enumerate() {
        assert!(res.is_success());
        assert_eq!(res.tag_id, format!("Tag{}", i + 1));
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
        .bind("Mock.Server.BatchWrite");

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
