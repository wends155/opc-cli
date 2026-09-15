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

use crate::com::connector::ComConnector;
use crate::com::worker::{ComRequest, ComWorker};
use crate::connector::ServerBackend;
use crate::errors::{OpcError, OpcResult};
use crate::types::{OpcServerEndpoint, ServerIdentifier};

#[cfg(feature = "opc-da-backend")]
pub type DefaultOpcDaClient<State = Unbound> = OpcDaClient<ComConnector, State>;

/// High-level client facade parameterized by connector backend `C` and typestate `State`.
pub struct OpcDaClient<C: ServerBackend + 'static = ComConnector, State = Unbound> {
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

impl OpcDaClient<ComConnector, Unbound> {
    #[must_use]
    pub fn builder() -> OpcDaClientBuilder<ComConnector> {
        OpcDaClientBuilder::new()
    }

    pub fn connect(
        server: impl Into<ServerIdentifier>,
    ) -> OpcResult<OpcDaClient<ComConnector, Bound>> {
        let endpoint = OpcServerEndpoint::from(server.into());
        let client = Self::new(ComConnector::new())?;
        Ok(client.bind(endpoint))
    }

    pub fn connect_remote(
        host: impl Into<String>,
        server: impl Into<ServerIdentifier>,
    ) -> OpcResult<OpcDaClient<ComConnector, Bound>> {
        let endpoint = OpcServerEndpoint::remote(host, server);
        let client = Self::new(ComConnector::new())?;
        Ok(client.bind(endpoint))
    }
}

impl<C: ServerBackend + 'static> OpcDaClient<C, Unbound> {
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
