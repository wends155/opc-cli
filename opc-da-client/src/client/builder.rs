//! Fluent builder for configuring and constructing an [`OpcDaClient`].

use crate::client::OpcDaClient;
use crate::client::typestate::{Bound, Unbound};
#[cfg(feature = "opc-da-backend")]
use crate::com::connector::ComConnector;
use crate::connector::ServerBackend;
use crate::errors::{OpcError, OpcResult};
use crate::types::{OpcServerEndpoint, ServerIdentifier, normalize_host};
use std::time::Duration;

/// Fluent builder for configuring and constructing an [`OpcDaClient`].
#[cfg(feature = "opc-da-backend")]
#[derive(Debug, Clone)]
pub struct OpcDaClientBuilder<C = ComConnector> {
    pub(crate) host: Option<String>,
    pub(crate) server: Option<ServerIdentifier>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) legacy_dcom: bool,
    pub(crate) connector: Option<C>,
}

/// Fluent builder for configuring and constructing an [`OpcDaClient`].
#[cfg(all(not(feature = "opc-da-backend"), any(test, feature = "test-support")))]
#[derive(Debug, Clone)]
pub struct OpcDaClientBuilder<C = crate::connector::MockServerConnector> {
    pub(crate) host: Option<String>,
    pub(crate) server: Option<ServerIdentifier>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) legacy_dcom: bool,
    pub(crate) connector: Option<C>,
}

/// Fluent builder for configuring and constructing an [`OpcDaClient`].
#[cfg(all(
    not(feature = "opc-da-backend"),
    not(any(test, feature = "test-support"))
))]
#[derive(Debug, Clone)]
pub struct OpcDaClientBuilder<C> {
    pub(crate) host: Option<String>,
    pub(crate) server: Option<ServerIdentifier>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) legacy_dcom: bool,
    pub(crate) connector: Option<C>,
}

#[cfg(feature = "opc-da-backend")]
impl OpcDaClientBuilder<ComConnector> {
    /// Creates a new default client builder targeting the standard Windows [`ComConnector`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            host: None,
            server: None,
            timeout: None,
            legacy_dcom: false,
            connector: Some(ComConnector::new()),
        }
    }

    /// Configures legacy DCOM security blanketing.
    #[must_use]
    pub fn with_legacy_dcom(mut self, legacy: bool) -> Self {
        self.legacy_dcom = legacy;
        self.connector = Some(ComConnector::with_legacy_dcom(legacy));
        self
    }
}

impl<C: ServerBackend + 'static> OpcDaClientBuilder<C> {
    /// Creates a new client builder pre-configured with a custom backend connector.
    /// Available on all platforms, including offline builds.
    #[must_use]
    pub fn new_with_connector(connector: C) -> Self {
        Self {
            host: None,
            server: None,
            timeout: None,
            legacy_dcom: false,
            connector: Some(connector),
        }
    }

    /// Sets the target remote host (or `"localhost"`).
    #[must_use]
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Sets the target server identifier (ProgID or CLSID).
    #[must_use]
    pub fn server(mut self, server: impl Into<ServerIdentifier>) -> Self {
        self.server = Some(server.into());
        self
    }

    /// Sets operation timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Transitions the builder to use an alternative backend connector (e.g., a test mock).
    #[must_use]
    pub fn with_connector<C2: ServerBackend + 'static>(
        self,
        connector: C2,
    ) -> OpcDaClientBuilder<C2> {
        OpcDaClientBuilder {
            host: self.host,
            server: self.server,
            timeout: self.timeout,
            legacy_dcom: self.legacy_dcom,
            connector: Some(connector),
        }
    }

    /// Returns the configured timeout duration for this builder, if any.
    #[must_use]
    pub fn timeout_duration(&self) -> Option<Duration> {
        self.timeout
    }

    /// Returns whether legacy DCOM authentication is enabled for this builder.
    #[must_use]
    pub fn legacy_dcom(&self) -> bool {
        self.legacy_dcom
    }

    fn build_internal(
        connector: C,
        host: Option<&str>,
        server: Option<ServerIdentifier>,
        timeout: Option<Duration>,
    ) -> OpcResult<OpcDaClient<C, Unbound>> {
        let mut client = OpcDaClient::new(connector)?;
        client.timeout = timeout;
        if let Some(server) = server {
            let host = normalize_host(host);
            client.endpoint = Some(OpcServerEndpoint {
                host,
                identifier: server,
            });
        }
        Ok(client)
    }

    /// Builds the `OpcDaClient` using an explicit connector instance.
    pub fn build_with_connector(self, connector: C) -> OpcResult<OpcDaClient<C, Unbound>> {
        Self::build_internal(connector, self.host.as_deref(), self.server, self.timeout)
    }
}

impl<C: ServerBackend + Default + 'static> OpcDaClientBuilder<C> {
    /// Builds the `OpcDaClient` using the configured options and default connector.
    pub fn build(self) -> OpcResult<OpcDaClient<C, Unbound>> {
        let connector = self.connector.unwrap_or_default();
        Self::build_internal(connector, self.host.as_deref(), self.server, self.timeout)
    }

    /// Builds the `OpcDaClient` directly in the [`Bound`] typestate.
    ///
    /// # Errors
    /// Returns [`OpcError::InvalidState`] if no server identifier has been configured.
    /// Returns [`OpcError::Worker`] if worker thread initialization fails.
    pub fn build_bound(self) -> OpcResult<OpcDaClient<C, Bound>> {
        if self.server.is_none() {
            return Err(OpcError::InvalidState(
                "Cannot build bound client without configuring server identifier".into(),
            ));
        }
        let unbound = self.build()?;
        let ep = unbound.endpoint.clone().ok_or_else(|| {
            OpcError::InvalidState(
                "Cannot build bound client without configuring server identifier".into(),
            )
        })?;
        Ok(unbound.bind(ep))
    }
}

impl<C: ServerBackend + Default + 'static> Default for OpcDaClientBuilder<C> {
    fn default() -> Self {
        Self::new_with_connector(C::default())
    }
}
