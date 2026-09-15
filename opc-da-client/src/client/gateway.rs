//! Inherent multi-server gateway operations on [`OpcDaClient<C, Unbound>`] and role trait implementations.

use crate::client::OpcDaClient;
use crate::client::typestate::Unbound;
use crate::com::worker::ComRequest;
use crate::connector::ServerBackend;
use crate::errors::{OpcError, OpcResult};
use crate::provider::{ServerDiscovery, TagBrowser, TagReader, TagWriter};
use crate::types::{
    IntoTags, IntoWriteBatch, OpcServerInfo, OpcValue, TagBatch, TagCollector, TagValue, TagValues,
    WriteBatch, WriteResult,
};

impl<C: ServerBackend + 'static> OpcDaClient<C, Unbound> {
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn list_servers(&self, host: &str) -> OpcResult<Vec<String>> {
        ServerDiscovery::list_servers(self, host).await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn list_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        ServerDiscovery::list_server_details(self, host).await
    }

    #[tracing::instrument(level = "info", skip(self, collector), err)]
    pub async fn browse_tags(
        &self,
        server: &str,
        collector: TagCollector,
    ) -> OpcResult<Vec<String>> {
        TagBrowser::browse_tags(self, server, collector).await
    }

    #[tracing::instrument(level = "info", skip(self, value), err)]
    pub async fn write_tag_value(
        &self,
        server: &str,
        tag_id: &str,
        value: OpcValue,
    ) -> OpcResult<WriteResult> {
        TagWriter::write_tag_value(self, server, tag_id, value).await
    }

    #[tracing::instrument(level = "info", skip(self, writes), fields(write_count = writes.len()), err)]
    pub async fn write_tag_batch(
        &self,
        server: &str,
        writes: WriteBatch,
    ) -> OpcResult<Vec<WriteResult>> {
        TagWriter::write_tag_batch(self, server, writes).await
    }

    #[tracing::instrument(level = "info", skip(self, tags), err)]
    pub async fn read_tag_values(&self, server: &str, tags: impl IntoTags) -> OpcResult<TagValues> {
        TagReader::read_tag_values(self, server, tags.into_tag_batch()).await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_tag_value(&self, server: &str, tag_id: &str) -> OpcResult<TagValue> {
        TagReader::read_tag_value(self, server, tag_id).await
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

impl<C: ServerBackend + 'static, State: Send + Sync + 'static> TagBrowser
    for OpcDaClient<C, State>
{
    #[tracing::instrument(level = "info", skip(self, collector), err)]
    async fn browse_tags(&self, server: &str, collector: TagCollector) -> OpcResult<Vec<String>> {
        let endpoint = server.parse()?;
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
        let endpoint = server.parse()?;
        self.dispatch_request(|reply| ComRequest::ReadTagValues {
            endpoint,
            tags,
            reply,
        })
        .await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    async fn read_tag_value(&self, server: &str, tag_id: &str) -> OpcResult<TagValue> {
        let endpoint = server.parse()?;
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
        let endpoint = server.parse()?;
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
        let endpoint = server.parse()?;
        self.dispatch_request(|reply| ComRequest::WriteTagValues {
            endpoint,
            writes,
            reply,
        })
        .await
    }

    #[allow(deprecated)]
    #[tracing::instrument(level = "info", skip(self, writes), fields(write_count = writes.len()), err)]
    async fn write_tag_values(
        &self,
        server: &str,
        writes: &[(String, OpcValue)],
    ) -> OpcResult<Vec<WriteResult>> {
        self.write_tag_batch(server, writes.into_write_batch())
            .await
    }
}
