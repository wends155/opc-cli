//! Integration tests for OPC DA connection resilience, connection pooling, and worker lifecycle.
//!
//! Validates eager ping reachability, active group auto-recovery on invalid handles (`0xC0040001`),
//! connection drop eviction and transparent reconnection (`0x800706BA`), circuit breaker cooldown
//! short-circuiting, and deterministic thread lifecycle teardown.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use opc_da_client::OpcDaClient;
use opc_da_client::connector::mock::{MockServerConnector, MockState};
use opc_da_client::connector::traits::GroupItemState;
use opc_da_client::errors::OpcError;
use opc_da_client::errors::hresult::RPC_S_SERVER_UNAVAILABLE;
use opc_da_client::types::{ClientItemHandle, OpcQuality, OpcValue};

#[tokio::test]
async fn test_eager_ping_reachability() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());

    // 1. Healthy ping probe on eager connection succeeds
    let client = OpcDaClient::builder()
        .server("Mock.Simulation.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    assert_eq!(client.server_id(), "Mock.Simulation.Server");
    assert!(client.connect_eager().await.is_ok());

    // 2. Unreachable server fails eager ping probe
    state.should_fail_ping.store(true, Ordering::Relaxed);
    let ping_err = client.connect_eager().await.unwrap_err();
    match ping_err {
        OpcError::Com { source } => {
            assert_eq!(source.code(), RPC_S_SERVER_UNAVAILABLE);
        }
        other => panic!("Expected OpcError::Com, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_active_group_auto_recovery_on_invalid_handle() {
    const OPC_E_INVALIDHANDLE: windows_core::HRESULT =
        windows_core::HRESULT(0xC004_0001_u32.cast_signed());

    let state = Arc::new(MockState::default());
    let read_counter = Arc::new(AtomicUsize::new(0));
    let read_counter_clone = read_counter.clone();

    // Group fails specifically on the 2nd read invocation with OPC_E_INVALIDHANDLE (0xC0040001)
    let connector =
        MockServerConnector::with_state(state.clone()).with_read_fn(move |_source, handles| {
            let count = read_counter_clone.fetch_add(1, Ordering::Relaxed);
            if count == 1 {
                return Err(OpcError::Com {
                    source: windows_core::Error::from_hresult(OPC_E_INVALIDHANDLE),
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

    let client = OpcDaClient::builder()
        .server("Mock.AutoRecovery.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    // 1st read: cache miss, creates group, adds item, reads successfully
    let res1 = client
        .read_tags(["Tag1", "Tag2"])
        .await
        .expect("1st read must succeed");
    assert_eq!(res1[0].outcome, Ok(OpcValue::Int(42)));
    assert_eq!(res1[1].outcome, Ok(OpcValue::Int(43)));
    assert_eq!(state.add_group_count.load(Ordering::Relaxed), 1);
    assert_eq!(state.add_items_count.load(Ordering::Relaxed), 1);

    // 2nd read: triggers 0xC0040001 error on cached group, worker clears active group,
    // re-creates group, re-adds items, and successfully retries!
    let res2 = client
        .read_tags(["Tag1", "Tag2"])
        .await
        .expect("2nd read must recover and succeed");
    assert_eq!(res2[0].outcome, Ok(OpcValue::Int(42)));
    assert_eq!(res2[1].outcome, Ok(OpcValue::Int(43)));
    assert_eq!(
        state.add_group_count.load(Ordering::Relaxed),
        2,
        "add_group_count must be 2 after transparent invalidation and auto-recovery"
    );
    assert_eq!(
        state.add_items_count.load(Ordering::Relaxed),
        2,
        "add_items_count must be 2 after re-adding items"
    );
}

#[tokio::test]
async fn test_connection_drop_eviction_and_reconnection() {
    let state = Arc::new(MockState::default());
    let read_counter = Arc::new(AtomicUsize::new(0));
    let read_counter_clone = read_counter.clone();

    let connector =
        MockServerConnector::with_state(state.clone()).with_read_fn(move |_source, handles| {
            let count = read_counter_clone.fetch_add(1, Ordering::Relaxed);
            // On call 1 (the 2nd read call overall), simulate dropped RPC connection
            if count == 1 {
                return Err(OpcError::Com {
                    source: windows_core::Error::from_hresult(RPC_S_SERVER_UNAVAILABLE),
                });
            }
            Ok(handles
                .iter()
                .map(|&h| {
                    Ok(GroupItemState {
                        client_handle: ClientItemHandle::new(h.as_raw()),
                        value: OpcValue::Int(42),
                        quality: OpcQuality::GOOD,
                        timestamp: std::time::SystemTime::UNIX_EPOCH,
                    })
                })
                .collect())
        });

    let client = OpcDaClient::builder()
        .server("Mock.Reconnect.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    // 1st read: establishes initial connection
    let res1 = client
        .read_tag("Sensor.Flow")
        .await
        .expect("read 1 must succeed");
    assert_eq!(res1.outcome, Ok(OpcValue::Int(42)));
    assert_eq!(state.connect_count.load(Ordering::Relaxed), 1);

    // 2nd read: Call 1 fails with RPC_S_SERVER_UNAVAILABLE -> worker evicts stale proxy
    // -> calls connect_endpoint afresh -> retries read (Call 2) -> succeeds!
    let res2 = client
        .read_tag("Sensor.Flow")
        .await
        .expect("read 2 must reconnect and succeed");
    assert_eq!(res2.outcome, Ok(OpcValue::Int(42)));
    assert_eq!(
        state.connect_count.load(Ordering::Relaxed),
        2,
        "connect_count must be 2 indicating stale proxy eviction and fresh reconnection"
    );
}

#[tokio::test]
async fn test_circuit_breaker_failure_cooldown_short_circuit() {
    let state = Arc::new(MockState::default());
    state.should_fail_connection.store(true, Ordering::Relaxed);

    let connector = MockServerConnector::with_state(state.clone());
    let client = OpcDaClient::builder()
        .server("Mock.Cooldown.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    // 1. Initial read fails because connection attempt fails with connection error
    let err1 = client.read_tag("Sensor.Temp").await.unwrap_err();
    assert!(err1.is_connection_error());

    // 2. Immediate subsequent request must short-circuit in circuit breaker cooldown
    let err2 = client.read_tag("Sensor.Temp").await.unwrap_err();
    match err2 {
        OpcError::Connection(msg) => {
            assert!(
                msg.contains("circuit breaker cooldown"),
                "Expected circuit breaker cooldown message, got: {msg}"
            );
        }
        other => panic!("Expected OpcError::Connection cooldown error, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_client_worker_deterministic_lifecycle_teardown() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state);

    let client = OpcDaClient::builder()
        .server("Mock.Lifecycle.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    // Perform an initial read to guarantee the worker thread is actively processing requests
    let read_res = client.read_tag("Lifecycle.Tag").await;
    assert!(read_res.is_ok());

    // Explicitly drop client; ComWorker::drop sends channel disconnect and joins OS thread
    drop(client);

    // If ComWorker thread teardown was not clean or hung on channel join, this test would hang or leak.
    // Reaching this point confirms deterministic termination.
}
