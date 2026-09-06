use crate::com::connector::{ComConnector, ServerConnector};
use crate::com::worker::{ComRequest, ComWorker};
use crate::errors::{OpcError, OpcResult};
use crate::provider::{
    OpcValue, ServerDiscovery, TagBrowser, TagCollector, TagReader, TagValue, TagWriter,
    WriteResult,
};
use crate::types::{
    IntoTags, OpcServerEndpoint, OpcServerInfo, ServerIdentifier, TagBatch, TagValues,
};
use async_trait::async_trait;
use std::sync::Arc;

/// Fluent builder for configuring and constructing an [`OpcDaClient`].
#[derive(Debug, Clone)]
pub struct OpcDaClientBuilder<C = ComConnector> {
    host: Option<String>,
    server: Option<ServerIdentifier>,
    pub timeout: Option<std::time::Duration>,
    pub legacy_dcom: bool,
    connector: Option<C>,
}

impl Default for OpcDaClientBuilder<ComConnector> {
    fn default() -> Self {
        Self::new()
    }
}

impl OpcDaClientBuilder<ComConnector> {
    /// Creates a new default client builder targeting the standard Windows [`ComConnector`].
    ///
    /// # Returns
    ///
    /// A new [`OpcDaClientBuilder`] initialized with default options.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::OpcDaClientBuilder;
    ///
    /// let builder = OpcDaClientBuilder::new();
    /// ```
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
    ///
    /// Sets authentication level to `RPC_C_AUTHN_LEVEL_CONNECT` (2) instead of
    /// modern post-KB5004442 `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY` (5).
    ///
    /// # Arguments
    ///
    /// * `legacy` - `true` to enable legacy DCOM packet authentication.
    ///
    /// # Returns
    ///
    /// The updated builder instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::OpcDaClientBuilder;
    ///
    /// let builder = OpcDaClientBuilder::new().with_legacy_dcom(true);
    /// ```
    #[must_use]
    pub fn with_legacy_dcom(mut self, legacy: bool) -> Self {
        self.legacy_dcom = legacy;
        self.connector = Some(ComConnector::with_legacy_dcom(legacy));
        self
    }
}

impl<C: ServerConnector + 'static> OpcDaClientBuilder<C> {
    /// Sets the target remote host (or `"localhost"`).
    ///
    /// # Arguments
    ///
    /// * `host` - Target host IP address or hostname.
    ///
    /// # Returns
    ///
    /// The updated builder instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::OpcDaClientBuilder;
    ///
    /// let builder = OpcDaClientBuilder::new().host("192.168.1.10");
    /// ```
    #[must_use]
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = Some(host.into());
        self
    }

    /// Sets the target server identifier (ProgID or CLSID).
    ///
    /// # Arguments
    ///
    /// * `server` - OPC server ProgID string or GUID CLSID.
    ///
    /// # Returns
    ///
    /// The updated builder instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::OpcDaClientBuilder;
    ///
    /// let builder = OpcDaClientBuilder::new().server("Matrikon.OPC.Simulation.1");
    /// ```
    #[must_use]
    pub fn server(mut self, server: impl Into<ServerIdentifier>) -> Self {
        self.server = Some(server.into());
        self
    }

    /// Sets operation timeout.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Duration before operations time out.
    ///
    /// # Returns
    ///
    /// The updated builder instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Duration;
    /// use opc_da_client::OpcDaClientBuilder;
    ///
    /// let builder = OpcDaClientBuilder::new().timeout(Duration::from_secs(5));
    /// ```
    #[must_use]
    pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Transitions the builder to use an alternative backend connector (e.g., a test mock).
    ///
    /// # Arguments
    ///
    /// * `connector` - Backend connector implementing [`ServerConnector`].
    ///
    /// # Returns
    ///
    /// A new builder parameterized by the connector type `C2`.
    #[must_use]
    pub fn with_connector<C2: ServerConnector + 'static>(
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

    /// Builds the `OpcDaClient` using an explicit connector instance.
    ///
    /// # Arguments
    ///
    /// * `connector` - Connector instance to use.
    fn build_internal(
        connector: C,
        host: Option<String>,
        server: Option<ServerIdentifier>,
        timeout: Option<std::time::Duration>,
    ) -> OpcResult<OpcDaClient<C, Unbound>> {
        let mut client = OpcDaClient::new(connector)?;
        client.timeout = timeout;
        if let Some(server) = server {
            client.endpoint = Some(OpcServerEndpoint {
                host,
                identifier: server,
            });
        }
        Ok(client)
    }

    /// Builds the `OpcDaClient` using an explicit connector instance.
    ///
    /// # Arguments
    ///
    /// * `connector` - Connector instance to use.
    ///
    /// # Returns
    ///
    /// A new [`OpcDaClient`] configured with the builder options in the [`Unbound`] typestate.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError::Connection`] if worker thread initialization fails.
    pub fn build_with_connector(self, connector: C) -> OpcResult<OpcDaClient<C, Unbound>> {
        Self::build_internal(connector, self.host, self.server, self.timeout)
    }
}

impl<C: ServerConnector + Default + 'static> OpcDaClientBuilder<C> {
    /// Builds the `OpcDaClient` using the configured options and default connector.
    ///
    /// # Returns
    ///
    /// A configured [`OpcDaClient`] in the [`Unbound`] typestate.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError::Connection`] if worker thread initialization fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use opc_da_client::OpcDaClientBuilder;
    ///
    /// let client = OpcDaClientBuilder::new()
    ///     .server("Matrikon.OPC.Simulation.1")
    ///     .build()?;
    /// # Ok::<(), opc_da_client::OpcError>(())
    /// ```
    pub fn build(self) -> OpcResult<OpcDaClient<C, Unbound>> {
        let connector = self.connector.unwrap_or_default();
        Self::build_internal(connector, self.host, self.server, self.timeout)
    }

    /// Builds the `OpcDaClient` directly in the [`Bound`] typestate.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError::InvalidState`] if the builder does not have a server identifier configured.
    /// Returns [`OpcError::Connection`] if worker thread initialization fails.
    pub fn build_bound(self) -> OpcResult<OpcDaClient<C, Bound>> {
        let unbound = self.build()?;
        let ep = unbound.endpoint.clone().ok_or_else(|| {
            OpcError::InvalidState(
                "Cannot build bound client without configuring server identifier".into(),
            )
        })?;
        Ok(unbound.bind(ep))
    }
}

/// Marker typestate indicating an unbound [`OpcDaClient`] gateway capable of multi-server operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Unbound;

/// Marker typestate indicating a bound single-server session [`OpcDaClient`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Bound;

/// Concrete [`OpcProvider`] implementation for Windows OPC DA.
///
/// Uses native `windows-rs` COM interop via the internal `com` subsystem.
pub struct OpcDaClient<C: ServerConnector + 'static = ComConnector, State = Unbound> {
    /// Background MTA worker handle managing asynchronous request channels.
    pub(crate) worker: Arc<ComWorker<C>>,
    /// Target OPC server endpoint if bound to a specific server.
    pub endpoint: Option<OpcServerEndpoint>,
    /// Configured operation timeout.
    pub timeout: Option<std::time::Duration>,
    pub(crate) _state: std::marker::PhantomData<State>,
}

impl<C: ServerConnector + 'static, State> Clone for OpcDaClient<C, State> {
    fn clone(&self) -> Self {
        Self {
            worker: Arc::clone(&self.worker),
            endpoint: self.endpoint.clone(),
            timeout: self.timeout,
            _state: std::marker::PhantomData,
        }
    }
}

impl OpcDaClient<ComConnector, Unbound> {
    /// Returns a new fluent builder for configuring and connecting an `OpcDaClient`.
    ///
    /// # Returns
    ///
    /// A new [`OpcDaClientBuilder`] targeting the standard [`ComConnector`].
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::OpcDaClient;
    ///
    /// let builder = OpcDaClient::builder()
    ///     .host("localhost")
    ///     .server("Matrikon.OPC.Simulation.1");
    /// ```
    #[must_use]
    pub fn builder() -> OpcDaClientBuilder<ComConnector> {
        OpcDaClientBuilder::new()
    }

    /// Quickly connects to a local OPC DA server by ProgID or CLSID.
    ///
    /// # Arguments
    ///
    /// * `server` - Target OPC server ProgID or GUID CLSID.
    ///
    /// # Returns
    ///
    /// A connected [`OpcDaClient`] bound to the target server in the [`Bound`] typestate.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError::Connection`] if worker thread initialization fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use opc_da_client::OpcDaClient;
    ///
    /// let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;
    /// # Ok::<(), opc_da_client::OpcError>(())
    /// ```
    pub fn connect(
        server: impl Into<ServerIdentifier>,
    ) -> OpcResult<OpcDaClient<ComConnector, Bound>> {
        let endpoint = OpcServerEndpoint::from(server.into());
        let client = Self::new(ComConnector::new())?;
        Ok(client.bind(endpoint))
    }

    /// Quickly connects to a remote OPC DA server by host and ProgID or CLSID.
    ///
    /// # Arguments
    ///
    /// * `host` - Target remote host IP address or hostname.
    /// * `server` - Target OPC server ProgID or GUID CLSID.
    ///
    /// # Returns
    ///
    /// A connected [`OpcDaClient`] bound to the target host and server in the [`Bound`] typestate.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError::Connection`] if worker thread initialization fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use opc_da_client::OpcDaClient;
    ///
    /// let client = OpcDaClient::connect_remote("192.168.1.10", "Matrikon.OPC.Simulation.1")?;
    /// # Ok::<(), opc_da_client::OpcError>(())
    /// ```
    pub fn connect_remote(
        host: impl Into<String>,
        server: impl Into<ServerIdentifier>,
    ) -> OpcResult<OpcDaClient<ComConnector, Bound>> {
        let endpoint = OpcServerEndpoint {
            host: Some(host.into()),
            identifier: server.into(),
        };
        let client = Self::new(ComConnector::new())?;
        Ok(client.bind(endpoint))
    }
}

impl<C: ServerConnector + 'static> OpcDaClient<C, Unbound> {
    /// Creates a new `OpcDaClient` with the given connector in the [`Unbound`] typestate.
    ///
    /// # Arguments
    ///
    /// * `connector` - Backend connector implementing [`ServerConnector`].
    ///
    /// # Returns
    ///
    /// A new unbound [`OpcDaClient`] instance ready for communication.
    ///
    /// # Errors
    ///
    /// Returns [`crate::errors::OpcError::Connection`] if the background COM worker thread
    /// fails to spawn or MTA apartment initialization fails.
    #[tracing::instrument(level = "info", skip(connector), err)]
    pub fn new(connector: C) -> OpcResult<Self> {
        tracing::info!("Initializing OpcDaClient...");
        let worker = ComWorker::start(Arc::new(connector))?;
        tracing::info!("OpcDaClient initialized successfully");
        Ok(Self {
            worker: Arc::new(worker),
            endpoint: None,
            timeout: None,
            _state: std::marker::PhantomData,
        })
    }

    /// Returns the target OPC server endpoint if bound to a specific server.
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
        self.bind(OpcServerEndpoint {
            host: Some(host.into()),
            identifier: server.into(),
        })
    }
}

impl<C: ServerConnector + 'static> OpcDaClient<C, Bound> {
    /// Returns the target OPC server endpoint guaranteed to be present in the [`Bound`] typestate.
    ///
    /// # Panics
    ///
    /// Panics if the internal endpoint field is absent, which represents an invariant violation of [`Bound`].
    #[must_use]
    pub fn endpoint(&self) -> &OpcServerEndpoint {
        self.endpoint
            .as_ref()
            .expect("Bound typestate invariant: endpoint is always Some")
    }

    /// Returns the server identifier string (ProgID or CLSID) for this bound session.
    #[must_use]
    pub fn server_id(&self) -> &str {
        match &self.endpoint().identifier {
            ServerIdentifier::ProgId(prog_id) => prog_id.as_str(),
            ServerIdentifier::Clsid(_) => "{CLSID}",
        }
    }

    /// Unbinds the client from its endpoint, returning the unbound gateway and the previous endpoint.
    ///
    /// # Panics
    ///
    /// Panics if the internal endpoint field is absent, which represents an invariant violation of [`Bound`].
    #[must_use]
    pub fn unbind(self) -> (OpcDaClient<C, Unbound>, OpcServerEndpoint) {
        let ep = self
            .endpoint
            .expect("Bound typestate invariant: endpoint is always Some");
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

    /// Reads a single tag and returns its full [`TagValue`].
    pub async fn read_tag(&self, tag: &str) -> OpcResult<TagValue> {
        self.read_tag_value(tag).await
    }

    /// Reads a batch of tags and returns their [`TagValues`].
    pub async fn read_tags(&self, tags: impl IntoTags) -> OpcResult<TagValues> {
        self.read_tag_values(tags).await
    }

    /// Writes a typed value to a tag on the bound server.
    pub async fn write_tag(&self, tag: &str, value: impl Into<OpcValue>) -> OpcResult<WriteResult> {
        self.write(tag, value).await
    }

    /// Writes a batch of tag-value pairs to the bound server.
    pub async fn write_tags(&self, writes: Vec<(String, OpcValue)>) -> OpcResult<Vec<WriteResult>> {
        self.write_batch(writes).await
    }
}

impl<C: ServerConnector + 'static, State: Send + Sync + 'static> OpcDaClient<C, State> {
    /// Eagerly verifies active connectivity and reachability to the configured OPC DA server.
    ///
    /// Dispatches an initial probe request to the COM worker thread to verify that
    /// the target server can be reached and instantiated via COM/DCOM.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError::InvalidState`] if the client is not bound to a server endpoint.
    /// Returns [`OpcError::Connection`] if connecting or communicating with the server fails.
    pub async fn connect_eager(&self) -> OpcResult<()> {
        let _ = self.read_tag_values(TagBatch::default()).await?;
        Ok(())
    }

    /// Dispatches a request to the COM worker thread, applying timeout if configured.
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

    /// Asynchronously reads current values, quality, and timestamps for a batch of tags.
    ///
    /// Accepts any type implementing [`IntoTags`] (slices, arrays, vectors, single tag strings)
    /// without requiring intermediate heap allocations.
    ///
    /// # Arguments
    ///
    /// * `tags` - Tag batch implementing [`IntoTags`].
    ///
    /// # Returns
    ///
    /// A [`TagValues`] collection containing the read results.
    ///
    /// # Errors
    ///
    /// * [`OpcError::InvalidState`] - Client is not bound to a server.
    /// * [`OpcError::Connection`] - DCOM connection failure or server disconnect.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// use opc_da_client::OpcDaClient;
    ///
    /// let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;
    /// let values = client.read_tag_values(["Random.Int4", "Random.Real8"]).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self, tags), err)]
    pub async fn read_tag_values(&self, tags: impl IntoTags) -> OpcResult<TagValues> {
        let endpoint = self.endpoint.as_ref().ok_or_else(|| {
            OpcError::InvalidState(
                "Client is not bound to a server. Use OpcDaClient::builder().server(...) or OpcProvider::read_tag_values"
                    .into(),
            )
        })?.clone();
        let batch = tags.into_tag_batch();
        self.dispatch_request(|reply| ComRequest::ReadTagValues {
            endpoint,
            tags: batch,
            reply,
        })
        .await
    }

    /// Reads a single tag and unwraps its value using the supplied extractor closure.
    async fn read_single_typed<T, F>(&self, tag: &str, extract: F) -> OpcResult<T>
    where
        F: FnOnce(&TagValues, &str) -> Result<T, crate::types::TagExtractError>,
    {
        let batch = TagBatch::from_str_lenient(tag);
        let values = self.read_tag_values(batch).await?;
        extract(&values, tag).map_err(Into::into)
    }

    /// Reads a single tag and unwraps its value as an `f64`.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string.
    ///
    /// # Returns
    ///
    /// Decoded `f64` value.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError`] if the read fails, the tag was not found, or the value cannot be converted to `f64`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// use opc_da_client::OpcDaClient;
    ///
    /// let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;
    /// let temp = client.read_f64("Random.Real8").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read_f64(&self, tag: &str) -> OpcResult<f64> {
        self.read_single_typed(tag, TagValues::get_f64).await
    }

    /// Reads a single tag and unwraps its value as an `i32`.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string.
    ///
    /// # Returns
    ///
    /// Decoded `i32` value.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError`] if the read fails, the tag was not found, or the value cannot be converted to `i32`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// use opc_da_client::OpcDaClient;
    ///
    /// let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;
    /// let count = client.read_i32("Random.Int4").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read_i32(&self, tag: &str) -> OpcResult<i32> {
        self.read_single_typed(tag, TagValues::get_i32).await
    }

    /// Reads a single tag and unwraps its value as a `bool`.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string.
    ///
    /// # Returns
    ///
    /// Decoded `bool` value.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError`] if the read fails, the tag was not found, or the value cannot be converted to `bool`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// use opc_da_client::OpcDaClient;
    ///
    /// let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;
    /// let flag = client.read_bool("Random.Bool").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read_bool(&self, tag: &str) -> OpcResult<bool> {
        self.read_single_typed(tag, TagValues::get_bool).await
    }

    /// Reads a single tag and unwraps its value as a `String`.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string.
    ///
    /// # Returns
    ///
    /// Decoded `String` value.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError`] if the read fails, the tag was not found, or the value cannot be converted to `String`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// use opc_da_client::OpcDaClient;
    ///
    /// let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;
    /// let text = client.read_string("Random.String").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read_string(&self, tag: &str) -> OpcResult<String> {
        self.read_single_typed(tag, |v, t| v.get_str(t).map(ToString::to_string))
            .await
    }

    /// Reads a single tag and returns its full [`TagValue`].
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string.
    ///
    /// # Returns
    ///
    /// Decoded [`TagValue`].
    ///
    /// # Errors
    ///
    /// Returns [`OpcError`] if the read fails or the client is not bound to a server.
    pub async fn read_tag_value(&self, tag: &str) -> OpcResult<TagValue> {
        let batch = TagBatch::from_str_lenient(tag);
        let values = self.read_tag_values(batch).await?;
        values.into_vec().pop().ok_or_else(|| {
            OpcError::Internal(format!("Tag '{tag}' returned no response from server"))
        })
    }

    /// Asynchronously writes a typed value to a tag.
    ///
    /// # Arguments
    ///
    /// * `tag` - Target tag identifier.
    /// * `value` - Value to write, convertible into [`OpcValue`].
    ///
    /// # Returns
    ///
    /// A [`WriteResult`] summarizing the write status.
    ///
    /// # Errors
    ///
    /// * [`OpcError::InvalidState`] - Client is not bound to a server.
    /// * [`OpcError::Connection`] - DCOM connection failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// use opc_da_client::OpcDaClient;
    ///
    /// let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;
    /// let res = client.write("Bucket Brigade.Int4", 100).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self, value), err)]
    pub async fn write(&self, tag: &str, value: impl Into<OpcValue>) -> OpcResult<WriteResult> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or_else(|| OpcError::InvalidState("Client is not bound to a server".into()))?
            .clone();
        self.dispatch_request(|reply| ComRequest::WriteTagValue {
            endpoint,
            tag_id: tag.to_string(),
            value: value.into(),
            reply,
        })
        .await
    }

    /// Asynchronously writes a batch of tag-value pairs in a single operation.
    ///
    /// Uses a single COM group and single atomic `SyncIO::Write` RPC roundtrip.
    ///
    /// # Arguments
    ///
    /// * `writes` - Vector of `(tag_id, value)` pairs to write.
    ///
    /// # Returns
    ///
    /// A vector of [`WriteResult`] items corresponding to the input writes in order.
    ///
    /// # Errors
    ///
    /// * [`OpcError::InvalidState`] - Client is not bound to a server.
    /// * [`OpcError::Connection`] - DCOM connection failure.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// use opc_da_client::{OpcDaClient, OpcValue};
    ///
    /// let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;
    /// let writes = vec![
    ///     ("Bucket Brigade.Int4".to_string(), OpcValue::Int(100)),
    ///     ("Bucket Brigade.Real8".to_string(), OpcValue::Float(99.5)),
    /// ];
    /// let results = client.write_batch(writes).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self, writes), err)]
    pub async fn write_batch(
        &self,
        writes: Vec<(String, OpcValue)>,
    ) -> OpcResult<Vec<WriteResult>> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or_else(|| OpcError::InvalidState("Client is not bound to a server".into()))?
            .clone();
        self.dispatch_request(|reply| ComRequest::WriteTagValues {
            endpoint,
            writes,
            reply,
        })
        .await
    }

    /// Lists available OPC servers on a remote (or local) host.
    ///
    /// # Deprecated
    ///
    /// Prefer calling [`ServerDiscovery::list_servers`] or [`OpcProvider::list_servers`].
    #[deprecated(since = "0.2.1", note = "use ServerDiscovery::list_servers instead")]
    pub async fn list_servers_on(&self, host: &str) -> OpcResult<Vec<String>> {
        self.list_servers(host).await
    }

    /// Subscribes to a stream of periodic tag value reads, returning an asynchronous [`tokio::sync::mpsc::Receiver`].
    ///
    /// Spawns a non-blocking background polling task. Dropping the returned receiver
    /// automatically cancels the background polling task.
    ///
    /// # Arguments
    ///
    /// * `tags` - Tag batch implementing [`IntoTags`].
    /// * `interval` - Polling interval duration.
    ///
    /// # Returns
    ///
    /// An asynchronous [`tokio::sync::mpsc::Receiver`] yielding [`TagValues`] updates.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// use opc_da_client::OpcDaClient;
    /// use std::time::Duration;
    ///
    /// let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;
    /// let mut rx = client.subscribe(["Random.Int4"], Duration::from_millis(500));
    /// if let Some(values) = rx.recv().await {
    ///     let _count = values.len();
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn subscribe(
        &self,
        tags: impl IntoTags,
        interval: std::time::Duration,
    ) -> tokio::sync::mpsc::Receiver<TagValues> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let tags_batch = tags.into_tag_batch().into_shareable();
        let client = self.clone();

        tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                timer.tick().await;
                match client.read_tag_values(tags_batch.clone()).await {
                    Ok(values) => {
                        if tx.send(values).await.is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        tracing::warn!(error = ?err, "Subscription polling tick failed");
                        if tx.is_closed() {
                            break;
                        }
                    }
                }
            }
        });

        rx
    }

    /// Asynchronously browses tags on the currently bound server.
    ///
    /// # Arguments
    ///
    /// * `collector` - Bounded [`TagCollector`] to accumulate discovered tags.
    ///
    /// # Returns
    ///
    /// A vector of discovered tag identifier strings.
    ///
    /// # Errors
    ///
    /// * [`OpcError::InvalidState`] - Client is not bound to a server.
    /// * [`OpcError::Connection`] - DCOM connection failure.
    #[tracing::instrument(level = "info", skip(self, collector), err)]
    pub async fn browse(&self, collector: TagCollector) -> OpcResult<Vec<String>> {
        let endpoint = self.endpoint.as_ref().ok_or_else(|| {
            OpcError::InvalidState(
                "Client is not bound to a server. Use OpcDaClient::builder().server(...) or OpcProvider::browse_tags"
                    .into(),
            )
        })?.clone();
        self.dispatch_request(|reply| ComRequest::BrowseTags {
            endpoint,
            collector,
            reply,
        })
        .await
    }

    /// Lists available OPC DA servers registered on the specified host.
    pub async fn list_servers(&self, host: &str) -> OpcResult<Vec<String>> {
        ServerDiscovery::list_servers(self, host).await
    }

    /// Lists available OPC DA servers with rich metadata registered on the specified host.
    pub async fn list_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        ServerDiscovery::list_server_details(self, host).await
    }

    /// Discovers available tag identifiers on a specified target server namespace.
    pub async fn browse_tags(
        &self,
        server: &str,
        collector: TagCollector,
    ) -> OpcResult<Vec<String>> {
        TagBrowser::browse_tags(self, server, collector).await
    }

    /// Asynchronously writes a typed value to a tag on the specified target server.
    pub async fn write_tag_value(
        &self,
        server: &str,
        tag_id: &str,
        value: OpcValue,
    ) -> OpcResult<WriteResult> {
        TagWriter::write_tag_value(self, server, tag_id, value).await
    }

    /// Asynchronously writes multiple tag values in a batch to the specified target server.
    pub async fn write_tag_values(
        &self,
        server: &str,
        writes: &[(String, OpcValue)],
    ) -> OpcResult<Vec<WriteResult>> {
        TagWriter::write_tag_values(self, server, writes).await
    }
}

#[async_trait]
impl<C: ServerConnector + 'static, State: Send + Sync + 'static> ServerDiscovery
    for OpcDaClient<C, State>
{
    #[tracing::instrument(level = "info", skip(self), err)]
    async fn list_servers(&self, host: &str) -> OpcResult<Vec<String>> {
        let host_owned = host.to_string();
        self.dispatch_request(|reply| ComRequest::ListServers {
            host: host_owned,
            reply,
        })
        .await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    async fn list_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        let host_owned = host.to_string();
        self.dispatch_request(|reply| ComRequest::ListServerDetails {
            host: host_owned,
            reply,
        })
        .await
    }
}

#[async_trait]
impl<C: ServerConnector + 'static, State: Send + Sync + 'static> TagBrowser
    for OpcDaClient<C, State>
{
    #[tracing::instrument(level = "info", skip(self, collector), err)]
    async fn browse_tags(&self, server: &str, collector: TagCollector) -> OpcResult<Vec<String>> {
        let endpoint = crate::types::OpcServerEndpoint::from(server);
        self.dispatch_request(|reply| ComRequest::BrowseTags {
            endpoint,
            collector,
            reply,
        })
        .await
    }
}

#[async_trait]
impl<C: ServerConnector + 'static, State: Send + Sync + 'static> TagReader
    for OpcDaClient<C, State>
{
    #[tracing::instrument(level = "info", skip(self, tags), fields(tag_count = tags.len()), err)]
    async fn read_tag_values(&self, server: &str, tags: TagBatch) -> OpcResult<TagValues> {
        let endpoint = crate::types::OpcServerEndpoint::from(server);
        self.dispatch_request(|reply| ComRequest::ReadTagValues {
            endpoint,
            tags,
            reply,
        })
        .await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    async fn read_tag_value(&self, server: &str, tag_id: &str) -> OpcResult<TagValue> {
        let endpoint = crate::types::OpcServerEndpoint::from(server);
        let tags = TagBatch::from_str_lenient(tag_id);
        let values = self
            .dispatch_request(|reply| ComRequest::ReadTagValues {
                endpoint,
                tags,
                reply,
            })
            .await?;
        values.into_vec().pop().ok_or_else(|| {
            OpcError::Internal(format!("Tag '{tag_id}' returned no response from server"))
        })
    }
}

#[async_trait]
impl<C: ServerConnector + 'static, State: Send + Sync + 'static> TagWriter
    for OpcDaClient<C, State>
{
    #[tracing::instrument(level = "info", skip(self, value), err)]
    async fn write_tag_value(
        &self,
        server: &str,
        tag_id: &str,
        value: OpcValue,
    ) -> OpcResult<WriteResult> {
        let endpoint = crate::types::OpcServerEndpoint::from(server);
        let tag_id_owned = tag_id.to_string();
        self.dispatch_request(|reply| ComRequest::WriteTagValue {
            endpoint,
            tag_id: tag_id_owned,
            value,
            reply,
        })
        .await
    }

    #[tracing::instrument(level = "info", skip(self, writes), fields(write_count = writes.len()), err)]
    async fn write_tag_values(
        &self,
        server: &str,
        writes: &[(String, OpcValue)],
    ) -> OpcResult<Vec<WriteResult>> {
        let endpoint = crate::types::OpcServerEndpoint::from(server);
        let writes_vec = writes.to_vec();
        self.dispatch_request(|reply| ComRequest::WriteTagValues {
            endpoint,
            writes: writes_vec,
            reply,
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::com::connector::MockServerConnector;
    use crate::provider::OpcProvider;
    use crate::types::{ClientItemHandle, OpcQuality, ServerItemHandle};

    #[tokio::test]
    async fn test_client_list_server_details() {
        let connector = MockServerConnector::new().with_server_details(vec![OpcServerInfo {
            prog_id: "Test.Server.1".into(),
            clsid: windows::core::GUID::zeroed(),
            user_type: Some("Test OPC Server".into()),
            host: None,
        }]);
        let client = OpcDaClient::new(connector).unwrap();
        let details = client.list_server_details("localhost").await.unwrap();
        assert_eq!(details.len(), 1);
        assert_eq!(details[0].prog_id, "Test.Server.1");
        assert_eq!(details[0].display_name(), "Test OPC Server");
    }

    #[tokio::test]
    async fn test_client_builder_configuration_and_unbound_discovery() {
        let state = std::sync::Arc::new(crate::com::connector::mock::MockState::default());
        let connector = MockServerConnector::with_state(state.clone());

        // 1. Bound client
        let client = OpcDaClient::builder()
            .host("192.168.1.50")
            .server("Matrikon.OPC.Simulation.1")
            .with_legacy_dcom(true)
            .with_connector(connector.clone())
            .build()
            .expect("building client should succeed");

        assert_eq!(
            client.endpoint.as_ref().unwrap().host.as_deref(),
            Some("192.168.1.50")
        );
        assert_eq!(
            client.endpoint.as_ref().unwrap().identifier,
            ServerIdentifier::from("Matrikon.OPC.Simulation.1")
        );

        // 2. Unbound client discovery
        let unbound_client = OpcDaClient::builder()
            .with_connector(connector)
            .build()
            .expect("building unbound client should succeed");
        assert!(unbound_client.endpoint.is_none());

        let servers = unbound_client.list_servers("192.168.1.50").await.unwrap();
        assert_eq!(servers, vec!["Mock.Server.1".to_string()]);

        #[allow(deprecated)]
        let legacy_servers = unbound_client
            .list_servers_on("192.168.1.50")
            .await
            .unwrap();
        assert_eq!(legacy_servers, vec!["Mock.Server.1".to_string()]);
    }

    #[tokio::test]
    async fn test_builder_timeout_and_legacy_dcom() {
        let builder = OpcDaClientBuilder::new()
            .timeout(std::time::Duration::from_millis(50))
            .with_legacy_dcom(true);
        assert_eq!(builder.timeout, Some(std::time::Duration::from_millis(50)));
        assert!(builder.legacy_dcom);

        let connector = MockServerConnector::new().with_read_fn(|_, _| {
            std::thread::sleep(std::time::Duration::from_millis(150));
            Ok(vec![])
        });

        let client = OpcDaClient::builder()
            .server("Mock.Server.1")
            .timeout(std::time::Duration::from_millis(50))
            .with_connector(connector)
            .build()
            .unwrap();

        assert_eq!(client.timeout, Some(std::time::Duration::from_millis(50)));
        let err = client.read_tag_values(["Tag1"]).await.unwrap_err();
        assert!(matches!(err, OpcError::Timeout(_)));
        assert!(err.is_connection_error());
    }

    #[tokio::test]
    async fn test_inherent_async_reads_and_writes_on_client() {
        let state = std::sync::Arc::new(crate::com::connector::mock::MockState::default());
        let group = crate::com::connector::mock::MockConnectedGroup {
            state: state.clone(),
            ..Default::default()
        };
        let item_registry =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
                ServerItemHandle,
                String,
            >::new()));
        let reg_add = item_registry.clone();
        let add_fn = Box::new(
            move |items: &[crate::com::connector::traits::GroupItemDef]| {
                let mut reg = reg_add.lock().unwrap();
                Ok(items
                    .iter()
                    .enumerate()
                    .map(|(i, it)| {
                        #[allow(clippy::cast_possible_truncation)]
                        let h = ServerItemHandle::new((i + 1) as u32);
                        reg.insert(h, it.item_id.clone());
                        crate::com::connector::traits::GroupItemResult {
                            server_handle: h,
                            canonical_type: windows::Win32::System::Variant::VT_BSTR.0,
                            error: None,
                        }
                    })
                    .collect())
            },
        );
        let reg_read = item_registry.clone();
        let read_fn = Box::new(move |_source, handles: &[ServerItemHandle]| {
            let reg = reg_read.lock().unwrap();
            Ok(handles
                .iter()
                .map(|h| {
                    let tag = reg.get(h).map_or("", String::as_str);
                    let val = match tag {
                        "Random.Real8" => OpcValue::Float(12.345),
                        "Random.String" => OpcValue::String("mock_string".into()),
                        _ => OpcValue::Int(42),
                    };
                    Ok(crate::com::connector::traits::GroupItemState {
                        client_handle: ClientItemHandle::new(h.as_raw()),
                        value: val,
                        quality: OpcQuality::GOOD,
                        timestamp: std::time::SystemTime::UNIX_EPOCH,
                    })
                })
                .collect())
        });
        let group = group.with_add_items_fn(add_fn).with_read_fn(read_fn);
        let server = std::sync::Arc::new(crate::com::connector::mock::MockConnectedServer {
            group: std::sync::Arc::new(group),
            state: state.clone(),
            ..Default::default()
        });
        let connector = MockServerConnector {
            server,
            state: state.clone(),
            servers: std::sync::Arc::new(std::sync::Mutex::new(vec!["Mock.Server.1".into()])),
            server_details: std::sync::Arc::new(std::sync::Mutex::new(vec![])),
        };

        let client = OpcDaClient::builder()
            .server("Matrikon.OPC.Simulation.1")
            .with_connector(connector)
            .build()
            .expect("client build");

        // Test inherent read_tag_values with static array
        let values = client
            .read_tag_values(["Random.Int4", "Random.Real8"])
            .await
            .expect("batch read");
        assert_eq!(values.len(), 2);

        // Test typed inherent readers
        let i_val = client.read_i32("Random.Int4").await.expect("read_i32");
        assert_eq!(i_val, 42);

        let f_val = client.read_f64("Random.Real8").await.expect("read_f64");
        assert!((f_val - 12.345).abs() < 1e-4);

        let s_val = client
            .read_string("Random.String")
            .await
            .expect("read_string");
        assert_eq!(s_val, "mock_string");

        // Test inherent write
        let write_res = client.write("Random.Int4", 100).await.expect("write");
        assert!(write_res.is_success());

        // Test inherent write_batch
        let batch_res = client
            .write_batch(vec![("Random.Int4".to_string(), OpcValue::Int(200))])
            .await
            .expect("write_batch");
        assert_eq!(batch_res.len(), 1);
        assert!(batch_res[0].is_success());
    }

    #[tokio::test]
    async fn test_remote_host_propagation_and_discovery() {
        let state = std::sync::Arc::new(crate::com::connector::mock::MockState::default());
        let connector = MockServerConnector::with_state(state.clone());

        let client = OpcDaClient::builder()
            .with_connector(connector)
            .build()
            .expect("unbound client");

        let servers = client.list_servers("192.168.1.100").await.unwrap();
        assert_eq!(servers, vec!["Mock.Server.1".to_string()]);
        assert_eq!(
            state.last_enumerated_host.lock().unwrap().as_deref(),
            Some("192.168.1.100")
        );
    }

    #[tokio::test]
    async fn test_client_subscribe_mpsc_polling_stream() {
        let state = std::sync::Arc::new(crate::com::connector::mock::MockState::default());
        let connector = MockServerConnector::with_state(state.clone());

        let client = OpcDaClient::builder()
            .server("Matrikon.OPC.Simulation.1")
            .with_connector(connector)
            .build()
            .expect("client build");

        let mut rx = client.subscribe(["Random.Int4"], std::time::Duration::from_millis(20));

        let tick1 = rx.recv().await.expect("tick 1");
        assert_eq!(tick1.len(), 1);
        assert_eq!(tick1.get("Random.Int4").unwrap().tag_id, "Random.Int4");

        let tick2 = rx.recv().await.expect("tick 2");
        assert_eq!(tick2.len(), 1);
    }

    #[tokio::test]
    async fn test_subscribe_receiver_drop_cancellation() {
        let state = std::sync::Arc::new(crate::com::connector::mock::MockState::default());
        let connector = MockServerConnector::with_state(state.clone());

        let client = OpcDaClient::builder()
            .server("Matrikon.OPC.Simulation.1")
            .with_connector(connector)
            .build()
            .expect("client build");

        let rx = client.subscribe(["Random.Int4"], std::time::Duration::from_millis(10));
        drop(rx);

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn test_client_bind_and_connect_eager() {
        let state = std::sync::Arc::new(crate::com::connector::mock::MockState::default());
        let connector = MockServerConnector::with_state(state.clone());

        let client = OpcDaClient::new(connector).unwrap();

        // Calling connect_eager on unbound client fails with InvalidState
        assert!(matches!(
            client.connect_eager().await,
            Err(OpcError::InvalidState(_))
        ));

        // Binding to a server endpoint succeeds locally
        let bound = OpcDaClient::builder()
            .server("Matrikon.OPC.Simulation.1")
            .with_connector(MockServerConnector::with_state(state.clone()))
            .build()
            .unwrap();

        assert!(bound.connect_eager().await.is_ok());
    }

    #[tokio::test]
    async fn test_client_role_traits_via_opc_provider() {
        let state = std::sync::Arc::new(crate::com::connector::mock::MockState::default());
        let connector = MockServerConnector::with_state(state.clone());

        let client: Arc<dyn OpcProvider> = Arc::new(
            OpcDaClient::builder()
                .with_connector(connector)
                .build()
                .unwrap(),
        );

        // ServerDiscovery
        let servers = client.list_servers("localhost").await.unwrap();
        assert_eq!(servers, vec!["Mock.Server.1".to_string()]);

        // TagBrowser
        let tags = client
            .browse_tags("Mock.Server.1", TagCollector::new(1000))
            .await
            .unwrap();
        assert_eq!(
            tags,
            vec![
                "Random.Int4".to_string(),
                "Random.Real8".to_string(),
                "Random.String".to_string()
            ]
        );

        // TagReader
        let values = client
            .read_tag_values("Mock.Server.1", TagBatch::from(vec!["Tag1".into()]))
            .await
            .unwrap();
        assert_eq!(values.len(), 1);

        // TagWriter
        let res = client
            .write_tag_value("Mock.Server.1", "Tag1", OpcValue::Int(10))
            .await
            .unwrap();
        assert!(res.is_success());
    }
}
