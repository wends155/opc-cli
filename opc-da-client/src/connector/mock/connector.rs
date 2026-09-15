use std::sync::{Arc, Mutex, PoisonError};

use crate::connector::mock::group::MockConnectedGroup;
use crate::connector::mock::server::MockConnectedServer;
use crate::connector::mock::state::MockState;
use crate::connector::traits::{
    DataSource, GroupItemDef, GroupItemResult, GroupItemState, ItemWrite, ServerCatalogDiscovery,
    ServerConnector,
};
use crate::errors::hresult::CO_E_CLASSSTRING;
use crate::errors::{OpcError, OpcResult};
use crate::types::clsid::Clsid;
use crate::types::handles::ServerItemHandle;
use crate::types::server::{OpcServerEndpoint, OpcServerInfo, ServerIdentifier};
use crate::types::value::OpcValue;

/// Pure-Rust mock implementation of [`ServerConnector`] for testing and test-support.
///
/// Exports configurable server enumeration and mock server connections without Windows COM interfaces.
#[derive(Clone)]
pub struct MockServerConnector {
    /// Mock server yielded on connection.
    pub server: Arc<MockConnectedServer>,
    /// Shared failure injection state.
    pub state: Arc<MockState>,
    /// Simulated server ProgIDs returned by server enumeration.
    pub servers: Arc<Mutex<Vec<String>>>,
    /// Simulated server details returned by structured server enumeration.
    pub server_details: Arc<Mutex<Vec<OpcServerInfo>>>,
}

impl Default for MockServerConnector {
    fn default() -> Self {
        let state = Arc::new(MockState::default());
        let server = Arc::new(MockConnectedServer {
            group: Arc::new(MockConnectedGroup {
                state: state.clone(),
                ..Default::default()
            }),
            state: state.clone(),
            ..Default::default()
        });
        let default_details = vec![OpcServerInfo::new(
            "Matrikon.OPC.Simulation.1",
            Clsid::zeroed(),
            Some("Matrikon OPC Simulation Server".to_string()),
            None,
        )];
        Self {
            server,
            state,
            servers: Arc::new(Mutex::new(vec!["Matrikon.OPC.Simulation.1".to_string()])),
            server_details: Arc::new(Mutex::new(default_details)),
        }
    }
}

#[allow(dead_code)]
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
    pub fn with_state(state: Arc<MockState>) -> Self {
        let server = Arc::new(MockConnectedServer {
            group: Arc::new(MockConnectedGroup {
                state: state.clone(),
                ..Default::default()
            }),
            state: state.clone(),
            ..Default::default()
        });
        let default_details = vec![OpcServerInfo::new(
            "Mock.Server.1",
            Clsid::zeroed(),
            Some("Mock Server 1".to_string()),
            None,
        )];
        Self {
            server,
            state,
            servers: Arc::new(Mutex::new(vec!["Mock.Server.1".to_string()])),
            server_details: Arc::new(Mutex::new(default_details)),
        }
    }

    /// Overrides simulated tag IDs returned during browse operations.
    ///
    /// # Arguments
    /// * `tags` - Vector of tag identifier strings.
    #[must_use]
    pub fn with_tags(self, tags: Vec<String>) -> Self {
        *self
            .server
            .tags
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = tags;
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
            .map(|s| OpcServerInfo::new(s.clone(), Clsid::zeroed(), None, None))
            .collect();
        *self.servers.lock().unwrap_or_else(PoisonError::into_inner) = servers;
        *self
            .server_details
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = synthesized;
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
        let prog_ids: Vec<String> = details.iter().map(|d| d.prog_id().to_string()).collect();
        *self
            .server_details
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = details;
        *self.servers.lock().unwrap_or_else(PoisonError::into_inner) = prog_ids;
        self
    }

    /// Overrides simulated tag values returned during read operations.
    #[must_use]
    pub fn with_tag_values<I>(self, values: I) -> Self
    where
        I: IntoIterator<Item = OpcValue>,
    {
        *self
            .server
            .group
            .tag_values
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = values.into_iter().collect();
        self
    }

    /// Overrides simulated branch tag names returned during branch browsing.
    #[must_use]
    pub fn with_branch_tags(self, branches: Vec<String>) -> Self {
        *self
            .server
            .branch_tags
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = branches;
        self
    }

    /// Overrides handler for adding items to the mock group.
    #[must_use]
    pub fn with_add_items_fn<F>(self, f: F) -> Self
    where
        F: Fn(&[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> + Send + Sync + 'static,
    {
        *self
            .server
            .group
            .add_items_fn
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Box::new(f));
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
        *self
            .server
            .group
            .read_fn
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Box::new(f));
        self
    }

    /// Overrides handler for writing items to the mock group.
    #[must_use]
    pub fn with_write_fn<F>(self, f: F) -> Self
    where
        F: Fn(&[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>> + Send + Sync + 'static,
    {
        *self
            .server
            .group
            .write_fn
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Box::new(f));
        self
    }
}

impl ServerCatalogDiscovery for MockServerConnector {
    fn enumerate_servers(&self, host: &str) -> OpcResult<Vec<String>> {
        if self
            .state
            .should_fail_connect
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(OpcError::Internal("Server enumeration failed".into()));
        }

        self.state.record_enumerated_host(host);

        let servers = self.servers.lock().unwrap_or_else(PoisonError::into_inner);
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

        self.state.record_enumerated_host(host);

        let details = self
            .server_details
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Ok(details.clone())
    }
}

impl ServerConnector for MockServerConnector {
    type Server = Arc<MockConnectedServer>;

    #[tracing::instrument(level = "info", skip(self), err)]
    fn connect_endpoint(&self, endpoint: &OpcServerEndpoint) -> OpcResult<Self::Server> {
        if self
            .state
            .should_fail_connect
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(OpcError::Connection("Mock connection failed".into()));
        }

        if self
            .state
            .should_fail_progid
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(OpcError::Com {
                source: windows_core::Error::from_hresult(CO_E_CLASSSTRING),
            });
        }

        self.state
            .connect_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.state.record_connection(endpoint);
        Ok(self.server.clone())
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    fn connect_identifier(&self, identifier: &ServerIdentifier) -> OpcResult<Self::Server> {
        self.connect_endpoint(&OpcServerEndpoint::from(identifier.clone()))
    }
}
