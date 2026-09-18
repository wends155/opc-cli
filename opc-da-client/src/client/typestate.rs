//! Compile-time typestate markers and state transitions for [`OpcDaClient`].

use crate::client::OpcDaClient;
use crate::connector::ServerBackend;
use crate::errors::OpcResult;
use crate::types::{OpcServerEndpoint, ServerIdentifier};
use std::borrow::Cow;

/// Marker typestate indicating an unbound [`OpcDaClient`] gateway capable of multi-server operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Unbound;

/// Marker typestate indicating a bound single-server session [`OpcDaClient`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Bound;

impl<C: ServerBackend + 'static> OpcDaClient<C, Unbound> {
    /// Returns the target OPC server endpoint if configured on this unbound client.
    #[must_use]
    pub fn endpoint(&self) -> Option<&OpcServerEndpoint> {
        self.endpoint.as_ref()
    }

    /// Binds the unbound client to a target endpoint, transitioning it to the [`Bound`] typestate.
    #[must_use]
    pub fn bind<E: Into<OpcServerEndpoint>>(self, endpoint: E) -> OpcDaClient<C, Bound> {
        let ep = endpoint.into();
        OpcDaClient {
            worker: self.worker,
            endpoint: Some(ep),
            timeout: self.timeout,
            _state: std::marker::PhantomData,
        }
    }

    /// Binds the unbound client to a remote OPC DA server by host and identifier.
    #[must_use]
    pub fn bind_remote(
        self,
        host: impl Into<String>,
        server: impl Into<ServerIdentifier>,
    ) -> OpcDaClient<C, Bound> {
        self.bind(OpcServerEndpoint::remote(host, server))
    }
}

impl<C: ServerBackend + 'static> OpcDaClient<C, Bound> {
    /// Returns the target OPC server endpoint guaranteed to be present in the [`Bound`] typestate.
    ///
    /// # Panics
    /// Panics if the internal endpoint field is absent, which represents an invariant violation of [`Bound`].
    #[must_use]
    pub fn endpoint(&self) -> &OpcServerEndpoint {
        match &self.endpoint {
            Some(ep) => ep,
            None => unreachable!("Bound typestate invariant: endpoint is always Some"),
        }
    }

    /// Returns the server identifier string (ProgID or CLSID) for this bound session.
    #[must_use]
    pub fn server_id(&self) -> Cow<'_, str> {
        match &self.endpoint().identifier {
            ServerIdentifier::ProgId(prog_id) => Cow::Borrowed(prog_id.as_str()),
            ServerIdentifier::Clsid(clsid) => Cow::Owned(clsid.to_bracketed()),
        }
    }

    /// Unbinds the client from its endpoint, returning the unbound gateway and the previous endpoint.
    #[must_use]
    pub fn unbind(self) -> (OpcDaClient<C, Unbound>, OpcServerEndpoint) {
        let Some(ep) = self.endpoint else {
            unreachable!("Bound typestate invariant: endpoint is always Some");
        };
        (
            OpcDaClient {
                worker: self.worker,
                endpoint: None,
                timeout: self.timeout,
                _state: std::marker::PhantomData,
            },
            ep,
        )
    }

    /// Eagerly verifies active connectivity and reachability to the configured OPC DA server.
    ///
    /// Sends an asynchronous ping request to the background COM worker to verify that the
    /// configured server can be instantiated and reached on the network or locally.
    ///
    /// # Errors
    ///
    /// Returns [`crate::errors::OpcError::Connection`] if the server cannot be instantiated or reached,
    /// [`crate::errors::OpcError::Timeout`] if connectivity verification exceeds the configured timeout,
    /// or [`crate::errors::OpcError::Worker`] if communication with the background worker fails.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C, opc_da_client::client::typestate::Bound>) -> OpcResult<()> {
    /// client.connect_eager().await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn connect_eager(&self) -> OpcResult<()> {
        self.dispatch_request(|reply| crate::com::worker::ComRequest::Ping {
            endpoint: self.endpoint().clone(),
            reply,
        })
        .await
    }
}
