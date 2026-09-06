use opc_da_client::{
    Bound, MockOpcDaClient, OpcDaClient, OpcProvider, OpcServerEndpoint, OpcValue, Unbound,
};
use std::sync::Arc;

#[tokio::test]
async fn test_client_typestate_bind_and_unbind() {
    // 1. Unbound client instantiation
    let unbound: OpcDaClient<_, Unbound> = MockOpcDaClient::default();
    assert!(unbound.endpoint().is_none());

    // 2. Transition to Bound typestate
    let endpoint = OpcServerEndpoint::from("Matrikon.OPC.Simulation.1");
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
    let rebound = restored_unbound.bind("Another.Server.1");
    assert_eq!(rebound.server_id(), "Another.Server.1");
}

#[tokio::test]
async fn test_typestate_implements_opc_provider() {
    let unbound = MockOpcDaClient::default();
    let provider_unbound: Arc<dyn OpcProvider> = Arc::new(unbound);
    assert!(provider_unbound.list_servers("localhost").await.is_ok());

    let bound = MockOpcDaClient::default().bind("Matrikon.OPC.Simulation.1");
    let provider_bound: Arc<dyn OpcProvider> = Arc::new(bound);
    assert!(provider_bound.list_servers("localhost").await.is_ok());
}
