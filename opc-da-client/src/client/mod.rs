//! High-level OPC DA client facade and typestate session management.

pub mod builder;
pub mod gateway;
pub mod session;
pub mod subscription;
pub mod typestate;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

pub use builder::OpcDaClientBuilder;
pub use typestate::{Bound, Unbound};

#[cfg(feature = "opc-da-backend")]
use crate::com::connector::ComConnector;
use crate::com::worker::{ComRequest, ComWorker};
use crate::connector::ServerBackend;
use crate::errors::{OpcError, OpcResult};
use crate::types::{OpcServerEndpoint, ServerIdentifier};

#[cfg(feature = "opc-da-backend")]
pub type DefaultBackendConnector = ComConnector;

#[cfg(all(not(feature = "opc-da-backend"), any(test, feature = "test-support")))]
pub type DefaultBackendConnector = crate::connector::MockServerConnector;

#[cfg(all(
    not(feature = "opc-da-backend"),
    not(any(test, feature = "test-support"))
))]
pub type DefaultBackendConnector = crate::connector::NoopServerBackend;

pub type DefaultOpcDaClient<State = Unbound> = OpcDaClient<DefaultBackendConnector, State>;

/// High-level client facade parameterized by connector backend `C` and typestate `State`.
pub struct OpcDaClient<C: ServerBackend + 'static = DefaultBackendConnector, State = Unbound> {
    pub(crate) worker: Arc<ComWorker<C>>,
    pub(crate) endpoint: Option<OpcServerEndpoint>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) _state: std::marker::PhantomData<State>,
}

impl<C: ServerBackend + 'static, State> Clone for OpcDaClient<C, State> {
    fn clone(&self) -> Self {
        Self {
            worker: Arc::clone(&self.worker),
            endpoint: self.endpoint.clone(),
            timeout: self.timeout,
            _state: std::marker::PhantomData,
        }
    }
}

impl<C: ServerBackend + 'static, State> std::fmt::Debug for OpcDaClient<C, State> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpcDaClient")
            .field("endpoint", &self.endpoint)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl OpcDaClient<DefaultBackendConnector, Unbound> {
    /// Creates a new default client builder targeting the default backend connector.
    #[must_use]
    pub fn builder() -> OpcDaClientBuilder<DefaultBackendConnector> {
        OpcDaClientBuilder::new()
    }
}

impl OpcDaClient<DefaultBackendConnector, Unbound> {
    /// Constructs a client bound to an OPC DA server endpoint (local ProgID/CLSID, UNC path, or URI).
    ///
    /// Validates the target endpoint *before* initializing the backend connector.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError`] if the endpoint format is invalid or backend initialization fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use opc_da_client::OpcDaClient;
    ///
    /// # fn run() -> opc_da_client::OpcResult<()> {
    /// let client = OpcDaClient::bind_new("Matrikon.OPC.Simulation.1")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn bind_new(
        server: impl TryInto<OpcServerEndpoint, Error: Into<OpcError>>,
    ) -> OpcResult<OpcDaClient<DefaultBackendConnector, Bound>> {
        let endpoint = server.try_into().map_err(Into::into)?;
        let client = Self::new(DefaultBackendConnector::default())?;
        Ok(client.bind(endpoint))
    }

    /// Constructs a client bound to a remote OPC DA server by host and ProgID or CLSID.
    ///
    /// Validates the host and server identifier *before* initializing the backend connector.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError`] if the host contains interior null bytes, the server identifier is invalid,
    /// or backend initialization fails.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult};
    /// # fn run() -> OpcResult<()> {
    /// let client = OpcDaClient::bind_new_remote("192.168.1.10", "Matrikon.OPC.Simulation.1")?;
    /// assert!(client.endpoint().is_remote());
    /// # Ok(())
    /// # }
    /// ```
    pub fn bind_new_remote(
        host: impl Into<String>,
        server: impl TryInto<ServerIdentifier, Error: Into<OpcError>>,
    ) -> OpcResult<OpcDaClient<DefaultBackendConnector, Bound>> {
        let host_str = host.into();
        if host_str.contains('\0') {
            return Err(OpcError::InvalidState(
                "Host cannot contain interior null bytes".to_string(),
            ));
        }
        let identifier = server.try_into().map_err(Into::into)?;
        let endpoint = OpcServerEndpoint::remote(host_str, identifier);
        let client = Self::new(DefaultBackendConnector::default())?;
        Ok(client.bind(endpoint))
    }
}

impl<C: ServerBackend + 'static> OpcDaClient<C, Unbound> {
    /// Creates a builder targeting a specific backend connector.
    #[must_use]
    pub fn builder_with_connector(connector: C) -> OpcDaClientBuilder<C> {
        OpcDaClientBuilder::new_with_connector(connector)
    }

    /// Constructs an unbound client initialized with the specified backend connector.
    ///
    /// Starts a dedicated background worker thread managing the lifecycle of connector `C`.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError::Worker`] if spawning or initializing the background worker thread fails.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "opc-da-backend")]
    /// # fn run() -> opc_da_client::OpcResult<()> {
    /// use opc_da_client::{ComConnector, OpcDaClient};
    ///
    /// let connector = ComConnector::new();
    /// let client = OpcDaClient::new(connector)?;
    /// assert!(client.endpoint().is_none());
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(connector), err)]
    pub fn new(connector: C) -> OpcResult<Self> {
        let worker = ComWorker::start(Arc::new(connector))?;
        Ok(Self {
            worker: Arc::new(worker),
            endpoint: None,
            timeout: None,
            _state: std::marker::PhantomData,
        })
    }
}

impl<C: ServerBackend + 'static, State: Send + Sync + 'static> OpcDaClient<C, State> {
    #[must_use]
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout
    }

    pub(crate) async fn dispatch_request<F, R>(&self, req_builder: F) -> OpcResult<R>
    where
        F: FnOnce(tokio::sync::oneshot::Sender<OpcResult<R>>) -> ComRequest,
    {
        let fut = self.worker.send_request(req_builder);
        if let Some(dur) = self.timeout {
            match tokio::time::timeout(dur, fut).await {
                Ok(res) => res,
                Err(_) => Err(OpcError::Timeout(dur)),
            }
        } else {
            fut.await
        }
    }
}

#[cfg(feature = "test-support")]
pub type MockOpcDaClient = OpcDaClient<crate::connector::MockServerConnector>;

#[cfg(feature = "test-support")]
impl Default for OpcDaClient<crate::connector::MockServerConnector> {
    fn default() -> Self {
        match Self::new(crate::connector::MockServerConnector::default()) {
            Ok(client) => client,
            Err(e) => unreachable!("mock client initializes successfully: {e:?}"),
        }
    }
}
