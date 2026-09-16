//! Fluent builder for configuring and constructing an [`OpcDaClient`].

use crate::client::typestate::{Bound, Unbound};
use crate::client::{DefaultBackendConnector, OpcDaClient};
#[cfg(feature = "opc-da-backend")]
use crate::com::connector::ComConnector;
use crate::connector::ServerBackend;
use crate::errors::{OpcError, OpcResult};
use crate::types::{OpcServerEndpoint, ServerIdentifier, normalize_host};
use std::time::Duration;

/// Fluent builder for configuring and constructing an [`OpcDaClient`].
#[derive(Debug, Clone)]
pub struct OpcDaClientBuilder<C = DefaultBackendConnector> {
    pub(crate) host: Option<String>,
    pub(crate) server: Option<ServerIdentifier>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) legacy_dcom: bool,
    pub(crate) connector: Option<C>,
    pub(crate) server_err: Option<OpcError>,
}

impl OpcDaClientBuilder<DefaultBackendConnector> {
    /// Creates a new default client builder targeting the default backend connector.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::OpcDaClientBuilder;
    ///
    /// let builder = OpcDaClientBuilder::new();
    /// assert!(builder.timeout_duration().is_none());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            host: None,
            server: None,
            timeout: None,
            legacy_dcom: false,
            connector: Some(DefaultBackendConnector::default()),
            server_err: None,
        }
    }
}

#[cfg(feature = "opc-da-backend")]
impl OpcDaClientBuilder<ComConnector> {
    /// Configures legacy DCOM security blanketing.
    #[must_use]
    pub fn with_legacy_dcom(mut self, legacy_dcom: bool) -> Self {
        self.legacy_dcom = legacy_dcom;
        self.connector = Some(ComConnector::with_legacy_dcom(legacy_dcom));
        self
    }
}

impl<C: ServerBackend + 'static> OpcDaClientBuilder<C> {
    /// Creates a new client builder pre-configured with a custom backend connector.
    ///
    /// Available on all platforms, including offline and test builds.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcDaClientBuilder, connector::NoopServerBackend};
    ///
    /// let builder = OpcDaClientBuilder::new_with_connector(NoopServerBackend);
    /// assert!(builder.timeout_duration().is_none());
    /// ```
    #[must_use]
    pub fn new_with_connector(connector: C) -> Self {
        Self {
            host: None,
            server: None,
            timeout: None,
            legacy_dcom: false,
            connector: Some(connector),
            server_err: None,
        }
    }

    /// Sets the target remote host (or `"localhost"`).
    #[must_use]
    pub fn host(mut self, host: impl Into<String>) -> Self {
        let h = host.into();
        if h.contains('\0') {
            let _ = self.server_err.get_or_insert_with(|| {
                OpcError::InvalidState("Host cannot contain interior null bytes".to_string())
            });
        } else {
            self.host = Some(h);
        }
        self
    }

    /// Sets the target server identifier (ProgID or CLSID).
    #[must_use]
    pub fn server(mut self, server: impl TryInto<ServerIdentifier, Error: Into<OpcError>>) -> Self {
        match server.try_into() {
            Ok(id) => self.server = Some(id),
            Err(err) => {
                let _ = self.server_err.get_or_insert_with(|| err.into());
            }
        }
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
            server_err: self.server_err,
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
        server_err: Option<OpcError>,
    ) -> OpcResult<OpcDaClient<C, Unbound>> {
        if let Some(err) = server_err {
            return Err(err);
        }
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
    ///
    /// # Errors
    ///
    /// Returns [`OpcError::Worker`] if the background worker thread fails to initialize.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use opc_da_client::{OpcDaClientBuilder, connector::NoopServerBackend, errors::OpcResult};
    ///
    /// # fn run() -> OpcResult<()> {
    /// let client = OpcDaClientBuilder::new_with_connector(NoopServerBackend)
    ///     .build_with_connector(NoopServerBackend)?;
    /// assert!(client.endpoint().is_none());
    /// # Ok(())
    /// # }
    /// ```
    pub fn build_with_connector(self, connector: C) -> OpcResult<OpcDaClient<C, Unbound>> {
        Self::build_internal(
            connector,
            self.host.as_deref(),
            self.server,
            self.timeout,
            self.server_err,
        )
    }
}

impl<C: ServerBackend + Default + 'static> OpcDaClientBuilder<C> {
    /// Builds the `OpcDaClient` using the configured options and default connector.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError::Worker`] if the background worker thread fails to initialize.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use opc_da_client::{OpcDaClientBuilder, errors::OpcResult};
    ///
    /// # fn run() -> OpcResult<()> {
    /// let client = OpcDaClientBuilder::new().build()?;
    /// assert!(client.endpoint().is_none());
    /// # Ok(())
    /// # }
    /// ```
    pub fn build(self) -> OpcResult<OpcDaClient<C, Unbound>> {
        let connector = self.connector.unwrap_or_default();
        Self::build_internal(
            connector,
            self.host.as_deref(),
            self.server,
            self.timeout,
            self.server_err,
        )
    }

    /// Builds the `OpcDaClient` directly in the [`Bound`] typestate.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError::InvalidState`] if no server identifier has been configured.
    /// Returns [`OpcError::Worker`] if worker thread initialization fails.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use opc_da_client::{OpcDaClientBuilder, errors::OpcResult};
    ///
    /// # fn run() -> OpcResult<()> {
    /// let client = OpcDaClientBuilder::new()
    ///     .server("Matrikon.OPC.Simulation.1")
    ///     .build_bound()?;
    /// assert_eq!(client.server_id(), "Matrikon.OPC.Simulation.1");
    /// # Ok(())
    /// # }
    /// ```
    pub fn build_bound(self) -> OpcResult<OpcDaClient<C, Bound>> {
        if let Some(ref err) = self.server_err {
            return Err(err.clone());
        }
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
