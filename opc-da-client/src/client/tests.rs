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
        ServerIdentifier::from("Matrikon.OPC.Simulation.1")
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
        .bind(OpcServerEndpoint::local("Mock.Server.1"));
    let mut rx = client.subscribe(["Random.Int4"], Duration::from_millis(20));

    let first = rx.recv().await;
    assert!(first.is_some());
}

#[tokio::test]
async fn test_subscribe_receiver_drop_cancellation() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local("Mock.Server.1"));
    let rx = client.subscribe(["Random.Int4"], Duration::from_millis(10));
    drop(rx);
    tokio::time::sleep(Duration::from_millis(30)).await;
}

#[tokio::test]
async fn test_client_bind_and_connect_eager() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local("Mock.Server.1"));
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
        .bind(OpcServerEndpoint::local("Mock.Server.1"));
    let val = client.read_tag("Random.Int4").await.unwrap();
    assert_eq!(val.tag_id, "Random.Int4");
}

#[tokio::test]
async fn test_client_read_f32() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local("Mock.Server.1"));
    let _ = client.read_f32("Random.Real4").await;
}

#[tokio::test]
async fn test_client_read_i64() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local("Mock.Server.1"));
    let _ = client.read_i64("Random.Int8").await;
}

#[tokio::test]
async fn test_client_read_u32() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local("Mock.Server.1"));
    let _ = client.read_u32("Random.UInt4").await;
}

#[tokio::test]
async fn test_client_read_u64() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local("Mock.Server.1"));
    let _ = client.read_u64("Random.UInt8").await;
}

#[tokio::test]
async fn test_client_inherent_read_forwarders() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind(OpcServerEndpoint::local("Mock.Server.1"));
    let _ = client.read_f64("Random.Real8").await;
    let _ = client.read_i32("Random.Int4").await;
    let _ = client.read_bool("Random.Boolean").await;
    let _ = client.read_string("Random.String").await;
}

#[test]
fn test_client_bind_remote_localhost_normalization() {
    let connector = MockServerConnector::new();
    let client = OpcDaClient::new(connector)
        .unwrap()
        .bind_remote("localhost", "Mock.Server.1");
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

    let bound = client.bind(OpcServerEndpoint::local("Matrikon.OPC.Simulation.1"));
    assert_eq!(bound.server_id(), "Matrikon.OPC.Simulation.1");

    let (unbound, ep) = bound.unbind();
    assert!(unbound.endpoint().is_none());
    assert_eq!(
        ep.identifier,
        ServerIdentifier::from("Matrikon.OPC.Simulation.1")
    );
}
