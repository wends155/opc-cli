//! Pure-Rust mock infrastructure for testing and test-support.
//!
//! Provides in-memory test doubles for [`ServerConnector`], [`ConnectedServer`],
//! and [`ConnectedGroup`] without requiring Win32 COM interfaces or native drivers.

use crate::com::connector::traits::{
    ConnectedGroup, ConnectedServer, CreatedGroup, DataSource, GroupConfig, GroupItemDef,
    GroupItemResult, GroupItemState, ServerConnector,
};
use crate::com::iterator::StringIterator;
use crate::errors::{OpcError, OpcResult};
use crate::provider::{OpcQuality, OpcValue};
use crate::raw::hresult::RPC_S_SERVER_UNAVAILABLE;
use crate::types::{
    BrowseDirection, BrowseType, GroupHandle, ItemHandle, OpcServerInfo, ServerIdentifier,
};

/// Type alias for mock `add_items` closure.
pub type MockAddItemsFn =
    Box<dyn Fn(&[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> + Send + Sync>;

/// Type alias for mock `read` closure.
pub type MockReadFn = Box<
    dyn Fn(DataSource, &[ItemHandle]) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>
        + Send
        + Sync,
>;

/// Type alias for mock `write` closure.
pub type MockWriteFn =
    Box<dyn Fn(&[ItemHandle], &[OpcValue]) -> OpcResult<Vec<Result<(), OpcError>>> + Send + Sync>;

/// Shared atomic state for mock failure injection and counters.
#[derive(Default, Debug)]
pub struct MockState {
    /// Number of successful connection invocations.
    pub connect_count: std::sync::atomic::AtomicUsize,
    /// Injects failure on server connection and server enumeration.
    pub should_fail_connect: std::sync::atomic::AtomicBool,
    /// Injects write errors on item write operations.
    pub should_fail_write: std::sync::atomic::AtomicBool,
    /// Simulates general connection drop errors.
    pub should_fail_connection: std::sync::atomic::AtomicBool,
    /// Simulates RPC server unavailable error (0x800706BA) triggering connection eviction.
    pub should_fail_with_connection_error: std::sync::atomic::AtomicBool,
    /// Simulates worker thread panic on request handling.
    pub should_panic_on_request: std::sync::atomic::AtomicBool,
    /// Number of times remove_group has been invoked.
    pub remove_group_count: std::sync::atomic::AtomicUsize,
}

/// Pure-Rust mock implementation of [`ConnectedGroup`] for testing.
///
/// Supports customizable closures for `add_items`, `read`, and `write` operations.
#[derive(Default)]
pub struct MockConnectedGroup {
    /// Shared atomic state for mock failure injection and counters.
    pub state: std::sync::Arc<MockState>,
    /// Optional custom handler for adding items to the mock group.
    pub add_items_fn: Option<MockAddItemsFn>,
    /// Optional custom handler for reading items from the mock group.
    pub read_fn: Option<MockReadFn>,
    /// Optional custom handler for writing items to the mock group.
    pub write_fn: Option<MockWriteFn>,
}

impl ConnectedGroup for MockConnectedGroup {
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        if let Some(f) = &self.add_items_fn {
            f(items)
        } else {
            Ok(items
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let handle_val = u32::try_from(i + 1).unwrap_or(u32::MAX);
                    GroupItemResult {
                        server_handle: ItemHandle(handle_val),
                        canonical_type: windows::Win32::System::Variant::VT_BSTR.0,
                        error: None,
                    }
                })
                .collect())
        }
    }

    fn read(
        &self,
        source: DataSource,
        server_handles: &[ItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        if let Some(f) = &self.read_fn {
            f(source, server_handles)
        } else {
            Ok(server_handles
                .iter()
                .map(|&h| {
                    Ok(GroupItemState {
                        client_handle: h,
                        value: OpcValue::Int(42),
                        quality: OpcQuality::GOOD,
                        timestamp: std::time::SystemTime::UNIX_EPOCH,
                    })
                })
                .collect())
        }
    }

    fn write(
        &self,
        server_handles: &[ItemHandle],
        values: &[OpcValue],
    ) -> OpcResult<Vec<Result<(), OpcError>>> {
        if self
            .state
            .should_fail_connection
            .load(std::sync::atomic::Ordering::Relaxed)
            || self
                .state
                .should_fail_with_connection_error
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            // RPC server unavailable (0x800706BA) triggers connection eviction
            return Err(OpcError::Com {
                source: windows::core::Error::from_hresult(RPC_S_SERVER_UNAVAILABLE),
            });
        }

        if self
            .state
            .should_fail_write
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Ok(server_handles
                .iter()
                .map(|_| {
                    Err(OpcError::Com {
                        source: windows::core::Error::from_hresult(
                            windows::Win32::Foundation::E_FAIL,
                        ),
                    })
                })
                .collect());
        }

        if let Some(f) = &self.write_fn {
            f(server_handles, values)
        } else {
            Ok(server_handles.iter().map(|_| Ok(())).collect())
        }
    }
}

impl ConnectedGroup for std::sync::Arc<MockConnectedGroup> {
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        (**self).add_items(items)
    }

    fn read(
        &self,
        source: DataSource,
        server_handles: &[ItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        (**self).read(source, server_handles)
    }

    fn write(
        &self,
        server_handles: &[ItemHandle],
        values: &[OpcValue],
    ) -> OpcResult<Vec<Result<(), OpcError>>> {
        (**self).write(server_handles, values)
    }
}

/// Pure-Rust mock implementation of [`ConnectedServer`] for testing.
///
/// Supports in-memory tag browsing via [`StringIterator::from_vec`] and configurable group handling.
pub struct MockConnectedServer {
    /// Mock group associated with this server instance.
    pub group: std::sync::Arc<MockConnectedGroup>,
    /// Shared failure injection state.
    pub state: std::sync::Arc<MockState>,
    /// Flag indicating if connection drop should be simulated.
    pub should_fail_connection: std::sync::atomic::AtomicBool,
    /// Simulated tag IDs yielded during browse operations.
    pub tags: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// Namespace organization type (1 = Hierarchical, 2 = Flat).
    pub organization: std::sync::atomic::AtomicU32,
}

impl Default for MockConnectedServer {
    fn default() -> Self {
        let state = std::sync::Arc::new(MockState::default());
        Self {
            group: std::sync::Arc::new(MockConnectedGroup {
                state: state.clone(),
                ..Default::default()
            }),
            state,
            should_fail_connection: std::sync::atomic::AtomicBool::new(false),
            tags: std::sync::Arc::new(std::sync::Mutex::new(vec![
                "Random.Int4".to_string(),
                "Random.Real8".to_string(),
                "Random.String".to_string(),
            ])),
            organization: std::sync::atomic::AtomicU32::new(1),
        }
    }
}

impl ConnectedServer for MockConnectedServer {
    type Group = std::sync::Arc<MockConnectedGroup>;

    fn query_organization(&self) -> OpcResult<u32> {
        Ok(self.organization.load(std::sync::atomic::Ordering::Relaxed))
    }

    fn browse_opc_item_ids(
        &self,
        _browse_type: BrowseType,
        _filter: Option<&str>,
        _data_type: u16,
        _access_rights: u32,
    ) -> OpcResult<StringIterator> {
        let tags = self.tags.lock()?;
        Ok(StringIterator::from_vec(tags.clone()))
    }

    fn change_browse_position(&self, _direction: BrowseDirection, _name: &str) -> OpcResult<()> {
        Ok(())
    }

    fn get_item_id(&self, item_name: &str) -> OpcResult<String> {
        Ok(item_name.to_string())
    }

    fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
        if self
            .state
            .should_panic_on_request
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            std::panic::panic_any("Simulated worker panic");
        }

        if self
            .should_fail_connection
            .load(std::sync::atomic::Ordering::Relaxed)
            || self
                .state
                .should_fail_connection
                .load(std::sync::atomic::Ordering::Relaxed)
            || self
                .state
                .should_fail_with_connection_error
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            // RPC server unavailable (0x800706BA) triggers connection eviction
            return Err(OpcError::Com {
                source: windows::core::Error::from_hresult(RPC_S_SERVER_UNAVAILABLE),
            });
        }

        Ok(CreatedGroup {
            group: self.group.clone(),
            server_handle: GroupHandle(1),
            revised_update_rate_ms: config.update_rate_ms,
        })
    }

    fn remove_group(&self, _server_group: GroupHandle, _force: bool) -> OpcResult<()> {
        self.state
            .remove_group_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

/// Pure-Rust mock implementation of [`ServerConnector`] for testing and test-support.
///
/// Exports configurable server enumeration and mock server connections without Windows COM interfaces.
pub struct MockServerConnector {
    /// Mock server yielded on connection.
    pub server: std::sync::Arc<MockConnectedServer>,
    /// Shared failure injection state.
    pub state: std::sync::Arc<MockState>,
    /// Simulated server ProgIDs returned by server enumeration.
    pub servers: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// Simulated server details returned by structured server enumeration.
    pub server_details: std::sync::Arc<std::sync::Mutex<Vec<OpcServerInfo>>>,
}

impl Default for MockServerConnector {
    fn default() -> Self {
        let state = std::sync::Arc::new(MockState::default());
        let server = std::sync::Arc::new(MockConnectedServer {
            group: std::sync::Arc::new(MockConnectedGroup {
                state: state.clone(),
                ..Default::default()
            }),
            state: state.clone(),
            should_fail_connection: std::sync::atomic::AtomicBool::new(false),
            tags: std::sync::Arc::new(std::sync::Mutex::new(vec![
                "Random.Int4".to_string(),
                "Random.Real8".to_string(),
                "Random.String".to_string(),
            ])),
            organization: std::sync::atomic::AtomicU32::new(1),
        });
        let default_details = vec![OpcServerInfo {
            prog_id: "Matrikon.OPC.Simulation.1".to_string(),
            clsid: windows::core::GUID::zeroed(),
            user_type: Some("Matrikon OPC Simulation Server".to_string()),
            host: None,
        }];
        Self {
            server,
            state,
            servers: std::sync::Arc::new(std::sync::Mutex::new(vec![
                "Matrikon.OPC.Simulation.1".to_string(),
            ])),
            server_details: std::sync::Arc::new(std::sync::Mutex::new(default_details)),
        }
    }
}

impl MockServerConnector {
    /// Creates a new `MockServerConnector` with default simulation settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new `MockServerConnector` with the provided shared mock state.
    ///
    /// # Arguments
    /// * `state` - Shared atomic flags controlling mock failure injection.
    #[must_use]
    pub fn with_state(state: std::sync::Arc<MockState>) -> Self {
        let server = std::sync::Arc::new(MockConnectedServer {
            group: std::sync::Arc::new(MockConnectedGroup {
                state: state.clone(),
                ..Default::default()
            }),
            state: state.clone(),
            should_fail_connection: std::sync::atomic::AtomicBool::new(false),
            tags: std::sync::Arc::new(std::sync::Mutex::new(vec![
                "Random.Int4".to_string(),
                "Random.Real8".to_string(),
                "Random.String".to_string(),
            ])),
            organization: std::sync::atomic::AtomicU32::new(1),
        });
        let default_details = vec![OpcServerInfo {
            prog_id: "Mock.Server.1".to_string(),
            clsid: windows::core::GUID::zeroed(),
            user_type: Some("Mock Server 1".to_string()),
            host: None,
        }];
        Self {
            server,
            state,
            servers: std::sync::Arc::new(std::sync::Mutex::new(vec!["Mock.Server.1".to_string()])),
            server_details: std::sync::Arc::new(std::sync::Mutex::new(default_details)),
        }
    }

    /// Overrides simulated tag IDs returned during browse operations.
    ///
    /// # Arguments
    /// * `tags` - Vector of tag identifier strings.
    #[must_use]
    pub fn with_tags(self, tags: Vec<String>) -> Self {
        if let Ok(mut guard) = self.server.tags.lock() {
            *guard = tags;
        }
        self
    }

    /// Overrides simulated server ProgIDs returned during enumeration.
    ///
    /// Also synchronizes synthesized [`OpcServerInfo`] records into `server_details`.
    ///
    /// # Arguments
    /// * `servers` - Vector of server ProgID strings.
    #[must_use]
    pub fn with_servers(self, servers: Vec<String>) -> Self {
        let synthesized: Vec<OpcServerInfo> = servers
            .iter()
            .map(|s| OpcServerInfo {
                prog_id: s.clone(),
                clsid: windows::core::GUID::zeroed(),
                user_type: None,
                host: None,
            })
            .collect();
        if let Ok(mut guard) = self.servers.lock() {
            *guard = servers;
        }
        if let Ok(mut guard) = self.server_details.lock() {
            *guard = synthesized;
        }
        self
    }

    /// Overrides simulated structured server details returned during enumeration.
    ///
    /// Also synchronizes the ProgIDs into `servers`.
    ///
    /// # Arguments
    /// * `details` - Vector of [`OpcServerInfo`] records.
    #[must_use]
    pub fn with_server_details(self, details: Vec<OpcServerInfo>) -> Self {
        let prog_ids: Vec<String> = details.iter().map(|d| d.prog_id.clone()).collect();
        if let Ok(mut guard) = self.server_details.lock() {
            *guard = details;
        }
        if let Ok(mut guard) = self.servers.lock() {
            *guard = prog_ids;
        }
        self
    }
}

impl ServerConnector for MockServerConnector {
    type Server = std::sync::Arc<MockConnectedServer>;

    fn enumerate_servers(&self) -> OpcResult<Vec<String>> {
        if self
            .state
            .should_fail_connect
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(OpcError::Internal("Server enumeration failed".into()));
        }

        let servers = self.servers.lock()?;
        Ok(servers.clone())
    }

    fn enumerate_server_details(&self, _host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        if self
            .state
            .should_fail_connect
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(OpcError::Internal("Server enumeration failed".into()));
        }

        let details = self.server_details.lock()?;
        Ok(details.clone())
    }

    fn connect_identifier(&self, identifier: &ServerIdentifier) -> OpcResult<Self::Server> {
        self.connect(&identifier.to_string())
    }

    fn connect(&self, _server_name: &str) -> OpcResult<Self::Server> {
        if self
            .state
            .should_fail_connect
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(OpcError::Connection("Mock connection failed".into()));
        }

        self.state
            .connect_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(self.server.clone())
    }
}

impl ConnectedServer for std::sync::Arc<MockConnectedServer> {
    type Group = std::sync::Arc<MockConnectedGroup>;

    fn query_organization(&self) -> OpcResult<u32> {
        (**self).query_organization()
    }

    fn browse_opc_item_ids(
        &self,
        browse_type: BrowseType,
        filter: Option<&str>,
        data_type: u16,
        access_rights: u32,
    ) -> OpcResult<StringIterator> {
        (**self).browse_opc_item_ids(browse_type, filter, data_type, access_rights)
    }

    fn change_browse_position(&self, direction: BrowseDirection, name: &str) -> OpcResult<()> {
        (**self).change_browse_position(direction, name)
    }

    fn get_item_id(&self, item_name: &str) -> OpcResult<String> {
        (**self).get_item_id(item_name)
    }

    fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
        (**self).add_group(config)
    }

    fn remove_group(&self, server_group: GroupHandle, force: bool) -> OpcResult<()> {
        (**self).remove_group(server_group, force)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_group_defaults() {
        let group = MockConnectedGroup::default();
        let defs = vec![
            GroupItemDef {
                item_id: "Random.Int4".to_string(),
                client_handle: ItemHandle(0),
                active: true,
            },
            GroupItemDef {
                item_id: "Random.Real8".to_string(),
                client_handle: ItemHandle(1),
                active: true,
            },
        ];

        let results = group.add_items(&defs).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].server_handle, ItemHandle(1));
        assert!(results[0].error.is_none());
        assert_eq!(results[1].server_handle, ItemHandle(2));
        assert!(results[1].error.is_none());

        let states = group
            .read(DataSource::Device, &[ItemHandle(1), ItemHandle(2)])
            .unwrap();
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].as_ref().unwrap().client_handle, ItemHandle(1));
        assert_eq!(states[0].as_ref().unwrap().value, OpcValue::Int(42));
        assert_eq!(states[0].as_ref().unwrap().quality, OpcQuality::GOOD);

        let write_res = group
            .write(&[ItemHandle(1)], &[OpcValue::Int(100)])
            .unwrap();
        assert_eq!(write_res.len(), 1);
        assert!(write_res[0].is_ok());
    }

    #[test]
    fn test_mock_group_custom_handlers() {
        let group = MockConnectedGroup {
            read_fn: Some(Box::new(|source, handles| {
                assert_eq!(source, DataSource::Cache);
                Ok(handles
                    .iter()
                    .map(|&h| {
                        Ok(GroupItemState {
                            client_handle: h,
                            value: OpcValue::Float(42.5),
                            quality: OpcQuality::UNCERTAIN,
                            timestamp: std::time::SystemTime::UNIX_EPOCH,
                        })
                    })
                    .collect())
            })),
            ..Default::default()
        };

        let states = group.read(DataSource::Cache, &[ItemHandle(99)]).unwrap();
        assert_eq!(states.len(), 1);
        let s = states[0].as_ref().unwrap();
        assert_eq!(s.client_handle, ItemHandle(99));
        assert_eq!(s.value, OpcValue::Float(42.5));
        assert_eq!(s.quality, OpcQuality::UNCERTAIN);
    }

    #[test]
    fn test_mock_server_connector_type_aliases_and_dispatch() {
        let add_fn: MockAddItemsFn = Box::new(|defs| {
            Ok(defs
                .iter()
                .map(|d| GroupItemResult {
                    server_handle: d.client_handle,
                    canonical_type: windows::Win32::System::Variant::VT_BSTR.0,
                    error: None,
                })
                .collect())
        });
        let group = MockConnectedGroup {
            add_items_fn: Some(add_fn),
            ..Default::default()
        };
        let res = group
            .add_items(&[GroupItemDef {
                item_id: "test".into(),
                client_handle: ItemHandle(7),
                active: true,
            }])
            .unwrap();
        assert_eq!(res[0].server_handle, ItemHandle(7));
    }

    #[test]
    fn test_mock_server_add_group_and_eviction() {
        let server = MockConnectedServer::default();
        let config = GroupConfig {
            name: "test_group",
            active: true,
            update_rate_ms: 500,
            client_handle: GroupHandle(10),
            time_bias: 0,
            percent_deadband: 0.0,
            locale_id: 0,
        };

        let created = server.add_group(&config).unwrap();
        assert_eq!(created.server_handle, GroupHandle(1));
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
            client_handle: ItemHandle(42),
            active: true,
        };
        let cloned_def = def.clone();
        assert_eq!(def, cloned_def);

        let state = GroupItemState {
            client_handle: ItemHandle(42),
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
            .browse_opc_item_ids(BrowseType::Leaf, None, 0, 0)
            .expect("MockConnectedServer should support browse");
        let tags: Vec<String> = iter.collect::<Result<Vec<_>, _>>().unwrap();
        assert!(!tags.is_empty(), "Mock browse should return simulated tags");
    }

    #[test]
    fn test_mock_server_connector_server_details() {
        use crate::types::OpcServerInfo;
        let mock = MockServerConnector::new().with_server_details(vec![OpcServerInfo {
            prog_id: "Custom.Mock.1".into(),
            clsid: windows::core::GUID::zeroed(),
            user_type: Some("Custom Mock Title".into()),
            host: None,
        }]);
        let details = mock.enumerate_server_details("localhost").unwrap();
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].display_name(), "Custom Mock Title");
    }
}
