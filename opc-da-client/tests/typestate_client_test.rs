use opc_da_client::{
    Bound, MockOpcDaClient, OpcDaClient, OpcProvider, OpcServerEndpoint, OpcValue, ServerDiscovery,
    Unbound,
};

#[tokio::test]
async fn test_client_typestate_bind_and_unbind() {
    // 1. Unbound client instantiation
    let unbound: OpcDaClient<_, Unbound> = MockOpcDaClient::default();
    assert!(unbound.endpoint().is_none());

    // 2. Transition to Bound typestate
    let endpoint = OpcServerEndpoint::local("Matrikon.OPC.Simulation.1");
    let bound: OpcDaClient<_, Bound> = unbound.bind(endpoint.clone());

    // 3. Bound state guarantees non-optional endpoint and server_id
    assert_eq!(bound.server_id(), "Matrikon.OPC.Simulation.1");
    assert_eq!(bound.endpoint(), &endpoint);

    // 4. Inherent single-server operations on Bound state
    let read_res = bound.read_tags(["Random.Int4"]).await;
    assert!(read_res.is_ok());

    let single_read = bound.read_tag("Random.Int4").await;
    assert!(single_read.is_ok());

    let write_res = bound.write_tag("Random.Int4", OpcValue::Int(42)).await;
    assert!(write_res.is_ok());

    let batch_write = bound
        .write_tags(vec![("Random.Int4".to_string(), OpcValue::Int(42))])
        .await;
    assert!(batch_write.is_ok());

    // 5. Unbind restores Unbound typestate
    let (restored_unbound, returned_ep): (OpcDaClient<_, Unbound>, OpcServerEndpoint) =
        bound.unbind();
    assert_eq!(returned_ep, endpoint);
    assert!(restored_unbound.endpoint().is_none());

    // 6. Restored client can be rebound
    let rebound = restored_unbound.bind(OpcServerEndpoint::local("Another.Server.1"));
    assert_eq!(rebound.server_id(), "Another.Server.1");
}

#[tokio::test]
async fn test_typestate_implements_opc_provider() {
    fn assert_provider<P: OpcProvider>(_p: &P) {}

    let unbound = MockOpcDaClient::default();
    assert_provider(&unbound);
    assert!(unbound.list_servers("localhost").await.is_ok());

    let bound =
        MockOpcDaClient::default().bind(OpcServerEndpoint::local("Matrikon.OPC.Simulation.1"));
    assert_provider(&bound);
    assert!(bound.list_servers("localhost").await.is_ok());
}

#[tokio::test]
async fn test_server_id_returns_formatted_guid_for_clsid_endpoint() {
    let clsid = opc_da_client::Clsid::from_u128(0x13486D51_4821_11D2_A494_3CB306C10000);
    let endpoint = OpcServerEndpoint::local(clsid);
    let bound = MockOpcDaClient::default().bind(endpoint);
    let sid = bound.server_id();
    assert!(sid.starts_with('{'), "Expected GUID format, got: {sid}");
    assert!(sid.ends_with('}'), "Expected GUID format, got: {sid}");
    assert_ne!(sid, "{CLSID}", "Placeholder not replaced: {sid}");
}

#[tokio::test]
async fn test_typestate_failure_recovery_and_rebind() {
    use opc_da_client::connector::MockServerConnector;

    let connector = MockServerConnector::default();

    // 1. Bound client on Server A
    let bound_a = OpcDaClient::builder()
        .server("Mock.Server.Primary")
        .with_connector(connector)
        .build_bound()
        .expect("bound client on primary server");

    assert_eq!(bound_a.server_id(), "Mock.Server.Primary");

    // 2. Perform operation on Server A
    let read_a = bound_a
        .read_tag("Primary.Tag")
        .await
        .expect("read from primary server");
    assert!(read_a.outcome.is_ok());

    // 3. Unbind from Server A upon failover or migration
    let (unbound, previous_ep) = bound_a.unbind();
    assert_eq!(
        previous_ep.identifier().as_prog_id(),
        Some("Mock.Server.Primary")
    );
    assert!(unbound.endpoint().is_none());

    // 4. Rebind to Standby Server B on the same client worker instance
    let bound_b = unbound.bind(OpcServerEndpoint::local("Mock.Server.Standby"));
    assert_eq!(bound_b.server_id(), "Mock.Server.Standby");

    // 5. Successful operations on Standby Server B
    let read_b = bound_b
        .read_tag("Standby.Tag")
        .await
        .expect("read from standby server");
    assert!(read_b.outcome.is_ok());
}
