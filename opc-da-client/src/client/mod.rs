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
use crate::types::OpcServerEndpoint;
#[cfg(feature = "opc-da-backend")]
use crate::types::ServerIdentifier;

#[cfg(feature = "opc-da-backend")]
pub type DefaultOpcDaClient<State = Unbound> = OpcDaClient<ComConnector, State>;

/// High-level client facade parameterized by connector backend `C` and typestate `State`.
#[cfg(feature = "opc-da-backend")]
pub struct OpcDaClient<C: ServerBackend + 'static = ComConnector, State = Unbound> {
    pub(crate) worker: Arc<ComWorker<C>>,
    pub(crate) endpoint: Option<OpcServerEndpoint>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) _state: std::marker::PhantomData<State>,
}

/// High-level client facade parameterized by connector backend `C` and typestate `State`.
#[cfg(all(not(feature = "opc-da-backend"), any(test, feature = "test-support")))]
pub struct OpcDaClient<
    C: ServerBackend + 'static = crate::connector::MockServerConnector,
    State = Unbound,
> {
    pub(crate) worker: Arc<ComWorker<C>>,
    pub(crate) endpoint: Option<OpcServerEndpoint>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) _state: std::marker::PhantomData<State>,
}

/// High-level client facade parameterized by connector backend `C` and typestate `State`.
#[cfg(all(
    not(feature = "opc-da-backend"),
    not(any(test, feature = "test-support"))
))]
pub struct OpcDaClient<C: ServerBackend + 'static, State = Unbound> {
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

#[cfg(feature = "opc-da-backend")]
impl OpcDaClient<ComConnector, Unbound> {
    #[must_use]
    pub fn builder() -> OpcDaClientBuilder<ComConnector> {
        OpcDaClientBuilder::new()
    }

    /// Constructs a client bound to a local OPC DA server by ProgID or CLSID.
    ///
    /// # Errors
    /// Returns [`OpcError::Worker`] if the COM worker thread initialization fails.
    ///
    /// # Examples
    /// ```no_run
    /// use opc_da_client::OpcDaClient;
    ///
    /// # fn run() -> opc_da_client::OpcResult<()> {
    /// let client = OpcDaClient::bind_new("Matrikon.OPC.Simulation.1")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn bind_new(
        server: impl Into<ServerIdentifier>,
    ) -> OpcResult<OpcDaClient<ComConnector, Bound>> {
        let endpoint = OpcServerEndpoint::from(server.into());
        let client = Self::new(ComConnector::new())?;
        Ok(client.bind(endpoint))
    }

    /// Constructs a client bound to a remote OPC DA server by host and ProgID or CLSID.
    ///
    /// # Errors
    /// Returns [`OpcError::Worker`] if the COM worker thread initialization fails.
    pub fn bind_new_remote(
        host: impl Into<String>,
        server: impl Into<ServerIdentifier>,
    ) -> OpcResult<OpcDaClient<ComConnector, Bound>> {
        let endpoint = OpcServerEndpoint::remote(host, server);
        let client = Self::new(ComConnector::new())?;
        Ok(client.bind(endpoint))
    }

    #[deprecated(
        since = "0.2.1",
        note = "Use `bind_new` to construct a bound client, or follow with `.connect_eager().await` to actively probe server liveness."
    )]
    pub fn connect(
        server: impl Into<ServerIdentifier>,
    ) -> OpcResult<OpcDaClient<ComConnector, Bound>> {
        Self::bind_new(server)
    }

    #[deprecated(
        since = "0.2.1",
        note = "Use `bind_new_remote` to construct a bound client, or follow with `.connect_eager().await` to actively probe server liveness."
    )]
    pub fn connect_remote(
        host: impl Into<String>,
        server: impl Into<ServerIdentifier>,
    ) -> OpcResult<OpcDaClient<ComConnector, Bound>> {
        Self::bind_new_remote(host, server)
    }
}

#[cfg(all(not(feature = "opc-da-backend"), any(test, feature = "test-support")))]
impl OpcDaClient<crate::connector::MockServerConnector, Unbound> {
    #[must_use]
    pub fn builder() -> OpcDaClientBuilder<crate::connector::MockServerConnector> {
        OpcDaClientBuilder::default()
    }
}

impl<C: ServerBackend + 'static> OpcDaClient<C, Unbound> {
    /// Creates a builder targeting a specific backend connector.
    #[must_use]
    pub fn builder_with_connector(connector: C) -> OpcDaClientBuilder<C> {
        OpcDaClientBuilder::new_with_connector(connector)
    }
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
