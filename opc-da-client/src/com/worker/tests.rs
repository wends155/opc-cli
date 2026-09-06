use super::*;
use crate::com::connector::{
    GroupItemResult, GroupItemState, MockConnectedGroup, MockConnectedServer, MockServerConnector,
    MockState,
};
use crate::com::guard::GroupGuard;
use crate::errors::OpcError;
use crate::types::{
    ClientItemHandle, OpcQuality, OpcServerEndpoint, OpcValue, ServerGroupHandle, ServerItemHandle,
    TagBatch, TagCollector,
};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::oneshot;

#[tokio::test]
async fn test_worker_starts_and_stops() {
    let worker = tokio::task::spawn_blocking(|| {
        ComWorker::start(Arc::new(MockServerConnector::new())).unwrap()
    })
    .await
    .unwrap();
    drop(worker);
}

#[tokio::test]
async fn test_worker_list_servers() {
    let worker = tokio::task::spawn_blocking(|| {
        ComWorker::start(Arc::new(
            MockServerConnector::new().with_servers(vec!["Mock.Server.1".into()]),
        ))
        .unwrap()
    })
    .await
    .unwrap();
    let (reply, _rx) = oneshot::channel();
    worker
        .sender
        .send(ComRequest::ListServers {
            host: "localhost".into(),
            reply,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn test_worker_list_server_details() {
    let worker = tokio::task::spawn_blocking(|| {
        ComWorker::start(Arc::new(
            MockServerConnector::new().with_servers(vec!["Mock.Server.1".into()]),
        ))
        .unwrap()
    })
    .await
    .unwrap();
    let (reply, rx) = oneshot::channel();
    worker
        .sender
        .send(ComRequest::ListServerDetails {
            host: "localhost".into(),
            reply,
        })
        .await
        .unwrap();
    let details = rx.await.unwrap().unwrap();
    assert_eq!(details.len(), 1);
    assert_eq!(details[0].prog_id, "Mock.Server.1");
}

#[tokio::test]
async fn test_worker_read_tag_values_mismatched_lengths() {
    let connector = Arc::new(MockServerConnector::new().with_add_items_fn(|_| Ok(vec![])));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let result = worker
        .send_request(|reply| ComRequest::ReadTagValues {
            endpoint: OpcServerEndpoint::from("MockServer"),
            tags: TagBatch::from(vec!["Tag1".to_string(), "Tag2".to_string()]),
            reply,
        })
        .await;

    assert!(
        result.is_err(),
        "Expected read to fail due to mismatched lengths"
    );
    if let Err(OpcError::Internal(msg)) = result {
        assert!(msg.contains("mismatched result array sizes"));
    } else {
        panic!("Expected OpcError::Internal, got {:?}", result);
    }
}

#[tokio::test]
async fn test_worker_write_tag_value() {
    let state = Arc::new(MockState::default());
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let result = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            tag_id: "Random.Int4".to_string(),
            value: OpcValue::Int(42),
            reply,
        })
        .await
        .expect("Request should succeed");

    assert_eq!(result.tag_id, "Random.Int4");
    assert!(result.is_success(), "Write should be successful");
    assert!(result.error().is_none());
}

#[tokio::test]
async fn test_worker_write_tag_value_failure() {
    let state = Arc::new(MockState::default());
    state.should_fail_write.store(true, Ordering::Relaxed);
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let result = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            tag_id: "Random.Int4".to_string(),
            value: OpcValue::Int(42),
            reply,
        })
        .await
        .expect("Request should complete");

    assert_eq!(result.tag_id, "Random.Int4");
    assert!(result.is_error(), "Write should fail");
    match result.status {
        Err(OpcError::Com { source }) => {
            assert_eq!(source.code(), windows::Win32::Foundation::E_FAIL);
        }
        other => panic!("Expected OpcError::Com, got {:?}", other),
    }
}

#[tokio::test]
async fn test_connection_cache_reuse() {
    let state = Arc::new(MockState::default());
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let _ = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            tag_id: "Tag1".to_string(),
            value: OpcValue::Int(1),
            reply,
        })
        .await
        .unwrap();

    let _ = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            tag_id: "Tag2".to_string(),
            value: OpcValue::Int(2),
            reply,
        })
        .await
        .unwrap();

    assert_eq!(
        state.connect_count.load(Ordering::Relaxed),
        1,
        "Server connection should be cached and reused"
    );
}

#[tokio::test]
async fn test_stale_connection_eviction() {
    let state = Arc::new(MockState::default());
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    // Initial connect
    let _ = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            tag_id: "Tag1".to_string(),
            value: OpcValue::Int(1),
            reply,
        })
        .await
        .unwrap();

    assert_eq!(state.connect_count.load(Ordering::Relaxed), 1);

    // Enable connection error flag to trigger eviction on next operation
    state
        .should_fail_with_connection_error
        .store(true, Ordering::Relaxed);

    // Next request triggers eviction and reconnect attempt
    let _ = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            tag_id: "Tag2".to_string(),
            value: OpcValue::Int(2),
            reply,
        })
        .await;

    assert_eq!(
        state.connect_count.load(Ordering::Relaxed),
        2,
        "Stale connection should be evicted and reconnected"
    );
}

#[tokio::test]
async fn test_worker_panic_propagation() {
    let state = Arc::new(MockState::default());
    state.should_panic_on_request.store(true, Ordering::Relaxed);
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let result = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            tag_id: "Tag1".to_string(),
            value: OpcValue::Int(1),
            reply,
        })
        .await;

    assert!(result.is_err());
    if let Err(OpcError::Internal(msg)) = result {
        assert!(
            msg.contains("shut down") || msg.contains("channel closed") || msg.contains("panicked"),
            "Expected worker termination message, got: {}",
            msg
        );
    } else {
        panic!("Expected OpcError::Internal, got {:?}", result);
    }
}

#[tokio::test]
async fn test_worker_thread_recovery_after_panic() {
    let state = Arc::new(MockState::default());
    state.should_panic_on_request.store(true, Ordering::Relaxed);
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    // First request triggers simulated panic
    let result = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            tag_id: "Tag1".to_string(),
            value: OpcValue::Int(1),
            reply,
        })
        .await;

    assert!(result.is_err());
    if let Err(OpcError::Internal(msg)) = result {
        assert!(
            msg.contains("panicked"),
            "Expected worker panic message, got: {msg}"
        );
    } else {
        panic!("Expected OpcError::Internal, got {:?}", result);
    }

    // Now disarm the panic trigger and verify worker thread survived and processes subsequent requests
    state
        .should_panic_on_request
        .store(false, Ordering::Relaxed);

    let recovery_result = worker
        .send_request(|reply| ComRequest::WriteTagValue {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            tag_id: "Tag1".to_string(),
            value: OpcValue::Int(42),
            reply,
        })
        .await
        .expect("Worker thread must recover and process subsequent request successfully");

    assert!(recovery_result.is_success());
    assert_eq!(recovery_result.tag_id, "Tag1");
}

#[tokio::test]
async fn test_drop_during_active_request() {
    let state = Arc::new(MockState::default());
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    // Dropping worker handle closes channel gracefully
    drop(worker);
}

#[tokio::test]
async fn test_worker_init_failure() {
    let state = Arc::new(MockState::default());
    state.should_fail_connect.store(true, Ordering::Relaxed);
    let worker = tokio::task::spawn_blocking(move || {
        ComWorker::start(Arc::new(MockServerConnector::with_state(state))).unwrap()
    })
    .await
    .unwrap();

    let result = worker
        .send_request(|reply| ComRequest::ListServers {
            host: "localhost".into(),
            reply,
        })
        .await;

    assert!(
        result.is_err(),
        "ListServers request should fail when connector enumeration fails"
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn test_worker_read_tag_values_quality_decoding() {
    use crate::types::{QualityLimit, QualityMajor, QualitySubstatus};

    let connector = MockServerConnector::new()
        .with_add_items_fn(|items| {
            Ok(items
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    if i == 4 {
                        GroupItemResult {
                            server_handle: ServerItemHandle::new(0),
                            canonical_type: 0,
                            error: Some(OpcError::Com {
                                source: windows::core::Error::from_hresult(
                                    windows::Win32::Foundation::E_FAIL,
                                ),
                            }),
                        }
                    } else {
                        GroupItemResult {
                            #[allow(clippy::cast_possible_truncation)]
                            server_handle: ServerItemHandle::new((i + 1) as u32),
                            canonical_type: 8,
                            error: None,
                        }
                    }
                })
                .collect())
        })
        .with_read_fn(|_source, server_handles| {
            let qualities: [u16; 4] = [0x00C0, 0x00D8, 0x0018, 0x0056];
            Ok(server_handles
                .iter()
                .enumerate()
                .map(|(i, &h)| {
                    let val = if i != 2 {
                        OpcValue::Int(42)
                    } else {
                        OpcValue::String(String::new())
                    };
                    Ok(GroupItemState {
                        client_handle: ClientItemHandle::new(h.as_raw()),
                        value: val,
                        quality: OpcQuality::from(qualities[i % qualities.len()]),
                        timestamp: std::time::SystemTime::UNIX_EPOCH,
                    })
                })
                .collect())
        });

    let worker =
        tokio::task::spawn_blocking(move || ComWorker::start(Arc::new(connector)).unwrap())
            .await
            .unwrap();

    let tag_ids = vec![
        "Tag.Good".to_string(),
        "Tag.Override".to_string(),
        "Tag.Comm".to_string(),
        "Tag.Limit".to_string(),
        "Tag.Rejected".to_string(),
    ];

    let results = worker
        .send_request(|reply| ComRequest::ReadTagValues {
            endpoint: OpcServerEndpoint::from("Quality.Mock.Server"),
            tags: TagBatch::from(tag_ids),
            reply,
        })
        .await
        .unwrap();

    let results = results.into_vec();
    assert_eq!(results.len(), 5);

    // Tag 0: Good standard (0x00C0)
    assert_eq!(results[0].tag_id, "Tag.Good");
    assert_eq!(results[0].value(), Some(&OpcValue::Int(42)));
    assert_eq!(results[0].display_value(), "42");
    assert_eq!(results[0].quality.major(), QualityMajor::Good);
    assert_eq!(
        results[0].quality.substatus(),
        QualitySubstatus::NonSpecific
    );
    assert_eq!(results[0].quality.limit(), QualityLimit::NotLimited);
    assert_eq!(results[0].quality.to_string(), "Good");
    assert!(results[0].quality.is_good());
    assert!(!results[0].quality.is_bad());
    assert!(results[0].is_good());
    assert!(!results[0].is_error());

    // Tag 1: Good with Local Override (0x00D8)
    assert_eq!(results[1].tag_id, "Tag.Override");
    assert_eq!(results[1].value(), Some(&OpcValue::Int(42)));
    assert_eq!(results[1].quality.major(), QualityMajor::Good);
    assert_eq!(
        results[1].quality.substatus(),
        QualitySubstatus::LocalOverride
    );
    assert_eq!(results[1].quality.to_string(), "Good (Local Override)");

    // Tag 2: Bad with Comm Failure (0x0018)
    assert_eq!(results[2].tag_id, "Tag.Comm");
    assert_eq!(results[2].value(), Some(&OpcValue::String(String::new())));
    assert_eq!(results[2].quality.major(), QualityMajor::Bad);
    assert_eq!(
        results[2].quality.substatus(),
        QualitySubstatus::CommFailure
    );
    assert_eq!(results[2].quality.to_string(), "Bad (Comm Failure)");
    assert!(results[2].quality.is_bad());

    // Tag 3: Uncertain with EGU Exceeded and High Limited (0x0056)
    assert_eq!(results[3].tag_id, "Tag.Limit");
    assert_eq!(results[3].value(), Some(&OpcValue::Int(42)));
    assert_eq!(results[3].quality.major(), QualityMajor::Uncertain);
    assert_eq!(
        results[3].quality.substatus(),
        QualitySubstatus::EguExceeded
    );
    assert_eq!(results[3].quality.limit(), QualityLimit::HighLimited);
    assert_eq!(
        results[3].quality.to_string(),
        "Uncertain (EGU Exceeded) [High Limited]"
    );
    assert!(results[3].quality.is_uncertain());
    assert!(results[3].quality.is_limited());

    // Tag 4: Rejected at add_items
    assert_eq!(results[4].tag_id, "Tag.Rejected");
    assert_eq!(results[4].value(), None);
    assert_eq!(results[4].display_value(), "Error");
    assert_eq!(results[4].timestamp, None);
    assert_eq!(results[4].formatted_timestamp(), "N/A");
    assert!(results[4].is_error());
    assert_eq!(results[4].quality, OpcQuality::BAD_CONFIG_ERROR);
    assert_eq!(results[4].quality.to_string(), "Bad (Configuration Error)");
}

#[test]
fn test_worker_com_init_failure_propagates_opc_error() {
    let connector = Arc::new(MockServerConnector::default());
    let result = ComWorker::start_with_initializer::<crate::com::guard::FailingComInit>(connector);
    assert!(result.is_err());
    let Err(err) = result else { unreachable!() };
    assert!(
        !err.to_string().contains("COM init failed on worker"),
        "Expected forwarded OpcError, got hardcoded string: {err}"
    );
    assert!(
        err.to_string().contains("Synthetic COM init failure"),
        "Expected synthetic failure message, got: {err}"
    );
}

#[tokio::test]
async fn test_worker_browse_tags_success() {
    let connector = Arc::new(MockServerConnector::default());
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let collector = TagCollector::new(100);
    let result = worker
        .send_request(|reply| ComRequest::BrowseTags {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            collector: collector.clone(),
            reply,
        })
        .await
        .expect("BrowseTags request should succeed");

    assert_eq!(result.len(), 3);
    assert_eq!(result, vec!["Random.Int4", "Random.Real8", "Random.String"]);
    assert_eq!(collector.len(), 3);
}

#[tokio::test]
async fn test_worker_browse_tags_cancelled() {
    let connector = Arc::new(MockServerConnector::default());
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let collector = TagCollector::new(100);
    collector.cancel();
    let result = worker
        .send_request(|reply| ComRequest::BrowseTags {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            collector: collector.clone(),
            reply,
        })
        .await
        .expect("BrowseTags request should succeed when cancelled");

    assert_eq!(result.len(), 0);
    assert_eq!(collector.len(), 0);
}

#[tokio::test]
async fn test_worker_browse_tags_capacity_cap() {
    let connector = Arc::new(MockServerConnector::default());
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let collector = TagCollector::new(2);
    let result = worker
        .send_request(|reply| ComRequest::BrowseTags {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            collector: collector.clone(),
            reply,
        })
        .await
        .expect("BrowseTags request should succeed up to capacity");

    assert_eq!(result.len(), 2);
    assert_eq!(result, vec!["Random.Int4", "Random.Real8"]);
    assert_eq!(collector.len(), 2);
}

#[tokio::test]
async fn test_worker_browse_tags_flat_organization() {
    let connector = Arc::new(MockServerConnector::default());
    connector.server.organization.store(2, Ordering::Relaxed); // NamespaceType::Flat = 2
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let collector = TagCollector::new(100);
    let result = worker
        .send_request(|reply| ComRequest::BrowseTags {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            collector: collector.clone(),
            reply,
        })
        .await
        .expect("BrowseTags request should succeed on flat namespace");

    assert_eq!(result.len(), 3);
    assert_eq!(result, vec!["Random.Int4", "Random.Real8", "Random.String"]);
}

#[tokio::test]
async fn test_worker_tracing_instrumentation_execution() {
    let connector = std::sync::Arc::new(MockServerConnector::default());
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();
    let servers = worker
        .send_request(|reply| ComRequest::ListServers {
            host: "localhost".into(),
            reply,
        })
        .await
        .expect("list servers");
    assert_eq!(servers, vec!["Matrikon.OPC.Simulation.1".to_string()]);
}

#[test]
fn test_group_guard_cleanup_on_drop() {
    let server = MockConnectedServer::default();
    assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 0);
    {
        let guard = GroupGuard::new(&server, ServerGroupHandle::new(42));
        assert_eq!(guard.handle(), ServerGroupHandle::new(42));
    }
    assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 1);
}

#[test]
fn test_group_guard_disarm_prevents_cleanup() {
    let server = MockConnectedServer::default();
    {
        let mut guard = GroupGuard::new(&server, ServerGroupHandle::new(42));
        guard.disarm();
    }
    assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn test_worker_handle_read_error_cleans_group() {
    let connector = Arc::new(MockServerConnector::default());
    let group = MockConnectedGroup::default()
        .with_add_items_fn(|_| Err(OpcError::Internal("Simulated add_items failure".into())));
    let server = Arc::new(MockConnectedServer {
        group: Arc::new(group),
        state: connector.state.clone(),
        tags: std::sync::Arc::new(std::sync::Mutex::new(vec!["Test.Tag".to_string()])),
        ..Default::default()
    });
    let custom_connector = Arc::new(MockServerConnector {
        server: server.clone(),
        state: connector.state.clone(),
        servers: connector.servers.clone(),
        server_details: connector.server_details.clone(),
    });
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(custom_connector).unwrap())
        .await
        .unwrap();

    let result = worker
        .send_request(|reply| ComRequest::ReadTagValues {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            tags: TagBatch::from(vec!["Test.Tag".to_string()]),
            reply,
        })
        .await;

    assert!(result.is_err());
    assert_eq!(server.state.remove_group_count.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn test_worker_channel_drop_error_propagation() {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    drop(rx); // Drop receiver to simulate closed worker channel
    let worker: ComWorker<MockServerConnector> = ComWorker {
        sender: tx,
        handle: None,
        _phantom: std::marker::PhantomData,
    };
    let err = worker
        .send_request(|reply| ComRequest::ListServers {
            host: "localhost".into(),
            reply,
        })
        .await
        .unwrap_err();
    assert!(matches!(err, OpcError::Internal(msg) if msg.contains("channel closed")));
}

#[tokio::test]
async fn test_worker_native_write_batch_via_com_request() {
    let state = Arc::new(MockState::default());
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let writes = vec![
        ("Random.Int4".to_string(), OpcValue::Int(42)),
        ("Random.Real8".to_string(), OpcValue::Float(12.345)),
    ];

    let results = worker
        .send_request(|reply| ComRequest::WriteTagValues {
            endpoint: OpcServerEndpoint::from("Mock.Server.1"),
            writes,
            reply,
        })
        .await
        .expect("batch write should succeed");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].tag_id, "Random.Int4");
    assert!(results[0].is_success());
    assert_eq!(results[1].tag_id, "Random.Real8");
    assert!(results[1].is_success());
}
