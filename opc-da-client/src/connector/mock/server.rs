use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use crate::connector::mock::group::MockConnectedGroup;
use crate::connector::mock::state::MockState;
use crate::connector::traits::{ConnectedServer, CreatedGroup, GroupConfig, GroupRemovalMode};
use crate::errors::hresult::RPC_S_SERVER_UNAVAILABLE;
use crate::errors::{OpcError, OpcResult};
use crate::types::handles::ServerGroupHandle;
use crate::types::vartype::VarType;
use crate::types::{BrowseDirection, BrowseType, NamespaceType};

/// Callback signature for mocking item ID lookups.
pub type MockGetItemIdFn = dyn Fn(&str) -> OpcResult<String> + Send + Sync;

/// Pure-Rust mock implementation of [`ConnectedServer`] for testing.
pub struct MockConnectedServer {
    /// Mock group associated with this server instance.
    pub group: Arc<MockConnectedGroup>,
    /// Shared failure injection state.
    pub state: Arc<MockState>,
    /// Flag indicating if connection drop should be simulated.
    pub should_fail_connection: AtomicBool,
    /// Simulated leaf tags returned during browse operations.
    pub tags: Arc<Mutex<Vec<String>>>,
    /// Simulated branch tags returned during branch browsing.
    pub branch_tags: Arc<Mutex<Vec<String>>>,
    /// Namespace organization type (1 = Hierarchical, 2 = Flat).
    pub organization: AtomicU32,
    /// Custom callback overriding item ID lookup behavior.
    pub get_item_id_fn: Option<Arc<MockGetItemIdFn>>,
    /// Controls whether `BrowseType::Flat` is supported.
    pub supports_flat_browse: AtomicBool,
}

impl std::fmt::Debug for MockConnectedServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockConnectedServer")
            .finish_non_exhaustive()
    }
}

impl Default for MockConnectedServer {
    fn default() -> Self {
        let state = Arc::new(MockState::default());
        Self {
            group: Arc::new(MockConnectedGroup {
                state: state.clone(),
                ..Default::default()
            }),
            state,
            should_fail_connection: AtomicBool::new(false),
            tags: Arc::new(Mutex::new(vec![
                "Random.Int4".to_string(),
                "Random.Real8".to_string(),
                "Random.String".to_string(),
            ])),
            branch_tags: Arc::new(Mutex::new(vec![
                "Random".to_string(),
                "Simulation".to_string(),
            ])),
            organization: AtomicU32::new(1),
            get_item_id_fn: None,
            supports_flat_browse: AtomicBool::new(true),
        }
    }
}

#[allow(dead_code)]
impl MockConnectedServer {
    /// Creates a new `MockConnectedServer` with default simulation settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Overrides simulated tag IDs returned during browse operations.
    #[must_use]
    pub fn with_tags(self, tags: Vec<String>) -> Self {
        *self.tags.lock().unwrap_or_else(PoisonError::into_inner) = tags;
        self
    }

    /// Overrides simulated branch tag names returned during branch browsing.
    #[must_use]
    pub fn with_branch_tags(self, branch_tags: Vec<String>) -> Self {
        *self
            .branch_tags
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = branch_tags;
        self
    }

    /// Overrides handler for item ID lookups.
    #[must_use]
    pub fn with_get_item_id_fn<F>(mut self, f: F) -> Self
    where
        F: Fn(&str) -> OpcResult<String> + Send + Sync + 'static,
    {
        self.get_item_id_fn = Some(Arc::new(f));
        self
    }

    /// Overrides simulated namespace organization type.
    #[must_use]
    pub fn with_organization(self, org: NamespaceType) -> Self {
        self.organization.store(org as u32, Ordering::Relaxed);
        self
    }
}

impl ConnectedServer for MockConnectedServer {
    type Group = Arc<MockConnectedGroup>;
    type ItemIterator = std::vec::IntoIter<OpcResult<String>>;

    fn ping(&self) -> OpcResult<()> {
        if self.state.should_fail_ping.load(Ordering::Relaxed)
            || self.state.should_fail_connection.load(Ordering::Relaxed)
        {
            return Err(OpcError::Com {
                source: windows_core::Error::from_hresult(RPC_S_SERVER_UNAVAILABLE),
            });
        }
        Ok(())
    }

    fn query_organization(&self) -> OpcResult<NamespaceType> {
        let val = self.organization.load(Ordering::Relaxed);
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
        _data_type: VarType,
        _access_rights: u32,
    ) -> OpcResult<Self::ItemIterator> {
        if browse_type == BrowseType::Flat && !self.supports_flat_browse.load(Ordering::Relaxed) {
            return Err(OpcError::NotImplemented("OPC_FLAT not supported".into()));
        }

        let items: Vec<OpcResult<String>> = match browse_type {
            BrowseType::Branch => {
                let branches = self
                    .branch_tags
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                branches.iter().cloned().map(Ok).collect()
            }
            BrowseType::Leaf | BrowseType::Flat => {
                let tags = self.tags.lock().unwrap_or_else(PoisonError::into_inner);
                tags.iter().cloned().map(Ok).collect()
            }
        };
        Ok(items.into_iter())
    }

    fn change_browse_position(&self, direction: BrowseDirection, _name: &str) -> OpcResult<()> {
        self.state
            .change_browse_position_count
            .fetch_add(1, Ordering::Relaxed);
        self.state.record_browse_direction(direction);
        if self
            .state
            .should_fail_browse_position
            .load(Ordering::Relaxed)
        {
            return Err(OpcError::Internal(
                "Simulated browse position failure".into(),
            ));
        }
        Ok(())
    }

    fn get_item_id(&self, item_name: &str) -> OpcResult<String> {
        if let Some(ref f) = self.get_item_id_fn {
            return f(item_name);
        }
        Ok(item_name.to_string())
    }

    fn add_group(&self, config: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
        if self.state.should_panic_on_request.load(Ordering::Relaxed) {
            std::panic::panic_any("Simulated worker panic");
        }

        if self.should_fail_connection.load(Ordering::Relaxed)
            || self.state.should_fail_connection.load(Ordering::Relaxed)
            || self
                .state
                .should_fail_with_connection_error
                .load(Ordering::Relaxed)
        {
            return Err(OpcError::Com {
                source: windows_core::Error::from_hresult(RPC_S_SERVER_UNAVAILABLE),
            });
        }

        self.state.add_group_count.fetch_add(1, Ordering::Relaxed);
        self.state.record_group_name(config.name);

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
            .fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

impl ConnectedServer for Arc<MockConnectedServer> {
    type Group = Arc<MockConnectedGroup>;
    type ItemIterator = std::vec::IntoIter<OpcResult<String>>;

    fn ping(&self) -> OpcResult<()> {
        (**self).ping()
    }

    fn query_organization(&self) -> OpcResult<NamespaceType> {
        (**self).query_organization()
    }

    fn browse_opc_item_ids(
        &self,
        browse_type: BrowseType,
        filter: Option<&str>,
        data_type: VarType,
        access_rights: u32,
    ) -> OpcResult<Self::ItemIterator> {
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
