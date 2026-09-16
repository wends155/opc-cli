//! Inherent multi-server gateway operations on [`OpcDaClient<C, Unbound>`] and role trait implementations.

use crate::client::OpcDaClient;
use crate::client::typestate::Unbound;
use crate::com::worker::ComRequest;
use crate::connector::ServerBackend;
use crate::errors::{OpcError, OpcResult};
use crate::provider::{ServerDiscovery, TagBrowser, TagReader, TagWriter};
use crate::types::{
    IntoTags, IntoWriteBatch, OpcServerEndpoint, OpcServerInfo, OpcValue, TagBatch, TagCollector,
    TagValue, TagValues, WriteBatch, WriteResult,
};

impl<C: ServerBackend + 'static> OpcDaClient<C, Unbound> {
    /// Lists ProgIDs of OPC DA servers registered on the specified host.
    ///
    /// Connects to the host's OLE/COM Component Categories Manager or OPC Server List
    /// interface to enumerate all available Data Access servers.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if the host cannot be reached or the COM catalog query fails.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C>) -> OpcResult<()> {
    /// let servers = client.list_servers("localhost").await?;
    /// let _ = servers;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn list_servers(&self, host: &str) -> OpcResult<Vec<String>> {
        ServerDiscovery::list_servers(self, host).await
    }

    /// Lists detailed registration information of OPC DA servers registered on the specified host.
    ///
    /// Returns a list of [`OpcServerInfo`] structures containing ProgID, user-friendly type name,
    /// and registered CLSID.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if the host cannot be reached or server detail resolution fails.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C>) -> OpcResult<()> {
    /// let details = client.list_server_details("localhost").await?;
    /// let _ = details;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn list_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        ServerDiscovery::list_server_details(self, host).await
    }

    /// Recursively or flatly browses available tags from the specified server.
    ///
    /// Uses the provided [`TagCollector`] to accumulate discovered tag identifiers,
    /// respecting max item limits or branch filters configured on the collector.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if connection to the server fails or namespace browsing fails.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult, types::TagCollector};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C>) -> OpcResult<()> {
    /// let tags = client.browse_tags("Matrikon.OPC.Simulation.1", TagCollector::new(50)).await?;
    /// assert!(!tags.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self, collector), err)]
    pub async fn browse_tags(
        &self,
        server: &str,
        collector: TagCollector,
    ) -> OpcResult<Vec<String>> {
        TagBrowser::browse_tags(self, server, collector).await
    }

    /// Writes a single value to the specified tag on the target server.
    ///
    /// Accepts any value convertible into an [`OpcValue`], including primitive numbers,
    /// booleans, and string references.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if connection to the server fails or the write is rejected.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C>) -> OpcResult<()> {
    /// let res = client.write_tag_value("Matrikon.OPC.Simulation.1", "Sensor.Temp", 42.5f64).await?;
    /// assert!(res.is_success());
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self, value), err)]
    pub async fn write_tag_value(
        &self,
        server: &str,
        tag_id: &str,
        value: impl Into<OpcValue>,
    ) -> OpcResult<WriteResult> {
        TagWriter::write_tag_value(self, server, tag_id, value.into()).await
    }

    /// Writes a batch of tag-value pairs to the target server.
    ///
    /// Accepts any type convertible into a [`WriteBatch`] via [`IntoWriteBatch`], including
    /// arrays of tuples `[("Tag", value)]`, slices, vectors, or pre-constructed [`WriteBatch`].
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if the worker dispatch or server connection fails. Individual
    /// tag write failures are recorded in the per-item [`WriteResult`] return vector.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C>) -> OpcResult<()> {
    /// let results = client.write_tag_batch("Matrikon.OPC.Simulation.1", [("Tag.1", 1.0f64), ("Tag.2", 2.0f64)]).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self, writes), err)]
    pub async fn write_tag_batch(
        &self,
        server: &str,
        writes: impl IntoWriteBatch,
    ) -> OpcResult<Vec<WriteResult>> {
        TagWriter::write_tag_batch(self, server, writes.into_write_batch()).await
    }

    /// Reads multiple tags in a batch from the specified server.
    ///
    /// Accepts any type convertible into tags via [`IntoTags`], including array references,
    /// slices, vectors, or pre-constructed [`TagBatch`] instances.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if the server connection or worker dispatch fails entirely.
    /// Individual tag failures are preserved inside the returned [`TagValues`].
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C>) -> OpcResult<()> {
    /// let values = client.read_tag_values("Matrikon.OPC.Simulation.1", ["Tag.1", "Tag.2"]).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self, tags), err)]
    pub async fn read_tag_values(&self, server: &str, tags: impl IntoTags) -> OpcResult<TagValues> {
        TagReader::read_tag_values(self, server, tags.into_tag_batch()).await
    }

    /// Reads a single tag from the specified server and returns its complete metadata and value representation.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication fails or the tag returned no response from the server.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C>) -> OpcResult<()> {
    /// let tv = client.read_tag_value("Matrikon.OPC.Simulation.1", "Random.Int4").await?;
    /// assert!(tv.quality.is_good());
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_tag_value(&self, server: &str, tag_id: &str) -> OpcResult<TagValue> {
        TagReader::read_tag_value(self, server, tag_id).await
    }

    /// Shorthand alias for [`OpcDaClient::read_tag_values`].
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication fails. See [`OpcDaClient::read_tag_values`].
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C>) -> OpcResult<()> {
    /// let values = client.read_tags("Matrikon.OPC.Simulation.1", ["Tag.1", "Tag.2"]).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub async fn read_tags(&self, server: &str, tags: impl IntoTags) -> OpcResult<TagValues> {
        self.read_tag_values(server, tags).await
    }

    /// Shorthand alias for [`OpcDaClient::read_tag_value`].
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication fails. See [`OpcDaClient::read_tag_value`].
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C>) -> OpcResult<()> {
    /// let tv = client.read_tag("Matrikon.OPC.Simulation.1", "Random.Int4").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub async fn read_tag(&self, server: &str, tag_id: &str) -> OpcResult<TagValue> {
        self.read_tag_value(server, tag_id).await
    }

    /// Shorthand alias for [`OpcDaClient::write_tag_batch`].
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication fails. See [`OpcDaClient::write_tag_batch`].
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C>) -> OpcResult<()> {
    /// let results = client.write_tags("Matrikon.OPC.Simulation.1", [("Tag.A", 10i32)]).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub async fn write_tags(
        &self,
        server: &str,
        writes: impl IntoWriteBatch,
    ) -> OpcResult<Vec<WriteResult>> {
        self.write_tag_batch(server, writes).await
    }

    /// Shorthand alias for [`OpcDaClient::write_tag_value`].
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication fails. See [`OpcDaClient::write_tag_value`].
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C>) -> OpcResult<()> {
    /// let res = client.write_tag("Matrikon.OPC.Simulation.1", "Tag.A", 100i32).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub async fn write_tag(
        &self,
        server: &str,
        tag_id: &str,
        value: impl Into<OpcValue>,
    ) -> OpcResult<WriteResult> {
        self.write_tag_value(server, tag_id, value).await
    }

    /// Shorthand alias for [`OpcDaClient::browse_tags`].
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication fails. See [`OpcDaClient::browse_tags`].
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult, types::TagCollector};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C>) -> OpcResult<()> {
    /// let tags = client.browse("Matrikon.OPC.Simulation.1", TagCollector::new(50)).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub async fn browse(&self, server: &str, collector: TagCollector) -> OpcResult<Vec<String>> {
        self.browse_tags(server, collector).await
    }
}

impl<C: ServerBackend + 'static, State: Send + Sync + 'static> ServerDiscovery
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

impl<C: ServerBackend + 'static, State: Send + Sync + 'static> OpcDaClient<C, State> {
    pub(crate) fn validate_bound_server(&self, server: &str) -> OpcResult<OpcServerEndpoint> {
        let requested_ep: OpcServerEndpoint = server.parse()?;
        if let Some(bound_ep) = &self.endpoint {
            let has_explicit_host = server.contains('\\') || server.contains('/');
            if requested_ep.identifier() != bound_ep.identifier()
                || (has_explicit_host && requested_ep.host() != bound_ep.host())
            {
                return Err(OpcError::InvalidState(format!(
                    "Client is bound to server '{bound_ep}', but request targeted '{server}'"
                )));
            }
            Ok(bound_ep.clone())
        } else {
            Ok(requested_ep)
        }
    }
}

impl<C: ServerBackend + 'static, State: Send + Sync + 'static> TagBrowser
    for OpcDaClient<C, State>
{
    #[tracing::instrument(level = "info", skip(self, collector), err)]
    async fn browse_tags(&self, server: &str, collector: TagCollector) -> OpcResult<Vec<String>> {
        let endpoint = self.validate_bound_server(server)?;
        self.dispatch_request(|reply| ComRequest::BrowseTags {
            endpoint,
            collector,
            reply,
        })
        .await
    }
}

impl<C: ServerBackend + 'static, State: Send + Sync + 'static> TagReader for OpcDaClient<C, State> {
    #[tracing::instrument(level = "info", skip(self, tags), fields(tag_count = tags.len()), err)]
    async fn read_tag_values(&self, server: &str, tags: TagBatch) -> OpcResult<TagValues> {
        let endpoint = self.validate_bound_server(server)?;
        self.dispatch_request(|reply| ComRequest::ReadTagValues {
            endpoint,
            tags,
            reply,
        })
        .await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    async fn read_tag_value(&self, server: &str, tag_id: &str) -> OpcResult<TagValue> {
        let endpoint = self.validate_bound_server(server)?;
        let tags = TagBatch::from_str_lenient(tag_id);
        let values = self
            .dispatch_request(|reply| ComRequest::ReadTagValues {
                endpoint,
                tags,
                reply,
            })
            .await?;
        values.into_iter().next().ok_or_else(|| {
            OpcError::Internal(format!("Tag '{tag_id}' returned no response from server"))
        })
    }
}

impl<C: ServerBackend + 'static, State: Send + Sync + 'static> TagWriter for OpcDaClient<C, State> {
    #[tracing::instrument(level = "info", skip(self, value), err)]
    async fn write_tag_value(
        &self,
        server: &str,
        tag_id: &str,
        value: OpcValue,
    ) -> OpcResult<WriteResult> {
        let endpoint = self.validate_bound_server(server)?;
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
    async fn write_tag_batch(
        &self,
        server: &str,
        writes: WriteBatch,
    ) -> OpcResult<Vec<WriteResult>> {
        let endpoint = self.validate_bound_server(server)?;
        self.dispatch_request(|reply| ComRequest::WriteTagValues {
            endpoint,
            writes,
            reply,
        })
        .await
    }
}
