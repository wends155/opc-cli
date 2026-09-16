use std::sync::Arc;
use std::sync::atomic::Ordering;

use super::*;
use crate::connector::traits::{
    ConnectedGroup, ConnectedServer, DataSource, GroupConfig, GroupItemDef, GroupItemResult,
    GroupItemState, GroupRemovalMode, ItemWrite, ServerCatalogDiscovery, ServerConnector,
};
use crate::errors::{OpcError, OpcResult};
use crate::types::clsid::Clsid;
use crate::types::handles::{
    ClientGroupHandle, ClientItemHandle, ServerGroupHandle, ServerItemHandle,
};
use crate::types::quality::OpcQuality;
use crate::types::server::{OpcServerEndpoint, ServerIdentifier};
use crate::types::value::OpcValue;
use crate::types::vartype::VarType;
use crate::types::{BrowseType, NamespaceType};

#[test]
fn test_mock_group_defaults() {
    let group = MockConnectedGroup::default();
    let defs = vec![
        GroupItemDef {
            item_id: "Random.Int4".to_string(),
            client_handle: ClientItemHandle::new(0),
            active: true,
        },
        GroupItemDef {
            item_id: "Random.Real8".to_string(),
            client_handle: ClientItemHandle::new(1),
            active: true,
        },
    ];

    let results = group.add_items(&defs).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].server_handle, ServerItemHandle::new(1));
    assert!(results[0].error.is_none());
    assert_eq!(results[1].server_handle, ServerItemHandle::new(2));
    assert!(results[1].error.is_none());

    let states = group
        .read(
            DataSource::Device,
            &[ServerItemHandle::new(1), ServerItemHandle::new(2)],
        )
        .unwrap();
    assert_eq!(states.len(), 2);
    assert_eq!(
        states[0].as_ref().unwrap().client_handle,
        ClientItemHandle::new(1)
    );
    assert_eq!(states[0].as_ref().unwrap().value, OpcValue::Int(42));
    assert_eq!(states[0].as_ref().unwrap().quality, OpcQuality::GOOD);

    let write_res = group
        .write(&[ItemWrite::new(ServerItemHandle::new(1), OpcValue::Int(100))])
        .unwrap();
    assert_eq!(write_res.len(), 1);
    assert!(write_res[0].is_ok());
}

#[test]
fn test_mock_group_custom_handlers() {
    let group = MockConnectedGroup::default().with_read_fn(|source, handles| {
        assert_eq!(source, DataSource::Cache);
        Ok(handles
            .iter()
            .map(|&h| {
                Ok(GroupItemState {
                    client_handle: ClientItemHandle::new(h.as_raw()),
                    value: OpcValue::Float(42.5),
                    quality: OpcQuality::UNCERTAIN,
                    timestamp: std::time::SystemTime::UNIX_EPOCH,
                })
            })
            .collect())
    });

    let states = group
        .read(DataSource::Cache, &[ServerItemHandle::new(99)])
        .unwrap();
    assert_eq!(states.len(), 1);
    let s = states[0].as_ref().unwrap();
    assert_eq!(s.client_handle, ClientItemHandle::new(99));
    assert_eq!(s.value, OpcValue::Float(42.5));
    assert_eq!(s.quality, OpcQuality::UNCERTAIN);
}

#[test]
fn test_mock_server_connector_type_aliases_and_dispatch() {
    let add_fn: MockAddItemsFn = Box::new(|defs| {
        Ok(defs
            .iter()
            .map(|d| GroupItemResult {
                server_handle: ServerItemHandle::new(d.client_handle.as_raw()),
                canonical_type: VarType::BSTR,
                error: None,
            })
            .collect())
    });
    let group = MockConnectedGroup::default().with_add_items_fn(add_fn);
    let res = group
        .add_items(&[GroupItemDef {
            item_id: "test".into(),
            client_handle: ClientItemHandle::new(7),
            active: true,
        }])
        .unwrap();
    assert_eq!(res[0].server_handle, ServerItemHandle::new(7));
}

#[test]
fn test_mock_server_add_group_and_eviction() {
    let server = MockConnectedServer::default();
    let config = GroupConfig {
        name: "test_group",
        active: true,
        update_rate_ms: 500,
        client_handle: ClientGroupHandle::new(10),
        time_bias: 0,
        percent_deadband: 0.0,
        locale_id: 0,
    };

    let created = server.add_group(&config).unwrap();
    assert_eq!(created.server_handle, ServerGroupHandle::new(1));
    assert_eq!(created.revised_update_rate_ms, 500);

    server
        .should_fail_connection
        .store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(server.add_group(&config).is_err());
}

#[test]
fn test_group_item_def_and_state_cloning() {
    let def = GroupItemDef {
        item_id: "Tag1".to_string(),
        client_handle: ClientItemHandle::new(42),
        active: true,
    };
    let cloned_def = def.clone();
    assert_eq!(def, cloned_def);

    let state = GroupItemState {
        client_handle: ClientItemHandle::new(42),
        value: OpcValue::Bool(true),
        quality: OpcQuality::GOOD,
        timestamp: std::time::SystemTime::UNIX_EPOCH,
    };
    let cloned_state = state.clone();
    assert_eq!(state, cloned_state);
    assert_eq!(state.value.to_string(), "true");
}

#[test]
fn test_mock_connector_browse() {
    let server = MockConnectedServer::default();
    let iter = server
        .browse_opc_item_ids(BrowseType::Leaf, None, VarType::EMPTY, 0)
        .expect("MockConnectedServer should support browse");
    let tags: Vec<String> = iter.collect::<Result<Vec<_>, _>>().unwrap();
    assert!(!tags.is_empty(), "Mock browse should return simulated tags");
}

#[test]
fn test_mock_server_connector_server_details() {
    use crate::types::OpcServerInfo;
    let mock = MockServerConnector::new().with_server_details(vec![OpcServerInfo::new(
        "Custom.Mock.1",
        Clsid::zeroed(),
        Some("Custom Mock Title".into()),
        None,
    )]);
    let details = mock.enumerate_server_details("localhost").unwrap();
    assert_eq!(details.len(), 1);
    assert_eq!(details[0].display_name(), "Custom Mock Title");
}

#[test]
fn test_mock_group_preconditions() {
    let group = MockConnectedGroup::default();

    assert!(matches!(
        group.add_items(&[]),
        Err(OpcError::InvalidState(_))
    ));

    assert!(matches!(
        group.read(DataSource::Device, &[]),
        Err(OpcError::InvalidState(_))
    ));

    assert!(matches!(group.write(&[]), Err(OpcError::InvalidState(_))));
}

#[test]
fn test_mock_browse_branch_vs_leaf() {
    let server = MockConnectedServer::default();
    let branch_iter = server
        .browse_opc_item_ids(BrowseType::Branch, None, VarType::EMPTY, 0)
        .unwrap();
    let branches: Vec<String> = branch_iter.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(
        branches,
        vec!["Random".to_string(), "Simulation".to_string()]
    );

    let leaf_iter = server
        .browse_opc_item_ids(BrowseType::Leaf, None, VarType::EMPTY, 0)
        .unwrap();
    let leaves: Vec<String> = leaf_iter.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(
        leaves,
        vec![
            "Random.Int4".to_string(),
            "Random.Real8".to_string(),
            "Random.String".to_string()
        ]
    );
}

#[test]
fn test_mock_connector_with_tag_values() {
    let connector =
        MockServerConnector::new().with_tag_values(vec![OpcValue::Int(123), OpcValue::Float(99.9)]);
    let server = connector.connect("Mock.Server").unwrap();
    let group = server
        .add_group(&GroupConfig::ephemeral("g1"))
        .unwrap()
        .group;
    let states = group
        .read(
            DataSource::Device,
            &[ServerItemHandle::new(1), ServerItemHandle::new(2)],
        )
        .unwrap();
    assert_eq!(states.len(), 2);
    assert_eq!(states[0].as_ref().unwrap().value, OpcValue::Int(123));
    assert_eq!(states[1].as_ref().unwrap().value, OpcValue::Float(99.9));
}

#[test]
fn test_mock_state_observability_counters() {
    let state = std::sync::Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());

    // Connect
    let server = connector
        .connect_identifier(&ServerIdentifier::ProgId("Mock.Server.1".into()))
        .unwrap();
    assert_eq!(
        state
            .connect_count
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    assert_eq!(
        state
            .last_connected_endpoint
            .lock()
            .unwrap()
            .as_ref()
            .map(|e| e.identifier.to_string()),
        Some("Mock.Server.1".to_string())
    );

    // Add group
    let created = server
        .add_group(&GroupConfig::ephemeral("test-group-42"))
        .unwrap();
    assert_eq!(
        state
            .add_group_count
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    assert_eq!(
        state.last_group_name.lock().unwrap().as_deref(),
        Some("test-group-42")
    );

    // Add items
    let item_def = GroupItemDef {
        item_id: "Tag1".to_string(),
        client_handle: ClientItemHandle::new(1),
        active: true,
    };
    created.group.add_items(&[item_def]).unwrap();
    assert_eq!(
        state
            .add_items_count
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );

    // Read
    created
        .group
        .read(DataSource::Device, &[ServerItemHandle::new(1)])
        .unwrap();
    assert_eq!(
        state.read_count.load(std::sync::atomic::Ordering::Relaxed),
        1
    );

    // Remove group
    server
        .remove_group(ServerGroupHandle::new(1), GroupRemovalMode::Force)
        .unwrap();
    assert_eq!(
        state
            .remove_group_count
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
}

#[test]
fn test_connect_endpoint_preserves_host() {
    let state = std::sync::Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());
    let endpoint = crate::types::OpcServerEndpoint::remote_prog_id(
        "192.168.1.100",
        "Matrikon.OPC.Simulation.1",
    );
    let _server = connector
        .connect_endpoint(&endpoint)
        .expect("connect_endpoint should succeed");

    let recorded = state.last_connected_endpoint.lock().unwrap().clone();
    assert_eq!(recorded, Some(endpoint));
    assert_eq!(recorded.unwrap().host.as_deref(), Some("192.168.1.100"));
}

#[test]
fn test_mock_server_connector_fluent_add_items_and_read_hooks() {
    let connector = MockServerConnector::new()
        .with_add_items_fn(|items| {
            Ok(items
                .iter()
                .map(|_item| GroupItemResult {
                    server_handle: ServerItemHandle::new(999),
                    canonical_type: VarType::BSTR,
                    error: None,
                })
                .collect())
        })
        .with_read_fn(|_source, handles| {
            Ok(handles
                .iter()
                .map(|_| {
                    Ok(GroupItemState {
                        client_handle: ClientItemHandle::new(1),
                        value: OpcValue::String("mock-hook".into()),
                        quality: OpcQuality::GOOD,
                        timestamp: std::time::SystemTime::UNIX_EPOCH,
                    })
                })
                .collect())
        });

    let server = connector.connect("Mock.Server.1").unwrap();
    let group = server
        .add_group(&GroupConfig::ephemeral("test"))
        .unwrap()
        .group;

    let added = group
        .add_items(&[GroupItemDef {
            item_id: "CustomTag".into(),
            client_handle: ClientItemHandle::new(1),
            active: true,
        }])
        .unwrap();
    assert_eq!(added[0].server_handle, ServerItemHandle::new(999));

    let read_res = group
        .read(DataSource::Device, &[ServerItemHandle::new(999)])
        .unwrap();
    assert_eq!(
        read_res[0].as_ref().unwrap().value,
        OpcValue::String("mock-hook".into())
    );
}

#[test]
fn test_server_catalog_discovery_segregated_contract() {
    use super::super::traits::{ServerBackend, ServerCatalogDiscovery, ServerConnector};
    use crate::types::OpcServerInfo;

    let connector = MockServerConnector::new().with_server_details(vec![OpcServerInfo::new(
        "Matrikon.OPC.Simulation.1",
        crate::types::Clsid::zeroed(),
        Some("Matrikon Sim".into()),
        None,
    )]);

    // 1. Verify dynamic dispatch via segregated ServerCatalogDiscovery
    let discovery: &dyn ServerCatalogDiscovery = &connector;
    let servers = discovery
        .enumerate_servers("localhost")
        .expect("enumerate_servers failed");
    assert_eq!(servers, vec!["Matrikon.OPC.Simulation.1"]);

    // 2. Verify connection capability via ServerConnector
    fn assert_connector<C: ServerConnector + ?Sized>(_c: &C) {}
    assert_connector(&connector);

    // 3. Verify static bound via composite ServerBackend
    fn assert_backend<B: ServerBackend + ?Sized>(_b: &B) {}
    assert_backend(&connector);
}

#[test]
fn test_mock_browse_custom_item_iterator() {
    let server = MockConnectedServer::default().with_tags(vec![
        "Custom.Plant.Line1.Temperature".to_string(),
        "Custom.Plant.Line1.Pressure".to_string(),
    ]);

    // Verify that ItemIterator implements Iterator<Item = OpcResult<String>>
    fn assert_item_iterator<I: Iterator<Item = OpcResult<String>>>(_iter: I) {}

    let iter = server
        .browse_opc_item_ids(BrowseType::Leaf, None, VarType::EMPTY, 0)
        .expect("browse_opc_item_ids should succeed");
    assert_item_iterator(iter);

    // Verify that ConnectedServer::ItemIterator associated type can be consumed generically
    fn browse_all<S: ConnectedServer>(s: &S) -> OpcResult<Vec<String>> {
        let iter = s.browse_opc_item_ids(BrowseType::Leaf, None, VarType::EMPTY, 0)?;
        iter.collect()
    }

    let items = browse_all(&server).expect("generic browse_all should succeed");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0], "Custom.Plant.Line1.Temperature");
    assert_eq!(items[1], "Custom.Plant.Line1.Pressure");
}

#[test]
fn test_offline_tier2_spi_mocking_without_com() {
    // 1. Pure in-memory instantiation without COM runtime or CoInitializeEx
    let connector = MockServerConnector::new()
        .with_servers(vec!["Offline.Server.1".to_string()])
        .with_tag_values(vec![OpcValue::Int(42)]);

    // 2. Catalog enumeration in offline mode
    let servers = connector.enumerate_servers("localhost").unwrap();
    assert_eq!(servers, vec!["Offline.Server.1"]);
    let details = connector.enumerate_server_details("localhost").unwrap();
    assert_eq!(details.len(), 1);
    assert_eq!(details[0].prog_id(), "Offline.Server.1");

    // 3. Connect endpoint offline
    let server = connector
        .connect_endpoint(&crate::types::OpcServerEndpoint::local_prog_id(
            "Offline.Server.1",
        ))
        .unwrap();
    assert!(server.ping().is_ok());
    assert_eq!(
        server.query_organization().unwrap(),
        NamespaceType::Hierarchy
    );

    // 4. In-memory tag browsing
    let leaves: Vec<String> = server
        .browse_opc_item_ids(BrowseType::Leaf, None, VarType::EMPTY, 0)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(!leaves.is_empty());

    // 5. Ephemeral group creation
    let created = server
        .add_group(&GroupConfig::ephemeral("offline-group"))
        .unwrap();
    assert_eq!(created.server_handle, ServerGroupHandle::new(1));

    // 6. Pure-Rust item registration
    let items = [GroupItemDef {
        item_id: "Random.Int4".to_string(),
        client_handle: ClientItemHandle::new(1),
        active: true,
    }];
    let item_results = created.group.add_items(&items).unwrap();
    assert_eq!(item_results.len(), 1);
    assert_eq!(item_results[0].server_handle, ServerItemHandle::new(1));

    // 7. Synchronous read simulation
    let read_states = created
        .group
        .read(DataSource::Device, &[ServerItemHandle::new(1)])
        .unwrap();
    assert_eq!(read_states.len(), 1);
    assert_eq!(read_states[0].as_ref().unwrap().value, OpcValue::Int(42));

    // 8. Synchronous write simulation
    let write_results = created
        .group
        .write(&[ItemWrite::new(ServerItemHandle::new(1), OpcValue::Int(99))])
        .unwrap();
    assert_eq!(write_results.len(), 1);
    assert!(write_results[0].is_ok());

    // 9. Group removal
    server
        .remove_group(ServerGroupHandle::new(1), GroupRemovalMode::Force)
        .unwrap();
}

#[test]
fn test_mock_server_change_browse_position_tracking() {
    use crate::connector::traits::ConnectedServer;
    use crate::types::BrowseDirection;
    use std::sync::atomic::Ordering;

    let server = MockConnectedServer::default();
    assert_eq!(
        server
            .state
            .change_browse_position_count
            .load(Ordering::Relaxed),
        0
    );

    server
        .change_browse_position(BrowseDirection::Down, "Branch1")
        .unwrap();
    assert_eq!(
        server
            .state
            .change_browse_position_count
            .load(Ordering::Relaxed),
        1
    );
    assert_eq!(
        *server.state.last_browse_direction.lock().unwrap(),
        Some(BrowseDirection::Down)
    );

    server
        .state
        .should_fail_browse_position
        .store(true, Ordering::Relaxed);
    assert!(
        server
            .change_browse_position(BrowseDirection::Up, "")
            .is_err()
    );
}

#[test]
fn test_mock_telemetry_symmetry_on_failure_and_success() {
    let state = Arc::new(MockState::default());
    let connector = MockServerConnector::with_state(state.clone());
    let id = ServerIdentifier::ProgId("Mock.Server.1".into());
    let endpoint = OpcServerEndpoint::local_prog_id("Mock.Server.1");

    // 1. Connection failure injection: both methods return Err and do NOT increment connect_count
    state.should_fail_connect.store(true, Ordering::Relaxed);
    let res_id = connector.connect_identifier(&id);
    assert!(res_id.is_err());
    assert!(res_id.unwrap_err().is_connection_error());
    assert_eq!(state.connect_count.load(Ordering::Relaxed), 0);

    let res_ep = connector.connect_endpoint(&endpoint);
    assert!(res_ep.is_err());
    assert!(res_ep.unwrap_err().is_connection_error());
    assert_eq!(state.connect_count.load(Ordering::Relaxed), 0);

    // 2. ProgID failure injection: both return Com error and do NOT increment connect_count
    state.should_fail_connect.store(false, Ordering::Relaxed);
    state.should_fail_progid.store(true, Ordering::Relaxed);
    let res_prog_id = connector.connect_identifier(&id);
    assert!(res_prog_id.is_err());
    assert_eq!(state.connect_count.load(Ordering::Relaxed), 0);

    let res_prog_ep = connector.connect_endpoint(&endpoint);
    assert!(res_prog_ep.is_err());
    assert_eq!(state.connect_count.load(Ordering::Relaxed), 0);

    // 3. Success path: both methods increment connect_count and record last_connected_endpoint identically
    state.should_fail_progid.store(false, Ordering::Relaxed);
    let s1 = connector
        .connect_identifier(&id)
        .expect("connect_identifier should succeed");
    assert!(s1.ping().is_ok());
    assert_eq!(state.connect_count.load(Ordering::Relaxed), 1);
    assert_eq!(state.last_connected_endpoint().unwrap(), endpoint);

    let s2 = connector
        .connect_endpoint(&endpoint)
        .expect("connect_endpoint should succeed");
    assert!(s2.ping().is_ok());
    assert_eq!(state.connect_count.load(Ordering::Relaxed), 2);
    assert_eq!(state.last_connected_endpoint().unwrap(), endpoint);
}

#[test]
fn test_mock_browse_single_pass_allocation() {
    let server = MockConnectedServer::default().with_tags(vec![
        "Item.A".to_string(),
        "Item.B".to_string(),
        "Item.C".to_string(),
    ]);
    let iter = server
        .browse_opc_item_ids(BrowseType::Leaf, None, VarType::EMPTY, 0)
        .unwrap();
    let results: Vec<OpcResult<String>> = iter.collect();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].as_ref().unwrap(), "Item.A");
    assert_eq!(results[1].as_ref().unwrap(), "Item.B");
    assert_eq!(results[2].as_ref().unwrap(), "Item.C");
}

#[test]
fn test_mock_state_lock_poison_recovery() {
    let state = Arc::new(MockState::default());
    let state_clone = state.clone();

    // Poison last_connected_endpoint mutex via simulated panic
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = state_clone.last_connected_endpoint.lock().unwrap();
        panic!("simulated panic holding mock lock");
    }));

    // Assert getter recovers from poisoned mutex
    assert_eq!(state.last_connected_endpoint(), None);

    // Assert mutator recovers and records endpoint
    let ep = OpcServerEndpoint::local_prog_id("Recovered.Server");
    state.record_connection(&ep);
    assert_eq!(state.last_connected_endpoint(), Some(ep));
}
