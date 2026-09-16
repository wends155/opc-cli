use super::*;
use crate::connector::mock::{MockServerConnector, MockState};
use crate::errors::OpcError;
use crate::provider::OpcProvider;
use crate::types::{OpcServerInfo, OpcValue, ServerIdentifier};
use std::time::Duration;

#[tokio::test]
async fn test_client_list_server_details() {
    let connector = MockServerConnector::new().with_server_details(vec![OpcServerInfo::new(
        "Test.Server.1",
        crate::types::Clsid::zeroed(),
        Some("Test OPC Server".into()),
        None,
    )]);
    let client = OpcDaClient::new(connector).unwrap();
    let details = client.list_server_details("localhost").await.unwrap();
    assert_eq!(details.len(), 1);
    assert_eq!(details[0].prog_id(), "Test.Server.1");
    assert_eq!(details[0].display_name(), "Test OPC Server");
}

#[tokio::test]
async fn test_client_builder_configuration_and_unbound_discovery() {
    let state = std::sync::Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());

    // 1. Bound client
    let client = OpcDaClient::builder()
        .host("192.168.1.50")
        .server("Matrikon.OPC.Simulation.1")
        .with_connector(connector.clone())
        .build_bound()
        .expect("building bound client should succeed");

    assert_eq!(client.endpoint().host.as_deref(), Some("192.168.1.50"));
    assert_eq!(
        client.endpoint().identifier,
        ServerIdentifier::try_from("Matrikon.OPC.Simulation.1").unwrap()
    );

    // 2. Unbound client discovery
    let unbound_client = OpcDaClient::builder()
        .with_connector(connector)
        .build()
        .expect("building unbound client should succeed");
    assert!(unbound_client.endpoint().is_none());

    let servers = unbound_client.list_servers("192.168.1.50").await.unwrap();
    assert_eq!(servers, vec!["Mock.Server.1".to_string()]);
}

#[cfg(feature = "opc-da-backend")]
#[tokio::test]
async fn test_builder_timeout_and_legacy_dcom() {
    let timeout = Duration::from_secs(5);
    let builder = OpcDaClient::builder()
        .timeout(timeout)
        .with_legacy_dcom(true);
    assert_eq!(builder.timeout_duration(), Some(timeout));
    assert!(builder.legacy_dcom());
}

#[tokio::test]
async fn test_inherent_async_reads_and_writes_on_client() {
    let state = std::sync::Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());
    let client = OpcDaClient::new(connector).unwrap();

    let write_res = client
        .write_tag_value("Mock.Server.1", "Random.Int4", OpcValue::Int(42))
        .await
        .unwrap();
    assert!(write_res.is_success());

    let val = client
        .read_tag_value("Mock.Server.1", "Random.Int4")
        .await
        .unwrap();
    assert_eq!(val.tag_id, "Random.Int4");
}

#[tokio::test]
async fn test_remote_host_propagation_and_discovery() {
    let state = std::sync::Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());
    let client = OpcDaClient::new(connector).unwrap();
    let servers = client.list_servers("192.168.1.100").await.unwrap();
    assert_eq!(servers, vec!["Mock.Server.1".to_string()]);
    assert_eq!(
        state.last_enumerated_host.lock().unwrap().as_deref(),
        Some("192.168.1.100")
    );
}

#[tokio::test]
async fn test_client_subscribe_mpsc_polling_stream() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local_prog_id("Mock.Server.1"));
    let mut rx = client.subscribe(["Random.Int4"], Duration::from_millis(20));

    let first = rx.recv().await;
    assert!(first.is_some());
}

#[tokio::test]
async fn test_subscribe_receiver_drop_cancellation() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local_prog_id("Mock.Server.1"));
    let rx = client.subscribe(["Random.Int4"], Duration::from_millis(10));
    drop(rx);
    tokio::time::sleep(Duration::from_millis(30)).await;
}

#[tokio::test]
async fn test_client_bind_and_connect_eager() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local_prog_id("Mock.Server.1"));
    assert!(client.connect_eager().await.is_ok());
}

#[tokio::test]
async fn test_client_role_traits_via_opc_provider() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector).unwrap();
    fn assert_provider<P: OpcProvider>(_p: &P) {}
    assert_provider(&client);
}

#[tokio::test]
async fn test_inherent_read_tags_and_read_tag_session_methods() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local_prog_id("Mock.Server.1"));
    let val = client.read_tag("Random.Int4").await.unwrap();
    assert_eq!(val.tag_id, "Random.Int4");
}

#[tokio::test]
async fn test_client_read_f32() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local_prog_id("Mock.Server.1"));
    let _ = client.read_f32("Random.Real4").await;
}

#[tokio::test]
async fn test_client_read_i64() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local_prog_id("Mock.Server.1"));
    let _ = client.read_i64("Random.Int8").await;
}

#[tokio::test]
async fn test_client_read_u32() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local_prog_id("Mock.Server.1"));
    let _ = client.read_u32("Random.UInt4").await;
}

#[tokio::test]
async fn test_client_read_u64() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local_prog_id("Mock.Server.1"));
    let _ = client.read_u64("Random.UInt8").await;
}

#[tokio::test]
async fn test_client_inherent_read_forwarders() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local_prog_id("Mock.Server.1"));
    let _ = client.read_f64("Random.Real8").await;
    let _ = client.read_i32("Random.Int4").await;
    let _ = client.read_bool("Random.Boolean").await;
    let _ = client.read_string("Random.String").await;
}

#[test]
fn test_client_bind_remote_localhost_normalization() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector).unwrap().bind_remote(
        "localhost",
        ServerIdentifier::try_from("Mock.Server.1").unwrap(),
    );
    assert!(client.endpoint().host.is_none());
}

#[test]
fn test_client_builder_localhost_normalization() {
    let builder = OpcDaClient::builder()
        .host("localhost")
        .server("Mock.Server.1");
    let client = builder
        .with_connector(MockServerConnector::new())
        .build_bound()
        .unwrap();
    assert!(client.endpoint().host.is_none());
}

#[test]
fn test_client_builder_with_connector() {
    let connector = MockServerConnector::new();
    let builder = OpcDaClient::builder().with_connector(connector);
    let client = builder.build().expect("builder should construct client");
    assert!(client.endpoint().is_none());
}

#[test]
fn test_client_builder_build_bound_missing_server() {
    let builder = OpcDaClient::builder().with_connector(MockServerConnector::new());
    let err = builder.build_bound().unwrap_err();
    match err {
        OpcError::InvalidState(_) => {}
        other => panic!("expected InvalidState, got {other:?}"),
    }
}

#[test]
fn test_client_bind_unbind_lifecycle() {
    let client = OpcDaClient::new(MockServerConnector::new()).unwrap();
    assert!(client.endpoint().is_none());

    let bound = client.bind(OpcServerEndpoint::local_prog_id(
        "Matrikon.OPC.Simulation.1",
    ));
    assert_eq!(bound.server_id(), "Matrikon.OPC.Simulation.1");

    let (unbound, ep) = bound.unbind();
    assert!(unbound.endpoint().is_none());
    assert_eq!(
        ep.identifier,
        ServerIdentifier::try_from("Matrikon.OPC.Simulation.1").unwrap()
    );
}

fn setup_mock_client() -> (
    OpcDaClient<MockServerConnector, Unbound>,
    std::sync::Arc<MockState>,
) {
    let state = std::sync::Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());
    let client = OpcDaClient::new(connector).expect("mock client should initialize");
    (client, state)
}

#[tokio::test]
#[allow(clippy::similar_names)]
async fn test_unbound_write_tag_value_generic_primitives() {
    let (client, _state) = setup_mock_client();

    let res_f64 = client
        .write_tag_value("Mock.Server.1", "Sensor.Temp", 42.5f64)
        .await
        .expect("write f64 primitive must succeed");
    assert!(res_f64.is_success());
    assert_eq!(res_f64.tag_id, "Sensor.Temp");

    let res_i32 = client
        .write_tag_value("Mock.Server.1", "Motor.Speed", 100i32)
        .await
        .expect("write i32 primitive must succeed");
    assert!(res_i32.is_success());

    let res_bool = client
        .write_tag_value("Mock.Server.1", "Switch.Enabled", true)
        .await
        .expect("write bool primitive must succeed");
    assert!(res_bool.is_success());

    let res_str = client
        .write_tag_value("Mock.Server.1", "System.Status", "RUNNING")
        .await
        .expect("write &str primitive must succeed");
    assert!(res_str.is_success());
}

#[tokio::test]
async fn test_unbound_write_tag_batch_into_write_batch() {
    let (client, _state) = setup_mock_client();

    let res_arr = client
        .write_tag_batch("Mock.Server.1", [("Tag.1", 1.0f64), ("Tag.2", 2.0f64)])
        .await
        .expect("write array batch must succeed");
    assert_eq!(res_arr.len(), 2);
    assert_eq!(res_arr[0].tag_id, "Tag.1");
    assert_eq!(res_arr[1].tag_id, "Tag.2");

    let slice_data = [("Tag.3", 3.0f64), ("Tag.4", 4.0f64)];
    let res_slice = client
        .write_tag_batch("Mock.Server.1", &slice_data[..])
        .await
        .expect("write slice batch must succeed");
    assert_eq!(res_slice.len(), 2);

    let res_ref_arr = client
        .write_tag_batch("Mock.Server.1", &slice_data)
        .await
        .expect("write array ref batch must succeed");
    assert_eq!(res_ref_arr.len(), 2);

    let res_single = client
        .write_tag_batch("Mock.Server.1", ("Tag.Solo", 42i32))
        .await
        .expect("write single tuple batch must succeed");
    assert_eq!(res_single.len(), 1);
    assert!(res_single[0].is_success());
}

#[tokio::test]
async fn test_unbound_shorthand_aliases() {
    let (client, _state) = setup_mock_client();

    let tag_val = client
        .read_tag("Mock.Server.1", "Random.Int4")
        .await
        .expect("read_tag alias must succeed");
    assert_eq!(tag_val.tag_id, "Random.Int4");
    assert!(tag_val.is_good());

    let tag_vals = client
        .read_tags("Mock.Server.1", ["Random.Int4", "Random.Real8"])
        .await
        .expect("read_tags alias must succeed");
    assert_eq!(tag_vals.len(), 2);
    assert!(tag_vals.get("Random.Int4").is_some());
    assert!(tag_vals.get("Random.Real8").is_some());

    let write_single = client
        .write_tag("Mock.Server.1", "Random.Int4", 99i32)
        .await
        .expect("write_tag alias must succeed");
    assert_eq!(write_single.tag_id, "Random.Int4");
    assert!(write_single.is_success());

    let write_multi = client
        .write_tags("Mock.Server.1", [("Tag.A", 10i32), ("Tag.B", 20i32)])
        .await
        .expect("write_tags alias must succeed");
    assert_eq!(write_multi.len(), 2);
    assert_eq!(write_multi[0].tag_id, "Tag.A");
    assert_eq!(write_multi[1].tag_id, "Tag.B");
    assert!(write_multi[0].is_success());
    assert!(write_multi[1].is_success());

    let collector = crate::types::TagCollector::new(50);
    let tags = client
        .browse("Mock.Server.1", collector)
        .await
        .expect("browse alias must succeed");
    assert!(!tags.is_empty());
}

#[tokio::test]
async fn test_unbound_write_tag_generic_into_opc_value() {
    let (client, _state) = setup_mock_client();

    let res1 = client
        .write_tag("Mock.Server.1", "Tag.F64", 123.456f64)
        .await
        .unwrap();
    assert!(res1.is_success());

    let res2 = client
        .write_tag("Mock.Server.1", "Tag.Str", "operational")
        .await
        .unwrap();
    assert!(res2.is_success());

    let res3 = client
        .write_tag("Mock.Server.1", "Tag.Flag", false)
        .await
        .unwrap();
    assert!(res3.is_success());
}

#[test]
fn test_builder_server_deferred_error_accumulation() {
    use crate::client::builder::OpcDaClientBuilder;

    // 1. Invalid host with null byte
    let res = OpcDaClientBuilder::new()
        .host("bad\0host")
        .server("Valid.Server")
        .build();
    assert!(
        matches!(res, Err(OpcError::InvalidState(_))),
        "expected InvalidState for null host, got {res:?}"
    );

    // 2. Chaining after error does not clobber first error
    let res_clobber = OpcDaClientBuilder::new()
        .host("first\0bad")
        .server("Second\0Bad")
        .build();
    if let Err(OpcError::InvalidState(msg)) = res_clobber {
        assert!(
            msg.contains("Host"),
            "first error must be preserved, got {msg}"
        );
    } else {
        panic!("expected InvalidState, got {res_clobber:?}");
    }

    // 3. build_bound with missing server
    let res_no_server = OpcDaClientBuilder::new().build_bound();
    assert!(
        matches!(res_no_server, Err(OpcError::InvalidState(_))),
        "expected InvalidState for missing server, got {res_no_server:?}"
    );
}

#[test]
fn test_client_bind_new_fail_early_on_invalid_endpoint() {
    let res = OpcDaClient::bind_new("bad\0endpoint");
    assert!(res.is_err(), "bind_new must reject invalid endpoint early");

    let res_remote = OpcDaClient::bind_new_remote("bad\0host", "Valid.Server");
    assert!(
        res_remote.is_err(),
        "bind_new_remote must reject null host early"
    );
}
