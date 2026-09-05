use crate::com::connector::{ComConnector, ServerConnector};
use crate::com::worker::{ComRequest, ComWorker};
use crate::errors::OpcResult;
use crate::provider::{OpcProvider, OpcValue, TagCollector, TagValue, WriteResult};
use crate::types::{OpcServerInfo, ServerIdentifier};
use async_trait::async_trait;
use std::sync::Arc;

/// Concrete [`OpcProvider`] implementation for Windows OPC DA.
///
/// Uses native `windows-rs` COM interop via the internal `opc_da` module.
pub struct OpcDaClient<C: ServerConnector + 'static = ComConnector> {
    pub worker: ComWorker<C>,
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
                    worker: ComWorker::closed(),
                }
            }
        }
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
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use opc_da_client::{ComConnector, OpcDaClient, OpcResult};
    ///
    /// fn init_client() -> OpcResult<OpcDaClient> {
    ///     OpcDaClient::new(ComConnector)
    /// }
    /// ```
    #[tracing::instrument(level = "info", skip(connector), err)]
    pub fn new(connector: C) -> OpcResult<Self> {
        tracing::info!("Initializing OpcDaClient...");
        let worker = ComWorker::start(Arc::new(connector))?;
        tracing::info!("OpcDaClient initialized successfully");
        Ok(Self { worker })
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
        let server_id = ServerIdentifier::from(server);
        self.worker
            .send_request(|reply| ComRequest::BrowseTags {
                server: server_id,
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
        let server_id = ServerIdentifier::from(server);
        self.worker
            .send_request(|reply| ComRequest::ReadTagValues {
                server: server_id,
                tag_ids,
                reply,
            })
            .await
    }

    #[tracing::instrument(level = "info", skip(self, value), err)]
    async fn write_tag_value(
        &self,
        server: &str,
        tag_id: &str,
        value: OpcValue,
    ) -> OpcResult<WriteResult> {
        let server_id = ServerIdentifier::from(server);
        let tag_id_owned = tag_id.to_string();
        self.worker
            .send_request(|reply| ComRequest::WriteTagValue {
                server: server_id,
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
}
