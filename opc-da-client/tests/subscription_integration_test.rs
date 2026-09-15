//! Integration tests for OPC DA subscription polling streams and background tasks.
//!
//! Exercises `Bound::subscribe`, cadence timing, cancellation, dynamic updates,
//! and error resilience using `MockServerConnector` without Windows COM dependencies.

use opc_da_client::OpcDaClient;
use opc_da_client::connector::mock::{MockServerConnector, MockState};
use opc_da_client::connector::traits::GroupItemState;
use opc_da_client::errors::OpcError;
use opc_da_client::errors::hresult::{E_FAIL, RPC_S_SERVER_UNAVAILABLE};
use opc_da_client::types::{ClientItemHandle, OpcQuality, OpcValue};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::time::Duration;

#[tokio::test]
async fn test_subscription_multi_tick_cadence() {
    let state = Arc::new(MockState::default());
    let connector =
        MockServerConnector::with_state(state.clone()).with_tag_values(vec![OpcValue::Int(100)]);

    let client = OpcDaClient::builder()
        .server("Mock.Sub.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    let mut rx = client.subscribe(["Sensor.Pressure"], Duration::from_millis(15));

    for tick_idx in 1..=3 {
        let tick = tokio::time::timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("tick must not time out")
            .expect("channel must remain open");

        assert_eq!(
            tick.get_i32("Sensor.Pressure"),
            Ok(100),
            "tick {tick_idx} value match"
        );
    }
}

#[tokio::test]
async fn test_subscription_dynamic_mock_value_updates() {
    let state = Arc::new(MockState::default());
    let counter = Arc::new(AtomicI32::new(10));
    let counter_clone = counter.clone();

    let connector =
        MockServerConnector::with_state(state.clone()).with_read_fn(move |_source, handles| {
            let current = counter_clone.fetch_add(10, Ordering::Relaxed);
            Ok(handles
                .iter()
                .map(|&h| {
                    Ok(GroupItemState {
                        client_handle: ClientItemHandle::new(h.as_raw()),
                        value: OpcValue::Int(i64::from(current)),
                        quality: OpcQuality::GOOD,
                        timestamp: std::time::SystemTime::UNIX_EPOCH,
                    })
                })
                .collect())
        });

    let client = OpcDaClient::builder()
        .server("Mock.SubDynamic.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    let mut rx = client.subscribe(["Dynamic.Sensor"], Duration::from_millis(15));

    let tick1 = tokio::time::timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("tick 1 timeout")
        .expect("tick 1 channel open");
    assert_eq!(tick1.get_i32("Dynamic.Sensor"), Ok(10));

    let tick2 = tokio::time::timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("tick 2 timeout")
        .expect("tick 2 channel open");
    assert_eq!(tick2.get_i32("Dynamic.Sensor"), Ok(20));

    let tick3 = tokio::time::timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("tick 3 timeout")
        .expect("tick 3 channel open");
    assert_eq!(tick3.get_i32("Dynamic.Sensor"), Ok(30));
}

#[tokio::test]
async fn test_subscription_receiver_drop_cancellation() {
    let state = Arc::new(MockState::default());
    let connector =
        MockServerConnector::with_state(state.clone()).with_tag_values(vec![OpcValue::Int(42)]);

    let client = OpcDaClient::builder()
        .server("Mock.DropSub.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    let mut rx = client.subscribe(["Sensor.Cancel"], Duration::from_millis(15));

    // Receive initial tick
    let _ = tokio::time::timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("tick 1 timeout")
        .expect("tick 1 channel open");

    // Explicitly drop receiver
    drop(rx);

    // Yield execution and allow background task to detect channel disconnect
    tokio::time::sleep(Duration::from_millis(60)).await;
    let count_after_drop = state.read_count.load(Ordering::Relaxed);
    assert!(
        count_after_drop >= 1,
        "At least one read must occur prior to cancellation"
    );

    // Sleep further to verify background task is truly terminated
    tokio::time::sleep(Duration::from_millis(50)).await;
    let count_final = state.read_count.load(Ordering::Relaxed);

    assert_eq!(
        count_after_drop, count_final,
        "Background polling task must stop reading after receiver is dropped"
    );
}

#[tokio::test]
async fn test_subscription_transient_error_resilience() {
    let state = Arc::new(MockState::default());
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let connector =
        MockServerConnector::with_state(state.clone()).with_read_fn(move |_source, handles| {
            let n = call_count_clone.fetch_add(1, Ordering::Relaxed);
            if n == 1 || n == 2 {
                // Fails both the cached read and the subsequent internal retry
                // in handle_read, forcing client.read_tags to return Err to subscribe
                Err(OpcError::from(E_FAIL))
            } else {
                // Call 0 (tick 0) and Call 3+ (tick 2) succeed
                Ok(handles
                    .iter()
                    .map(|&h| {
                        Ok(GroupItemState {
                            client_handle: ClientItemHandle::new(h.as_raw()),
                            value: OpcValue::Int(100 + i64::from(n)),
                            quality: OpcQuality::GOOD,
                            timestamp: std::time::SystemTime::UNIX_EPOCH,
                        })
                    })
                    .collect())
            }
        });

    let client = OpcDaClient::builder()
        .server("Mock.TransientSub.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    let mut rx = client.subscribe(["Sensor.Resilient"], Duration::from_millis(15));

    // Tick 0: Success
    let tick0 = tokio::time::timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("tick 0 timeout")
        .expect("tick 0 channel open");
    assert_eq!(tick0.get_i32("Sensor.Resilient"), Ok(100));

    // Tick 1 encounters E_FAIL, loop logs warning and continues without terminating.
    // Next successful read delivers tick 2:
    let tick2 = tokio::time::timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("tick 2 timeout")
        .expect("subscription channel must remain open despite transient error");
    assert!(tick2.get_i32("Sensor.Resilient").is_ok());
}

#[tokio::test]
async fn test_subscription_connection_error_termination() {
    let state = Arc::new(MockState::default());
    let call_count = Arc::new(AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    let connector =
        MockServerConnector::with_state(state.clone()).with_read_fn(move |_source, handles| {
            let n = call_count_clone.fetch_add(1, Ordering::Relaxed);
            if n >= 1 {
                // Call 1+ fails with unrecoverable RPC_S_SERVER_UNAVAILABLE (connection error)
                Err(OpcError::from(RPC_S_SERVER_UNAVAILABLE))
            } else {
                Ok(handles
                    .iter()
                    .map(|&h| {
                        Ok(GroupItemState {
                            client_handle: ClientItemHandle::new(h.as_raw()),
                            value: OpcValue::Int(1),
                            quality: OpcQuality::GOOD,
                            timestamp: std::time::SystemTime::UNIX_EPOCH,
                        })
                    })
                    .collect())
            }
        });

    let client = OpcDaClient::builder()
        .server("Mock.DisconnectSub.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    let mut rx = client.subscribe(["Sensor.Disconnect"], Duration::from_millis(15));

    // Tick 0: Success
    let tick0 = tokio::time::timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("tick 0 timeout")
        .expect("tick 0 channel open");
    assert_eq!(tick0.get_i32("Sensor.Disconnect"), Ok(1));

    // Tick 1 encounters connection error: polling loop must terminate immediately
    let next_tick = tokio::time::timeout(Duration::from_millis(250), rx.recv())
        .await
        .expect("channel closure timeout");

    assert!(
        next_tick.is_none(),
        "Subscription channel must close (return None) upon encountering connection error"
    );
}

#[tokio::test]
async fn test_subscription_zero_allocation_batch_sharing() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone())
        .with_tag_values(vec![OpcValue::Int(1), OpcValue::Int(2)]);

    let client = OpcDaClient::builder()
        .server("Mock.Shareable.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    // 1. Subscribe with fixed-size array [&str; 2]
    let mut rx_array = client.subscribe(["Tag.A", "Tag.B"], Duration::from_millis(15));
    let tick_array = tokio::time::timeout(Duration::from_millis(250), rx_array.recv())
        .await
        .expect("array sub timeout")
        .expect("array sub channel open");
    assert_eq!(tick_array.len(), 2);

    // 2. Subscribe with slice &[&str]
    let tags_slice: &[&str] = &["Tag.A", "Tag.B"];
    let mut rx_slice = client.subscribe(tags_slice, Duration::from_millis(15));
    let tick_slice = tokio::time::timeout(Duration::from_millis(250), rx_slice.recv())
        .await
        .expect("slice sub timeout")
        .expect("slice sub channel open");
    assert_eq!(tick_slice.len(), 2);

    // 3. Subscribe with owned Vec<String>
    let tags_vec: Vec<String> = vec!["Tag.A".to_string(), "Tag.B".to_string()];
    let mut rx_vec = client.subscribe(tags_vec, Duration::from_millis(15));
    let tick_vec = tokio::time::timeout(Duration::from_millis(250), rx_vec.recv())
        .await
        .expect("vec sub timeout")
        .expect("vec sub channel open");
    assert_eq!(tick_vec.len(), 2);
}
