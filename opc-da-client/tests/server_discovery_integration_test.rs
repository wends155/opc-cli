//! Integration tests for OPC DA server discovery and catalog inspection.
//!
//! Exercises public `OpcDaClient` facade and `ServerDiscovery` trait contracts
//! using `MockServerConnector` without Windows COM dependencies.

use opc_da_client::connector::{MockServerConnector, MockState};
use opc_da_client::types::{Clsid, OpcServerInfo, ServerIdentifier};
use opc_da_client::{
    Bound, OpcDaClient, OpcError, OpcProvider, OpcResult, ServerDiscovery, Unbound,
};
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn test_server_discovery_local_and_remote_enumeration() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone()).with_servers(vec![
        "Vendor.Server.1".to_string(),
        "Vendor.Server.2".to_string(),
    ]);

    let client = OpcDaClient::builder()
        .with_connector(connector)
        .build()
        .expect("building unbound client must succeed");

    // 1. Local server enumeration
    let local_servers = client
        .list_servers("localhost")
        .await
        .expect("list_servers on localhost should succeed");
    assert_eq!(local_servers, vec!["Vendor.Server.1", "Vendor.Server.2"]);
    assert_eq!(state.last_enumerated_host().as_deref(), Some("localhost"));

    // 2. Remote server enumeration
    let remote_servers = client
        .list_servers("192.168.1.100")
        .await
        .expect("list_servers on remote host should succeed");
    assert_eq!(remote_servers, vec!["Vendor.Server.1", "Vendor.Server.2"]);
    assert_eq!(
        state.last_enumerated_host().as_deref(),
        Some("192.168.1.100")
    );
}

#[tokio::test]
async fn test_server_discovery_structured_metadata_inspection() {
    let state = Arc::new(MockState::default());
    let clsid = Clsid::from_u128(0x01234567_89AB_CDEF_0123_456789ABCDEF);
    let details = vec![OpcServerInfo::new(
        "Acme.ScadaServer.DA.1",
        clsid,
        Some("Acme SCADA OPC DA Server".to_string()),
        Some("10.0.0.50".to_string()),
    )];

    let connector = MockServerConnector::with_state(state.clone()).with_server_details(details);

    let client = OpcDaClient::builder()
        .with_connector(connector)
        .build()
        .expect("building unbound client must succeed");

    let inspected = client
        .list_server_details("10.0.0.50")
        .await
        .expect("list_server_details should succeed");

    assert_eq!(inspected.len(), 1);
    let server_info = &inspected[0];
    assert_eq!(server_info.prog_id(), "Acme.ScadaServer.DA.1");
    assert_eq!(server_info.clsid(), clsid);
    assert_eq!(server_info.user_type(), Some("Acme SCADA OPC DA Server"));
    assert_eq!(server_info.display_name(), "Acme SCADA OPC DA Server");
    assert_eq!(server_info.host(), Some("10.0.0.50"));

    let ep = server_info.endpoint();
    assert_eq!(ep.host(), Some("10.0.0.50"));
    assert_eq!(
        ep.identifier(),
        &ServerIdentifier::try_from("Acme.ScadaServer.DA.1").unwrap()
    );
    assert_eq!(state.last_enumerated_host().as_deref(), Some("10.0.0.50"));
}

#[tokio::test]
async fn test_server_discovery_trait_polymorphism() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone())
        .with_servers(vec!["Server.Alpha".to_string(), "Server.Beta".to_string()]);

    let unbound: OpcDaClient<MockServerConnector, Unbound> = OpcDaClient::builder()
        .with_connector(connector.clone())
        .build()
        .expect("building unbound client must succeed");

    let bound: OpcDaClient<MockServerConnector, Bound> = OpcDaClient::builder()
        .server("Server.Alpha")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    async fn discover_servers<D: ServerDiscovery>(d: &D, host: &str) -> OpcResult<Vec<String>> {
        d.list_servers(host).await
    }

    async fn discover_details<P: OpcProvider>(p: &P, host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        p.list_server_details(host).await
    }

    // 1. Verify Unbound gateway conforms to ServerDiscovery and OpcProvider
    let unbound_servers = discover_servers(&unbound, "host-unbound")
        .await
        .expect("unbound discovery");
    assert_eq!(unbound_servers, vec!["Server.Alpha", "Server.Beta"]);

    let unbound_details = discover_details(&unbound, "host-unbound")
        .await
        .expect("unbound provider details");
    assert_eq!(unbound_details.len(), 2);

    // 2. Verify Bound session conforms to ServerDiscovery and OpcProvider
    let bound_servers = discover_servers(&bound, "host-bound")
        .await
        .expect("bound discovery");
    assert_eq!(bound_servers, vec!["Server.Alpha", "Server.Beta"]);

    let bound_details = discover_details(&bound, "host-bound")
        .await
        .expect("bound provider details");
    assert_eq!(bound_details.len(), 2);
}

#[tokio::test]
async fn test_server_discovery_connection_failure_simulation() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone())
        .with_servers(vec!["Mock.Server.1".to_string()]);

    let client = OpcDaClient::builder()
        .with_connector(connector)
        .build()
        .expect("building unbound client must succeed");

    // Simulate enumeration failure
    state.should_fail_connect.store(true, Ordering::Relaxed);

    let servers_err = client.list_servers("failing-host").await;
    assert!(
        servers_err.is_err(),
        "list_servers must fail when should_fail_connect is set"
    );
    assert!(matches!(servers_err.unwrap_err(), OpcError::Internal(_)));

    let details_err = client.list_server_details("failing-host").await;
    assert!(
        details_err.is_err(),
        "list_server_details must fail when should_fail_connect is set"
    );
    assert!(matches!(details_err.unwrap_err(), OpcError::Internal(_)));

    // Reset failure injection — verify client self-healing / recovery
    state.should_fail_connect.store(false, Ordering::Relaxed);

    let recovered = client
        .list_servers("recovered-host")
        .await
        .expect("enumeration should succeed after failure cleared");
    assert_eq!(recovered, vec!["Mock.Server.1"]);
    assert_eq!(
        state.last_enumerated_host().as_deref(),
        Some("recovered-host")
    );
}

#[tokio::test]
async fn test_server_discovery_empty_catalog() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone()).with_servers(vec![]);

    let client = OpcDaClient::builder()
        .with_connector(connector)
        .build()
        .expect("building unbound client must succeed");

    let servers = client
        .list_servers("localhost")
        .await
        .expect("list_servers on empty catalog should succeed");
    assert!(
        servers.is_empty(),
        "Empty server catalog must return empty Vec, not error"
    );

    let details = client
        .list_server_details("localhost")
        .await
        .expect("list_server_details on empty catalog should succeed");
    assert!(
        details.is_empty(),
        "Empty catalog details must return empty Vec"
    );
    assert_eq!(state.last_enumerated_host().as_deref(), Some("localhost"));
}
