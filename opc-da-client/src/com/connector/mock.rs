//! Pure-Rust mock infrastructure for testing and test-support.
//!
//! Provides in-memory test doubles for [`ServerConnector`], [`ConnectedServer`],
//! and [`ConnectedGroup`] without requiring Win32 COM interfaces or native drivers.

use crate::com::connector::traits::{
    ConnectedGroup, ConnectedServer, CreatedGroup, DataSource, GroupConfig, GroupItemDef,
    GroupItemResult, GroupItemState, GroupRemovalMode, ItemWrite, ServerConnector,
};
use crate::com::iterator::StringIterator;
use crate::errors::{OpcError, OpcResult};
use crate::raw::hresult::RPC_S_SERVER_UNAVAILABLE;
use crate::types::{
    BrowseDirection, BrowseType, ClientItemHandle, NamespaceType, OpcQuality, OpcServerInfo,
    OpcValue, ServerGroupHandle, ServerIdentifier, ServerItemHandle,
};

/// Type alias for mock `add_items` closure.
pub type MockAddItemsFn =
    Box<dyn Fn(&[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> + Send + Sync>;

/// Type alias for mock `read` closure.
pub type MockReadFn = Box<
    dyn Fn(DataSource, &[ServerItemHandle]) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>
        + Send
        + Sync,
>;

/// Type alias for mock `write` closure.
pub type MockWriteFn =
    Box<dyn Fn(&[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>> + Send + Sync>;

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
    /// Number of times add_group has been invoked.
    pub add_group_count: std::sync::atomic::AtomicUsize,
    /// Number of times add_items has been invoked.
    pub add_items_count: std::sync::atomic::AtomicUsize,
    /// Number of times read has been invoked.
    pub read_count: std::sync::atomic::AtomicUsize,
    /// Last group name passed to add_group.
    pub last_group_name: std::sync::Mutex<Option<String>>,
    /// Last endpoint passed to connect_identifier.
    pub last_connected_endpoint: std::sync::Mutex<Option<crate::types::OpcServerEndpoint>>,
    /// Last host passed to enumerate_servers / enumerate_server_details.
    pub last_enumerated_host: std::sync::Mutex<Option<String>>,
}

/// Pure-Rust mock implementation of [`ConnectedGroup`] for testing.
///
/// Supports customizable closures for `add_items`, `read`, and `write` operations.
#[derive(Default)]
pub struct MockConnectedGroup {
    /// Shared atomic state for mock failure injection and counters.
    pub state: std::sync::Arc<MockState>,
    /// Preconfigured values returned on read when no custom read_fn is set.
    pub tag_values: std::sync::Arc<std::sync::Mutex<Vec<OpcValue>>>,
    /// Optional custom handler for adding items to the mock group.
    pub add_items_fn: std::sync::Arc<std::sync::Mutex<Option<MockAddItemsFn>>>,
    /// Optional custom handler for reading items from the mock group.
    pub read_fn: std::sync::Arc<std::sync::Mutex<Option<MockReadFn>>>,
    /// Optional custom handler for writing items to the mock group.
    pub write_fn: std::sync::Arc<std::sync::Mutex<Option<MockWriteFn>>>,
}

impl MockConnectedGroup {
    /// Configures a custom `add_items` handler.
    #[must_use]
    pub fn with_add_items_fn<F>(self, f: F) -> Self
    where
        F: Fn(&[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.add_items_fn.lock() {
            *guard = Some(Box::new(f));
        }
        self
    }

    /// Configures a custom `read` handler.
    #[must_use]
    pub fn with_read_fn<F>(self, f: F) -> Self
    where
        F: Fn(DataSource, &[ServerItemHandle]) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>
            + Send
            + Sync
            + 'static,
    {
        if let Ok(mut guard) = self.read_fn.lock() {
            *guard = Some(Box::new(f));
        }
        self
    }

    /// Configures a custom `write` handler.
    #[must_use]
    pub fn with_write_fn<F>(self, f: F) -> Self
    where
        F: Fn(&[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>> + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.write_fn.lock() {
            *guard = Some(Box::new(f));
        }
        self
    }
}

impl ConnectedGroup for MockConnectedGroup {
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        if items.is_empty() {
            return Err(OpcError::InvalidState("items cannot be empty".to_string()));
        }

        self.state
            .add_items_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        if let Ok(guard) = self.add_items_fn.lock()
            && let Some(f) = guard.as_ref()
        {
            return f(items);
        }

        Ok(items
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let handle_val = u32::try_from(i + 1).unwrap_or(u32::MAX);
                GroupItemResult {
                    server_handle: ServerItemHandle::new(handle_val),
                    canonical_type: windows::Win32::System::Variant::VT_BSTR.0,
                    error: None,
                }
            })
            .collect())
    }

    fn read(
        &self,
        source: DataSource,
        server_handles: &[ServerItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        if server_handles.is_empty() {
            return Err(OpcError::InvalidState(
                "server_handles cannot be empty".to_string(),
            ));
        }

        self.state
            .read_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        if let Ok(guard) = self.read_fn.lock()
            && let Some(f) = guard.as_ref()
        {
            return f(source, server_handles);
        }

        let configured = self.tag_values.lock()?;
        Ok(server_handles
            .iter()
            .enumerate()
            .map(|(i, &h)| {
                let val = configured.get(i).cloned().unwrap_or(OpcValue::Int(42));
                Ok(GroupItemState {
                    client_handle: ClientItemHandle::new(h.as_raw()),
                    value: val,
                    quality: OpcQuality::GOOD,
                    timestamp: std::time::SystemTime::UNIX_EPOCH,
                })
            })
            .collect())
    }

    fn write(&self, items: &[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>> {
        if items.is_empty() {
            return Err(OpcError::InvalidState("items cannot be empty".to_string()));
        }

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
            return Ok(items
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

        if let Ok(guard) = self.write_fn.lock()
            && let Some(f) = guard.as_ref()
        {
            return f(items);
        }

        Ok(items.iter().map(|_| Ok(())).collect())
    }
}

impl ConnectedGroup for std::sync::Arc<MockConnectedGroup> {
    fn add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        (**self).add_items(items)
    }

    fn read(
        &self,
        source: DataSource,
        server_handles: &[ServerItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        (**self).read(source, server_handles)
    }

    fn write(&self, items: &[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>> {
        (**self).write(items)
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
    /// Simulated tag IDs yielded during browse operations (leaf / flat).
    pub tags: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// Simulated branch tags yielded during branch browse operations.
    pub branch_tags: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
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
            branch_tags: std::sync::Arc::new(std::sync::Mutex::new(vec![
                "Random".to_string(),
                "Simulation".to_string(),
            ])),
            organization: std::sync::atomic::AtomicU32::new(1),
        }
    }
}

impl ConnectedServer for MockConnectedServer {
    type Group = std::sync::Arc<MockConnectedGroup>;

    fn query_organization(&self) -> OpcResult<NamespaceType> {
        let val = self.organization.load(std::sync::atomic::Ordering::Relaxed);
        if val == 2 {
            Ok(NamespaceType::Flat)
        } else {
            Ok(NamespaceType::Hierarchy)
        }
    }

    fn browse_opc_item_ids(
        &self,
        browse_type: BrowseType,
        _filter: Option<&str>,
        _data_type: u16,
        _access_rights: u32,
    ) -> OpcResult<StringIterator> {
        let items = match browse_type {
            BrowseType::Branch => {
                let branches = self.branch_tags.lock()?;
                branches.clone()
            }
            BrowseType::Leaf | BrowseType::Flat => {
                let tags = self.tags.lock()?;
                tags.clone()
            }
        };
        Ok(StringIterator::from_vec(items))
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

        self.state
            .add_group_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut lock) = self.state.last_group_name.lock() {
            *lock = Some(config.name.to_string());
        }

        Ok(CreatedGroup {
            group: self.group.clone(),
            server_handle: ServerGroupHandle::new(1),
            revised_update_rate_ms: config.update_rate_ms,
        })
    }

    fn remove_group(
        &self,
        _server_group: ServerGroupHandle,
        _mode: GroupRemovalMode,
    ) -> OpcResult<()> {
        self.state
            .remove_group_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

/// Pure-Rust mock implementation of [`ServerConnector`] for testing and test-support.
///
/// Exports configurable server enumeration and mock server connections without Windows COM interfaces.
#[derive(Clone)]
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
            ..Default::default()
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
            ..Default::default()
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

    /// Overrides simulated tag values returned during read operations.
    #[must_use]
    pub fn with_tag_values<I>(self, values: I) -> Self
    where
        I: IntoIterator<Item = OpcValue>,
    {
        if let Ok(mut guard) = self.server.group.tag_values.lock() {
            *guard = values.into_iter().collect();
        }
        self
    }

    /// Overrides simulated branch tag names returned during branch browsing.
    #[must_use]
    pub fn with_branch_tags(self, branches: Vec<String>) -> Self {
        if let Ok(mut guard) = self.server.branch_tags.lock() {
            *guard = branches;
        }
        self
    }

    /// Overrides handler for adding items to the mock group.
    #[must_use]
    pub fn with_add_items_fn<F>(self, f: F) -> Self
    where
        F: Fn(&[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.server.group.add_items_fn.lock() {
            *guard = Some(Box::new(f));
        }
        self
    }

    /// Overrides handler for reading items from the mock group.
    #[must_use]
    pub fn with_read_fn<F>(self, f: F) -> Self
    where
        F: Fn(DataSource, &[ServerItemHandle]) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>
            + Send
            + Sync
            + 'static,
    {
        if let Ok(mut guard) = self.server.group.read_fn.lock() {
            *guard = Some(Box::new(f));
        }
        self
    }

    /// Overrides handler for writing items to the mock group.
    #[must_use]
    pub fn with_write_fn<F>(self, f: F) -> Self
    where
        F: Fn(&[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>> + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.server.group.write_fn.lock() {
            *guard = Some(Box::new(f));
        }
        self
    }
}

impl ServerConnector for MockServerConnector {
    type Server = std::sync::Arc<MockConnectedServer>;

    fn enumerate_servers(&self, host: &str) -> OpcResult<Vec<String>> {
        if self
            .state
            .should_fail_connect
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(OpcError::Internal("Server enumeration failed".into()));
        }

        if let Ok(mut lock) = self.state.last_enumerated_host.lock() {
            *lock = Some(host.to_string());
        }

        let servers = self.servers.lock()?;
        Ok(servers.clone())
    }

    fn enumerate_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        if self
            .state
            .should_fail_connect
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(OpcError::Internal("Server enumeration failed".into()));
        }

        if let Ok(mut lock) = self.state.last_enumerated_host.lock() {
            *lock = Some(host.to_string());
        }

        let details = self.server_details.lock()?;
        Ok(details.clone())
    }

    fn connect_endpoint(
        &self,
        endpoint: &crate::types::OpcServerEndpoint,
    ) -> OpcResult<Self::Server> {
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
        if let Ok(mut lock) = self.state.last_connected_endpoint.lock() {
            *lock = Some(endpoint.clone());
        }
        Ok(self.server.clone())
    }

    fn connect_identifier(&self, identifier: &ServerIdentifier) -> OpcResult<Self::Server> {
        self.state
            .connect_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        if let Ok(mut lock) = self.state.last_connected_endpoint.lock() {
            *lock = Some(crate::types::OpcServerEndpoint::from(identifier.clone()));
        }

        if self
            .state
            .should_fail_connect
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(OpcError::connection_failed(
                identifier.to_string(),
                windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL),
            ));
        }

        Ok(self.server.clone())
    }
}

impl ConnectedServer for std::sync::Arc<MockConnectedServer> {
    type Group = std::sync::Arc<MockConnectedGroup>;

    fn query_organization(&self) -> OpcResult<NamespaceType> {
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

    fn remove_group(
        &self,
        server_group: ServerGroupHandle,
        mode: GroupRemovalMode,
    ) -> OpcResult<()> {
        (**self).remove_group(server_group, mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ClientGroupHandle;

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
                    canonical_type: windows::Win32::System::Variant::VT_BSTR.0,
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
            .browse_opc_item_ids(BrowseType::Branch, None, 0, 0)
            .unwrap();
        let branches: Vec<String> = branch_iter.collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(
            branches,
            vec!["Random".to_string(), "Simulation".to_string()]
        );

        let leaf_iter = server
            .browse_opc_item_ids(BrowseType::Leaf, None, 0, 0)
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
        let connector = MockServerConnector::new()
            .with_tag_values(vec![OpcValue::Int(123), OpcValue::Float(99.9)]);
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
        let endpoint =
            crate::types::OpcServerEndpoint::remote("192.168.1.100", "Matrikon.OPC.Simulation.1");
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
                        canonical_type: 8,
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
}
