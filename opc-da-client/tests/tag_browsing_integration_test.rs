//! Integration tests for OPC DA namespace browsing, TagCollector limits, and RAII cursor safety.
//!
//! Exercises public `OpcDaClient` facade and `TagBrowser` trait contracts
//! using `MockServerConnector` without Windows COM dependencies.

use opc_da_client::connector::{MockConnectedServer, MockServerConnector, MockState};
use opc_da_client::types::{BrowseDirection, NamespaceType, ServerIdentifier, TagCollector};
use opc_da_client::{OpcDaClient, OpcError, TagBrowser};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_tag_browsing_flat_namespace() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone()).with_tags(vec![
        "Pump.Speed".to_string(),
        "Valve.State".to_string(),
        "Tank.Level".to_string(),
    ]);
    connector
        .server
        .organization
        .store(NamespaceType::Flat as u32, Ordering::Relaxed);

    let client = OpcDaClient::builder()
        .with_connector(connector)
        .build()
        .expect("building unbound client must succeed");

    let collector = TagCollector::new(50);
    let tags = client
        .browse_tags("Mock.Flat.Server", collector.clone())
        .await
        .expect("browse_tags on flat namespace should succeed");

    assert_eq!(tags, vec!["Pump.Speed", "Valve.State", "Tank.Level"]);
    assert_eq!(collector.snapshot(), tags);
    assert_eq!(collector.len(), 3);
    assert_eq!(
        state.change_browse_position_count.load(Ordering::Relaxed),
        0,
        "Flat namespace must not navigate branches via change_browse_position"
    );
}

#[tokio::test]
async fn test_tag_browsing_hierarchical_fast_flat() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone()).with_tags(vec![
        "PLC1.DeviceA.Sensor1".to_string(),
        "PLC1.DeviceA.Sensor2".to_string(),
    ]);
    connector
        .server
        .organization
        .store(NamespaceType::Hierarchy as u32, Ordering::Relaxed);
    connector
        .server
        .supports_flat_browse
        .store(true, Ordering::Relaxed);

    let client = OpcDaClient::builder()
        .with_connector(connector)
        .build()
        .expect("building unbound client must succeed");

    let collector = TagCollector::new(50);
    let tags = client
        .browse_tags("Mock.Hierarchical.Server", collector.clone())
        .await
        .expect("fast-flat browse should succeed");

    assert_eq!(tags, vec!["PLC1.DeviceA.Sensor1", "PLC1.DeviceA.Sensor2"]);
    assert_eq!(collector.snapshot(), tags);
    assert_eq!(collector.len(), 2);
    assert_eq!(
        state.change_browse_position_count.load(Ordering::Relaxed),
        0,
        "Hierarchical fast flat enumeration must bypass branch traversal"
    );
}

#[tokio::test]
async fn test_tag_browsing_hierarchical_recursive_walk() {
    let state = Arc::new(MockState::default());

    let branch_tags = Arc::new(Mutex::new(vec!["Area1".to_string()]));
    let branch_tags_clone = branch_tags.clone();
    let tags = Arc::new(Mutex::new(vec!["Root.Status".to_string()]));
    let tags_clone = tags.clone();
    let visit_count = Arc::new(AtomicUsize::new(0));
    let visit_count_clone = visit_count.clone();

    let server = Arc::new(MockConnectedServer {
        state: state.clone(),
        tags: tags.clone(),
        branch_tags: branch_tags.clone(),
        organization: AtomicU32::new(NamespaceType::Hierarchy as u32),
        supports_flat_browse: AtomicBool::new(false), // Disable flat to force recursive walk
        get_item_id_fn: Some(Arc::new(move |leaf_name| {
            let count = visit_count_clone.fetch_add(1, Ordering::Relaxed);
            if count == 0 {
                // Depth 0: leaf "Root.Status". Update leaves for depth 1 child branch.
                *tags_clone.lock().unwrap() = vec!["Temperature".to_string()];
                Ok(leaf_name.to_string())
            } else {
                // Depth 1: leaf "Temperature". Clear branch_tags so depth 1 terminates cleanly.
                branch_tags_clone.lock().unwrap().clear();
                Ok(format!("Area1.{leaf_name}"))
            }
        })),
        ..Default::default()
    });

    let connector = MockServerConnector {
        server,
        state: state.clone(),
        ..Default::default()
    };

    let client = OpcDaClient::builder()
        .with_connector(connector)
        .build()
        .expect("building unbound client must succeed");

    let collector = TagCollector::new(50);
    let tags = client
        .browse_tags("Mock.Hierarchical.Server", collector.clone())
        .await
        .expect("hierarchical recursive browse should succeed");

    assert_eq!(tags, vec!["Root.Status", "Area1.Temperature"]);
    assert_eq!(collector.snapshot(), tags);
    assert_eq!(collector.len(), 2);

    // RAII BrowsePositionGuard down/up verification: exactly 1 Down into Area1, 1 Up back to root
    assert_eq!(
        state.change_browse_position_count.load(Ordering::Relaxed),
        2,
        "Recursive branch walk must execute exactly 1 Down and 1 Up navigation"
    );
    assert_eq!(
        state.last_browse_direction(),
        Some(BrowseDirection::Up),
        "Browse position must return to parent root upon completion"
    );
}

#[tokio::test]
async fn test_tag_browsing_collector_limits_and_cancellation() {
    let state = Arc::new(MockState::default());
    let all_tags = vec![
        "Tag.1".to_string(),
        "Tag.2".to_string(),
        "Tag.3".to_string(),
        "Tag.4".to_string(),
        "Tag.5".to_string(),
    ];
    let connector = MockServerConnector::with_state(state.clone()).with_tags(all_tags);
    connector
        .server
        .organization
        .store(NamespaceType::Flat as u32, Ordering::Relaxed);

    let client = OpcDaClient::builder()
        .with_connector(connector)
        .build()
        .expect("building unbound client must succeed");

    // 1. Capacity limit enforcement
    let bounded_collector = TagCollector::new(3);
    let bounded_tags = client
        .browse_tags("Mock.Server.Limits", bounded_collector.clone())
        .await
        .expect("browse with limit should succeed");

    assert_eq!(bounded_tags.len(), 3);
    assert_eq!(bounded_tags, vec!["Tag.1", "Tag.2", "Tag.3"]);
    assert_eq!(bounded_collector.len(), 3);
    assert!(bounded_collector.is_full());
    assert!(!bounded_collector.is_cancelled());

    // 2. Cooperative cancellation
    let cancel_collector = TagCollector::new(50);
    cancel_collector.cancel();
    assert!(cancel_collector.is_cancelled());

    let cancelled_tags = client
        .browse_tags("Mock.Server.Limits", cancel_collector.clone())
        .await
        .expect("cancelled browse must return without error");

    assert!(cancelled_tags.is_empty());
    assert_eq!(cancel_collector.len(), 0);
}

#[tokio::test]
async fn test_tag_browsing_bound_session_facade() {
    let state = Arc::new(MockState::default());
    let expected_tags = vec!["Sensor.FlowRate".to_string(), "Sensor.Pressure".to_string()];
    let connector = MockServerConnector::with_state(state.clone()).with_tags(expected_tags.clone());
    connector
        .server
        .organization
        .store(NamespaceType::Flat as u32, Ordering::Relaxed);

    // Build bound client session directly
    let bound_client = OpcDaClient::builder()
        .server("Mock.Bound.Server")
        .with_connector(connector)
        .build_bound()
        .expect("building bound client must succeed");

    // Infallible endpoint access in Bound typestate
    assert_eq!(bound_client.server_id(), "Mock.Bound.Server");
    assert_eq!(
        bound_client.endpoint().identifier(),
        &ServerIdentifier::from("Mock.Bound.Server")
    );

    // Inherent single-server browse method
    let collector = TagCollector::new(50);
    let tags = bound_client
        .browse(collector.clone())
        .await
        .expect("inherent browse on bound client should succeed");

    assert_eq!(tags, expected_tags);
    assert_eq!(collector.snapshot(), expected_tags);
    assert_eq!(collector.len(), 2);

    // Polymorphic TagBrowser invocation on bound client
    let trait_collector = TagCollector::new(50);
    let trait_tags =
        TagBrowser::browse_tags(&bound_client, "Mock.Bound.Server", trait_collector.clone())
            .await
            .expect("TagBrowser::browse_tags on bound client should succeed");
    assert_eq!(trait_tags, expected_tags);

    // Unbind transitions to Unbound gateway
    let (unbound_client, endpoint) = bound_client.unbind();
    assert_eq!(
        endpoint.identifier(),
        &ServerIdentifier::from("Mock.Bound.Server")
    );
    assert!(unbound_client.endpoint().is_none());
}

#[tokio::test]
async fn test_tag_browsing_guard_unwind_symmetry_and_error_recovery() {
    let state = Arc::new(MockState::default());

    // 1. Phase 1: Descent failure recovery using fluent builder
    let connector = MockServerConnector::with_state(state.clone())
        .with_tags(vec!["Root.Health".to_string()])
        .with_branch_tags(vec!["FailingBranch".to_string()]);
    connector
        .server
        .organization
        .store(NamespaceType::Hierarchy as u32, Ordering::Relaxed);
    connector
        .server
        .supports_flat_browse
        .store(false, Ordering::Relaxed); // Force recursive branch traversal

    let client = OpcDaClient::builder()
        .with_connector(connector.clone())
        .build()
        .expect("building unbound client must succeed");

    // Simulate browse position failure on branch descent
    state
        .should_fail_browse_position
        .store(true, Ordering::Relaxed);

    let collector = TagCollector::new(50);
    let result = client
        .browse_tags("Mock.ErrorRecovery.Server", collector.clone())
        .await
        .expect(
            "browse should recover from branch descent failure and collect sibling/root leaves",
        );

    // Root leaf was collected before attempting branch descent
    assert_eq!(result, vec!["Root.Health"]);
    assert_eq!(collector.snapshot(), vec!["Root.Health"]);

    // Attempted branch descent failed safely without crashing worker
    assert_eq!(
        state.change_browse_position_count.load(Ordering::Relaxed),
        1
    );
    assert_eq!(state.last_browse_direction(), Some(BrowseDirection::Down));

    // 2. Phase 2: Armed BrowsePositionGuard unwind symmetry on child leaf error
    state
        .should_fail_browse_position
        .store(false, Ordering::Relaxed);
    state
        .change_browse_position_count
        .store(0, Ordering::Relaxed);

    // Configure a branch with a leaf whose item_id resolution returns an error.
    // Clear branch_tags on child visit to terminate recursion cleanly without runaway.
    let unwind_state = Arc::new(MockState::default());
    let unwind_branch_tags = Arc::new(Mutex::new(vec!["ValidBranch".to_string()]));
    let unwind_branch_tags_clone = unwind_branch_tags.clone();
    let visit_count = Arc::new(AtomicUsize::new(0));
    let visit_count_clone = visit_count.clone();

    let unwind_server = Arc::new(MockConnectedServer {
        state: unwind_state.clone(),
        tags: Arc::new(Mutex::new(vec!["BadLeaf".to_string()])),
        branch_tags: unwind_branch_tags,
        organization: AtomicU32::new(NamespaceType::Hierarchy as u32),
        supports_flat_browse: AtomicBool::new(false),
        get_item_id_fn: Some(Arc::new(move |_leaf| {
            let count = visit_count_clone.fetch_add(1, Ordering::Relaxed);
            if count > 0 {
                // Depth 1: clear branches so child terminates cleanly without further descent
                unwind_branch_tags_clone.lock().unwrap().clear();
            }
            Err(OpcError::Internal("synthetic item id failure".into()))
        })),
        ..Default::default()
    });

    let unwind_connector = MockServerConnector {
        server: unwind_server,
        state: unwind_state.clone(),
        ..Default::default()
    };

    let unwind_client = OpcDaClient::builder()
        .with_connector(unwind_connector)
        .build()
        .expect("unwind client must initialize");

    let unwind_collector = TagCollector::new(50);
    let unwind_result = unwind_client
        .browse_tags("Mock.Unwind.Server", unwind_collector)
        .await
        .expect("browse should complete even when individual child leaves fail");

    // BadLeaf was rejected safely, so 0 tags collected
    assert!(unwind_result.is_empty());

    // Armed guard entered ValidBranch (1 Down) and dropped on completion/error (1 Up)
    assert_eq!(
        unwind_state
            .change_browse_position_count
            .load(Ordering::Relaxed),
        2,
        "Armed guard must execute symmetric Down and Up navigations"
    );
    assert_eq!(
        unwind_state.last_browse_direction(),
        Some(BrowseDirection::Up),
        "Browse position must return to parent root upon guard drop"
    );
}
