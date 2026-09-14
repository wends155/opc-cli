use super::*;
use crate::com::connector::{
    GroupItemResult, GroupItemState, MockConnectedGroup, MockConnectedServer, MockServerConnector,
    MockState,
};
use crate::com::guard::GroupGuard;
use crate::errors::{OpcError, WorkerError};
use crate::types::{
    ClientItemHandle, IntoWriteBatch, OpcQuality, OpcServerEndpoint, OpcValue, ServerGroupHandle,
    ServerItemHandle, TagBatch, TagCollector,
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
        .sender()
        .unwrap()
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
        .sender()
        .unwrap()
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
    assert!(
        matches!(
            result,
            Err(OpcError::Worker(
                WorkerError::Panic(_) | WorkerError::WorkerTerminated
            ))
        ),
        "Expected OpcError::Worker(Panic | WorkerTerminated), got {:?}",
        result
    );
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
    assert!(
        matches!(
            result,
            Err(OpcError::Worker(WorkerError::Panic(ref msg))) if msg.contains("panic")
        ),
        "Expected OpcError::Worker(Panic), got {:?}",
        result
    );

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
        sender: Some(tx),
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
    assert!(matches!(
        err,
        OpcError::Worker(WorkerError::WorkerTerminated)
    ));
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
            writes: writes.into_write_batch(),
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

#[test]
fn test_worker_priority_queue_preempts_low_priority() {
    let mut q = PriorityRequestQueue::new();
    let (tx1, _rx1) = oneshot::channel();
    let (tx2, _rx2) = oneshot::channel();
    let (tx3, _rx3) = oneshot::channel();

    // Push low, then high, then another low
    q.push(ComRequest::ListServers {
        host: "localhost".into(),
        reply: tx1,
    });
    q.push(ComRequest::ReadTagValues {
        endpoint: OpcServerEndpoint::from("Server1"),
        tags: TagBatch::from_str_lenient("Tag1"),
        reply: tx2,
    });
    q.push(ComRequest::BrowseTags {
        endpoint: OpcServerEndpoint::from("Server1"),
        collector: TagCollector::new(10),
        reply: tx3,
    });

    // Next must be high priority ReadTagValues!
    let next = q.pop_next().expect("should have item");
    assert!(matches!(next, ComRequest::ReadTagValues { .. }));

    // Subsequent items must be the low priority ones in FIFO order
    let next2 = q.pop_next().expect("should have item");
    assert!(matches!(next2, ComRequest::ListServers { .. }));

    let next3 = q.pop_next().expect("should have item");
    assert!(matches!(next3, ComRequest::BrowseTags { .. }));

    assert!(q.pop_next().is_none());
}

#[test]
fn test_worker_priority_queue_drains_fifo_within_tier() {
    let mut q = PriorityRequestQueue::new();
    let (tx1, _rx1) = oneshot::channel();
    let (tx2, _rx2) = oneshot::channel();

    q.push(ComRequest::ReadTagValues {
        endpoint: OpcServerEndpoint::from("Server1"),
        tags: TagBatch::from_str_lenient("Tag1"),
        reply: tx1,
    });
    q.push(ComRequest::WriteTagValue {
        endpoint: OpcServerEndpoint::from("Server1"),
        tag_id: "Tag2".into(),
        value: OpcValue::Int(10),
        reply: tx2,
    });

    let first = q.pop_next().unwrap();
    assert!(matches!(first, ComRequest::ReadTagValues { .. }));
    let second = q.pop_next().unwrap();
    assert!(matches!(second, ComRequest::WriteTagValue { .. }));
}

#[test]
fn test_worker_priority_queue_empty_pop_returns_none() {
    let mut q = PriorityRequestQueue::new();
    assert!(q.is_empty());
    assert!(q.pop_next().is_none());
}

#[test]
fn test_elapsed_ms_calculation() {
    use crate::com::worker::elapsed_ms;
    use std::time::{Duration, Instant};

    let start = Instant::now()
        .checked_sub(Duration::from_millis(25))
        .expect("valid instant subtraction");
    let elapsed = elapsed_ms(start);
    assert!(
        elapsed >= 25,
        "expected at least 25ms elapsed, got {elapsed}ms"
    );
}

#[tokio::test]
async fn test_connect_eager_ping_success_and_cached_failure() {
    use std::sync::atomic::Ordering;
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());
    let client = crate::com::client::OpcDaClient::new(connector)
        .expect("client initialization must succeed")
        .bind(OpcServerEndpoint::from("Mock.Server.Ping"));

    // 1. Initial eager connect succeeds when ping() returns Ok(())
    let ping_res = client.connect_eager().await;
    assert!(
        ping_res.is_ok(),
        "connect_eager must succeed when server ping returns Ok: got {ping_res:?}"
    );

    // 2. Server connection is now retained in PooledServer.
    // Configure ping to fail on the cached connection via MockState
    state.should_fail_ping.store(true, Ordering::Relaxed);

    // 3. Subsequent eager connect must fail on the cached server connection
    let err = client.connect_eager().await.unwrap_err();
    assert!(
        matches!(err, OpcError::Com { .. }),
        "Expected OpcError::Com when ping fails on cached connection, got: {err:?}"
    );
}

#[tokio::test]
async fn test_active_group_cache_invalidation_and_retry_on_handle_error() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let read_counter = Arc::new(AtomicUsize::new(0));
    let read_counter_clone = read_counter.clone();
    let state = Arc::new(MockState::default());

    // Group fails specifically on the 2nd read invocation with OPC_E_INVALIDHANDLE (0xC0040001)
    let group = MockConnectedGroup {
        state: state.clone(),
        ..Default::default()
    }
    .with_read_fn(move |_source, handles| {
        let count = read_counter_clone.fetch_add(1, Ordering::Relaxed);
        if count == 1 {
            return Err(OpcError::Com {
                source: windows::core::Error::from_hresult(windows_core::HRESULT(
                    0xC004_0001_u32.cast_signed(),
                )),
            });
        }
        Ok(handles
            .iter()
            .enumerate()
            .map(|(i, &h)| {
                Ok(GroupItemState {
                    client_handle: ClientItemHandle::new(h.as_raw()),
                    value: OpcValue::Int(42 + i64::try_from(i).unwrap_or(0)),
                    quality: OpcQuality::GOOD,
                    timestamp: std::time::SystemTime::UNIX_EPOCH,
                })
            })
            .collect())
    });

    let server = Arc::new(MockConnectedServer {
        group: Arc::new(group),
        state: state.clone(),
        ..Default::default()
    });
    let connector = Arc::new(MockServerConnector {
        server,
        state: state.clone(),
        ..Default::default()
    });
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let endpoint = OpcServerEndpoint::from("Mock.Server.GroupRetry");
    let tags = TagBatch::from(vec!["Tag1".to_string(), "Tag2".to_string()]);

    // 1st read: establishes initial active group
    let res1 = worker
        .send_request(|reply| ComRequest::ReadTagValues {
            endpoint: endpoint.clone(),
            tags: tags.clone(),
            reply,
        })
        .await
        .expect("1st read must establish active group and succeed");
    assert_eq!(res1.len(), 2);
    assert_eq!(state.add_items_count.load(Ordering::Relaxed), 1);
    assert_eq!(read_counter.load(Ordering::Relaxed), 1);

    // 2nd read: encounters OPC_E_INVALIDHANDLE on cached group -> clears active_group
    // -> falls through to cache-miss -> invokes add_items -> retry read succeeds!
    let res2 = worker
        .send_request(|reply| ComRequest::ReadTagValues {
            endpoint: endpoint.clone(),
            tags: tags.clone(),
            reply,
        })
        .await
        .expect("2nd read must invalidate stale active group, auto-retry, and succeed");
    assert_eq!(res2.len(), 2);
    assert_eq!(
        state.add_items_count.load(Ordering::Relaxed),
        2,
        "add_items must be called again following active group cache invalidation"
    );
    assert_eq!(
        read_counter.load(Ordering::Relaxed),
        3,
        "Read count must be 3 (initial read, failing cached read, recovery read)"
    );
}

#[tokio::test]
async fn test_handle_read_vector_length_mismatch_returns_internal_error() {
    // Mock returns only 1 state for 2 requested valid tags
    let group = MockConnectedGroup::default().with_read_fn(|_source, _handles| {
        Ok(vec![Ok(GroupItemState {
            client_handle: ClientItemHandle::new(0),
            value: OpcValue::Int(100),
            quality: OpcQuality::GOOD,
            timestamp: std::time::SystemTime::UNIX_EPOCH,
        })])
    });

    let state = Arc::new(MockState::default());
    let server = Arc::new(MockConnectedServer {
        group: Arc::new(group),
        state: state.clone(),
        ..Default::default()
    });
    let connector = Arc::new(MockServerConnector {
        server,
        state: state.clone(),
        ..Default::default()
    });
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let endpoint = OpcServerEndpoint::from("Mock.Server.MismatchRead");
    let tags = TagBatch::from(vec!["Tag1".to_string(), "Tag2".to_string()]);

    let err = worker
        .send_request(|reply| ComRequest::ReadTagValues {
            endpoint,
            tags,
            reply,
        })
        .await
        .unwrap_err();

    assert!(
        matches!(err, OpcError::Internal(ref msg) if msg.contains("mismatched read result array size")),
        "Expected OpcError::Internal with mismatch message, got: {err:?}"
    );
}

#[tokio::test]
async fn test_handle_write_batch_vector_length_mismatch_returns_internal_error() {
    // Mock write returns only 1 result for 2 requested valid tags
    let group = MockConnectedGroup::default().with_write_fn(|_items| Ok(vec![Ok(())]));

    let state = Arc::new(MockState::default());
    let server = Arc::new(MockConnectedServer {
        group: Arc::new(group),
        state: state.clone(),
        ..Default::default()
    });
    let connector = Arc::new(MockServerConnector {
        server,
        state: state.clone(),
        ..Default::default()
    });
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let endpoint = OpcServerEndpoint::from("Mock.Server.MismatchWrite");
    let writes = vec![
        ("Tag1".to_string(), OpcValue::Int(1)),
        ("Tag2".to_string(), OpcValue::Int(2)),
    ];

    let err = worker
        .send_request(|reply| ComRequest::WriteTagValues {
            endpoint,
            writes: writes.into_write_batch(),
            reply,
        })
        .await
        .unwrap_err();

    assert!(
        matches!(err, OpcError::Internal(ref msg) if msg.contains("mismatched write result array size")),
        "Expected OpcError::Internal with mismatch message, got: {err:?}"
    );
}

#[tokio::test]
async fn test_priority_request_queue_clear_drops_senders() {
    use crate::com::worker::{ComRequest, PriorityRequestQueue};
    let mut queue = PriorityRequestQueue::default();
    let (tx1, rx1) = tokio::sync::oneshot::channel();
    let (tx2, rx2) = tokio::sync::oneshot::channel();

    queue.push(ComRequest::Ping {
        endpoint: OpcServerEndpoint::from("Mock.Server.Q1"),
        reply: tx1,
    });
    queue.push(ComRequest::Ping {
        endpoint: OpcServerEndpoint::from("Mock.Server.Q2"),
        reply: tx2,
    });

    assert!(!queue.is_empty());
    queue.clear();
    assert!(queue.is_empty());

    // Dropping queued requests must close their oneshot reply channels
    assert!(
        rx1.await.is_err(),
        "rx1 must receive RecvError after queue.clear()"
    );
    assert!(
        rx2.await.is_err(),
        "rx2 must receive RecvError after queue.clear()"
    );
}

#[tokio::test]
async fn test_browse_recursive_resilient_to_get_item_id_failure() {
    use std::sync::atomic::Ordering;
    let state = Arc::new(MockState::default());
    let server = Arc::new(
        MockConnectedServer::default().with_get_item_id_fn(|item_name| {
            if item_name == "FailingTag" {
                Err(OpcError::Com {
                    source: windows::core::Error::from_hresult(
                        windows_core::HRESULT(0x8000_4005_u32.cast_signed()), // E_FAIL
                    ),
                })
            } else {
                Ok(format!("Resolved.{item_name}"))
            }
        }),
    );
    server.organization.store(1, Ordering::Relaxed); // Hierarchical
    server.supports_flat_browse.store(false, Ordering::Relaxed); // Force browse_recursive

    *server.tags.lock().unwrap() = vec![
        "FailingTag".to_string(),
        "GoodTag1".to_string(),
        "GoodTag2".to_string(),
    ];
    *server.branch_tags.lock().unwrap() = vec![];

    let connector = Arc::new(MockServerConnector {
        server,
        state: state.clone(),
        ..Default::default()
    });
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let endpoint = OpcServerEndpoint::from("Mock.Server.BrowseResilient");

    let result = worker
        .send_request(|reply| ComRequest::BrowseTags {
            endpoint,
            collector: TagCollector::new(10),
            reply,
        })
        .await
        .expect("browse must succeed despite single leaf failure");

    assert_eq!(result.len(), 2, "Must collect 2 valid sibling leaf tags");
    assert!(result.contains(&"Resolved.GoodTag1".to_string()));
    assert!(result.contains(&"Resolved.GoodTag2".to_string()));
    assert!(!result.iter().any(|t| t.contains("FailingTag")));
}

#[tokio::test]
async fn test_progid_resolution_error_mapping_not_connection_error() {
    use crate::errors::hresult::CO_E_CLASSSTRING;
    let err = OpcError::Com {
        source: windows::core::Error::from_hresult(CO_E_CLASSSTRING),
    };
    assert!(
        !err.is_connection_error(),
        "CO_E_CLASSSTRING must NOT be classified as connection error"
    );
}

#[tokio::test]
async fn test_progid_resolution_failure_does_not_engage_circuit_breaker() {
    use std::sync::atomic::Ordering;
    let state = Arc::new(MockState::default());
    let connector = Arc::new(MockServerConnector {
        state: state.clone(),
        ..Default::default()
    });
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let endpoint = OpcServerEndpoint::from("NonExistent.ProgID.Invalid");

    // Configure mock to fail with CO_E_CLASSSTRING
    state.should_fail_progid.store(true, Ordering::Relaxed);

    // 1st request fails with COM error
    let err1 = worker
        .send_request(|reply| ComRequest::Ping {
            endpoint: endpoint.clone(),
            reply,
        })
        .await
        .unwrap_err();
    assert!(matches!(err1, OpcError::Com { .. }));

    // 2nd request immediately following must NOT fail with CooldownActive
    let err2 = worker
        .send_request(|reply| ComRequest::Ping {
            endpoint: endpoint.clone(),
            reply,
        })
        .await
        .unwrap_err();

    assert!(
        !matches!(err2, OpcError::Connection(ref msg) if msg.contains("circuit breaker cooldown")),
        "Subsequent call must not be blocked by circuit breaker cooldown: got {err2}"
    );
}

#[tokio::test]
async fn test_worker_thread_joins_on_drop() {
    let connector = Arc::new(MockServerConnector::default());
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    // Verify worker is operational
    let ping_res = worker
        .send_request(|reply| ComRequest::Ping {
            endpoint: OpcServerEndpoint::from("Mock.Server.DropTest"),
            reply,
        })
        .await;
    assert!(ping_res.is_ok(), "Ping request must succeed before drop");

    // Dropping worker in spawn_blocking joins thread handle deterministically
    let join_completed = tokio::task::spawn_blocking(move || {
        drop(worker);
        true
    })
    .await
    .unwrap();

    assert!(
        join_completed,
        "Worker drop must complete cleanly and join the worker thread"
    );
}

#[tokio::test]
async fn test_worker_browse_with_mock_associated_item_iterator() {
    let server = Arc::new(MockConnectedServer::default().with_tags(vec![
        "Device.Sensors.Pressure".to_string(),
        "Device.Sensors.Flow".to_string(),
    ]));
    let connector = Arc::new(MockServerConnector {
        server,
        ..Default::default()
    });
    let worker = tokio::task::spawn_blocking(move || ComWorker::start(connector).unwrap())
        .await
        .unwrap();

    let tags = worker
        .send_request(|reply| ComRequest::BrowseTags {
            endpoint: OpcServerEndpoint::from("Mock.Server.BrowseIter"),
            collector: crate::provider::TagCollector::default(),
            reply,
        })
        .await
        .expect("BrowseTags request should succeed");

    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0], "Device.Sensors.Pressure");
    assert_eq!(tags[1], "Device.Sensors.Flow");
}
