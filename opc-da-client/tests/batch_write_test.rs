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

    let provider: &dyn OpcProvider = &client;
    let results = provider
        .write_tag_values("Mock.Server.1", &writes)
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
        "write_tag_values must dispatch a single atomic batch write group, not N individual groups"
    );
}
