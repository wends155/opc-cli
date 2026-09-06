use crate::com::connector::{ComConnector, ServerConnector};
use crate::com::worker::{ComRequest, ComWorker};
use crate::errors::{OpcError, OpcResult};
use crate::provider::{OpcProvider, OpcValue, TagCollector, TagValue, WriteResult};
use crate::types::{IntoTags, OpcServerEndpoint, OpcServerInfo, ServerIdentifier, TagValues};
use async_trait::async_trait;
use std::sync::Arc;

/// Fluent builder for configuring and constructing an [`OpcDaClient`].
#[derive(Debug, Clone)]
pub struct OpcDaClientBuilder<C = ComConnector> {
    host: Option<String>,
    server: Option<ServerIdentifier>,
    timeout: Option<std::time::Duration>,
    legacy_dcom: bool,
    connector: Option<C>,
}

impl Default for OpcDaClientBuilder<ComConnector> {
    fn default() -> Self {
        Self::new()
    }
}

impl OpcDaClientBuilder<ComConnector> {
    /// Creates a new default client builder targeting the standard Windows [`ComConnector`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            host: None,
            server: None,
            timeout: None,
            legacy_dcom: false,
            connector: None,
        }
    }
}

impl<C: ServerConnector + 'static> OpcDaClientBuilder<C> {
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
    pub fn timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Configures legacy DCOM security blanketing.
    #[must_use]
    pub fn with_legacy_dcom(mut self, legacy: bool) -> Self {
        self.legacy_dcom = legacy;
        self
    }

    /// Transitions the builder to use an alternative backend connector (e.g., a test mock).
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
    pub fn build_with_connector(self, connector: C) -> OpcResult<OpcDaClient<C>> {
        let mut client = OpcDaClient::new(connector)?;
        if let Some(server) = self.server {
            client.endpoint = Some(OpcServerEndpoint {
                host: self.host,
                identifier: server,
            });
        }
        Ok(client)
    }
}

impl<C: ServerConnector + Default + 'static> OpcDaClientBuilder<C> {
    /// Builds the `OpcDaClient` using the configured options and default connector.
    pub fn build(self) -> OpcResult<OpcDaClient<C>> {
        let connector = self.connector.unwrap_or_default();
        let mut client = OpcDaClient::new(connector)?;
        if let Some(server) = self.server {
            client.endpoint = Some(OpcServerEndpoint {
                host: self.host,
                identifier: server,
            });
        }
        Ok(client)
    }
}

/// Concrete [`OpcProvider`] implementation for Windows OPC DA.
///
/// Uses native `windows-rs` COM interop via the internal `com` subsystem.
pub struct OpcDaClient<C: ServerConnector + 'static = ComConnector> {
    /// Background MTA worker handle managing asynchronous request channels.
    pub worker: Arc<ComWorker<C>>,
    /// Target OPC server endpoint if bound to a specific server.
    pub endpoint: Option<OpcServerEndpoint>,
}

impl<C: ServerConnector + 'static> Clone for OpcDaClient<C> {
    fn clone(&self) -> Self {
        Self {
            worker: Arc::clone(&self.worker),
            endpoint: self.endpoint.clone(),
        }
    }
}

/// Returns the default `OpcDaClient` using native COM settings.
///
/// If the background COM worker thread cannot be started or COM
/// Multi-Threaded Apartment (MTA) initialization fails on the worker thread,
/// this logs an error and returns a closed client whose operations will fail
/// cleanly with [`crate::errors::OpcError::Connection`].
///
/// Use [`OpcDaClient::new`] for explicit fallible construction.
impl Default for OpcDaClient<ComConnector> {
    fn default() -> Self {
        match Self::new(ComConnector) {
            Ok(client) => client,
            Err(err) => {
                tracing::error!(error = ?err, "Failed to initialize default OpcDaClient");
                Self {
                    worker: Arc::new(ComWorker::closed()),
                    endpoint: None,
                }
            }
        }
    }
}

impl OpcDaClient<ComConnector> {
    /// Returns a new fluent builder for configuring and connecting an `OpcDaClient`.
    #[must_use]
    pub fn builder() -> OpcDaClientBuilder<ComConnector> {
        OpcDaClientBuilder::new()
    }

    /// Quickly connects to a local OPC DA server by ProgID or CLSID.
    pub fn connect(server: impl Into<ServerIdentifier>) -> OpcResult<Self> {
        Self::builder().server(server).build()
    }

    /// Quickly connects to a remote OPC DA server by host and ProgID or CLSID.
    pub fn connect_remote(
        host: impl Into<String>,
        server: impl Into<ServerIdentifier>,
    ) -> OpcResult<Self> {
        Self::builder().host(host).server(server).build()
    }
}

impl<C: ServerConnector + 'static> OpcDaClient<C> {
    /// Creates a new `OpcDaClient` with the given connector.
    ///
    /// # Arguments
    /// * `connector` - Backend connector implementing [`ServerConnector`].
    ///
    /// # Errors
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
        })
    }

    /// Binds or overrides the target remote host on this client.
    #[must_use]
    pub fn host(mut self, host: impl Into<String>) -> Self {
        let h = host.into();
        let endpoint = self.endpoint.get_or_insert_with(|| OpcServerEndpoint {
            host: None,
            identifier: ServerIdentifier::ProgId(String::new()),
        });
        endpoint.host = Some(h);
        self
    }

    /// Binds or overrides the target server identifier on this client.
    #[must_use]
    pub fn server(mut self, server: impl Into<ServerIdentifier>) -> Self {
        let s = server.into();
        let endpoint = self.endpoint.get_or_insert_with(|| OpcServerEndpoint {
            host: None,
            identifier: ServerIdentifier::ProgId(String::new()),
        });
        endpoint.identifier = s;
        self
    }

    /// Asynchronously reads current values, quality, and timestamps for a batch of tags.
    #[tracing::instrument(level = "info", skip(self, tags), err)]
    pub async fn read_tag_values(&self, tags: impl IntoTags) -> OpcResult<TagValues> {
        let endpoint = self.endpoint.as_ref().ok_or_else(|| {
            OpcError::InvalidState(
                "Client is not bound to a server. Use OpcDaClient::builder().server(...) or OpcProvider::read_tag_values"
                    .into(),
            )
        })?.clone();
        let batch = tags.into_tag_batch();
        self.worker
            .send_request(|reply| ComRequest::ReadTagValues {
                endpoint,
                tags: batch,
                reply,
            })
            .await
    }

    /// Reads a single tag and unwraps its value as an `f64`.
    pub async fn read_f64(&self, tag: &str) -> OpcResult<f64> {
        let values = self.read_tag_values(tag.to_string()).await?;
        values.get_f64(tag).map_err(Into::into)
    }

    /// Reads a single tag and unwraps its value as an `i32`.
    pub async fn read_i32(&self, tag: &str) -> OpcResult<i32> {
        let values = self.read_tag_values(tag.to_string()).await?;
        values.get_i32(tag).map_err(Into::into)
    }

    /// Reads a single tag and unwraps its value as a `bool`.
    pub async fn read_bool(&self, tag: &str) -> OpcResult<bool> {
        let values = self.read_tag_values(tag.to_string()).await?;
        values.get_bool(tag).map_err(Into::into)
    }

    /// Reads a single tag and unwraps its value as a `String`.
    pub async fn read_string(&self, tag: &str) -> OpcResult<String> {
        let values = self.read_tag_values(tag.to_string()).await?;
        values
            .get_str(tag)
            .map(ToString::to_string)
            .map_err(Into::into)
    }

    /// Asynchronously writes a typed value to a tag.
    #[tracing::instrument(level = "info", skip(self, value), err)]
    pub async fn write(&self, tag: &str, value: impl Into<OpcValue>) -> OpcResult<WriteResult> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or_else(|| OpcError::InvalidState("Client is not bound to a server".into()))?
            .clone();
        self.worker
            .send_request(|reply| ComRequest::WriteTagValue {
                endpoint,
                tag_id: tag.to_string(),
                value: value.into(),
                reply,
            })
            .await
    }

    /// Asynchronously writes a batch of tag-value pairs in a single operation.
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
        self.worker
            .send_request(|reply| ComRequest::WriteTagValues {
                endpoint,
                writes,
                reply,
            })
            .await
    }

    /// Lists available OPC servers on a remote (or local) host.
    pub async fn list_servers_on(&self, host: &str) -> OpcResult<Vec<String>> {
        self.list_servers(host).await
    }

    /// Subscribes to a stream of periodic tag value reads, returning an asynchronous [`tokio::sync::mpsc::Receiver`].
    ///
    /// Dropping the returned receiver automatically cancels the background polling task.
    pub fn subscribe(
        &self,
        tags: impl IntoTags,
        interval: std::time::Duration,
    ) -> tokio::sync::mpsc::Receiver<TagValues> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let tags_batch = tags.into_tag_batch();
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
}

#[allow(clippy::too_many_lines)]
#[async_trait]
impl<C: ServerConnector + 'static> OpcProvider for OpcDaClient<C> {
    #[tracing::instrument(level = "info", skip(self), err)]
    async fn list_servers(&self, host: &str) -> OpcResult<Vec<String>> {
        let host_owned = host.to_string();
        self.worker
            .send_request(|reply| ComRequest::ListServers {
                host: host_owned,
                reply,
            })
            .await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    async fn list_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        let host_owned = host.to_string();
        self.worker
            .send_request(|reply| ComRequest::ListServerDetails {
                host: host_owned,
                reply,
            })
            .await
    }

    #[tracing::instrument(level = "info", skip(self, collector), err)]
    async fn browse_tags(&self, server: &str, collector: TagCollector) -> OpcResult<Vec<String>> {
        let endpoint = crate::types::OpcServerEndpoint::from(server);
        self.worker
            .send_request(|reply| ComRequest::BrowseTags {
                endpoint,
                collector,
                reply,
            })
            .await
    }

    #[tracing::instrument(level = "info", skip(self, tag_ids), fields(tag_count = tag_ids.len()), err)]
    async fn read_tag_values(
        &self,
        server: &str,
        tag_ids: Vec<String>,
    ) -> OpcResult<Vec<TagValue>> {
        let endpoint = crate::types::OpcServerEndpoint::from(server);
        let tags = crate::types::TagBatch::from(tag_ids);
        let res = self
            .worker
            .send_request(|reply| ComRequest::ReadTagValues {
                endpoint,
                tags,
                reply,
            })
            .await?;
        Ok(res.into_vec())
    }

    #[tracing::instrument(level = "info", skip(self, value), err)]
    async fn write_tag_value(
        &self,
        server: &str,
        tag_id: &str,
        value: OpcValue,
    ) -> OpcResult<WriteResult> {
        let endpoint = crate::types::OpcServerEndpoint::from(server);
        let tag_id_owned = tag_id.to_string();
        self.worker
            .send_request(|reply| ComRequest::WriteTagValue {
                endpoint,
                tag_id: tag_id_owned,
                value,
                reply,
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::com::connector::MockServerConnector;
    use crate::types::{ItemHandle, OpcQuality};

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

        let servers = unbound_client
            .list_servers_on("192.168.1.50")
            .await
            .unwrap();
        assert_eq!(servers, vec!["Mock.Server.1".to_string()]);
    }

    #[tokio::test]
    async fn test_inherent_async_reads_and_writes_on_client() {
        let state = std::sync::Arc::new(crate::com::connector::mock::MockState::default());
        let mut group = crate::com::connector::mock::MockConnectedGroup {
            state: state.clone(),
            ..Default::default()
        };
        let item_registry =
            std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
                ItemHandle,
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
                        let h = ItemHandle::new((i + 1) as u32);
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
        let read_fn = Box::new(move |_source, handles: &[ItemHandle]| {
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
                        client_handle: *h,
                        value: val,
                        quality: OpcQuality::GOOD,
                        timestamp: std::time::SystemTime::UNIX_EPOCH,
                    })
                })
                .collect())
        });
        group.add_items_fn = Some(add_fn);
        group.read_fn = Some(read_fn);
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

        let servers = client.list_servers_on("192.168.1.100").await.unwrap();
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
}
