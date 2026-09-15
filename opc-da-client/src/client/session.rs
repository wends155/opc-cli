//! Inherent single-server operations on [`OpcDaClient<C, Bound>`].

use crate::client::OpcDaClient;
use crate::client::typestate::Bound;
use crate::com::worker::ComRequest;
use crate::connector::ServerBackend;
use crate::errors::{OpcError, OpcResult};
use crate::types::{
    IntoTags, IntoWriteBatch, OpcValue, TagBatch, TagCollector, TagValue, TagValues, WriteResult,
};

impl<C: ServerBackend + 'static> OpcDaClient<C, Bound> {
    async fn read_single_typed<T, F>(&self, tag: &str, extract: F) -> OpcResult<T>
    where
        F: FnOnce(&TagValues, &str) -> Result<T, crate::types::TagExtractError>,
    {
        let batch = TagBatch::from_str_lenient(tag);
        let values = self.read_tags(batch).await?;
        extract(&values, tag).map_err(Into::into)
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_f64(&self, tag: &str) -> OpcResult<f64> {
        self.read_single_typed(tag, TagValues::get_f64).await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_i32(&self, tag: &str) -> OpcResult<i32> {
        self.read_single_typed(tag, TagValues::get_i32).await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_bool(&self, tag: &str) -> OpcResult<bool> {
        self.read_single_typed(tag, TagValues::get_bool).await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_string(&self, tag: &str) -> OpcResult<String> {
        self.read_single_typed(tag, |v, t| v.get_str(t).map(ToString::to_string))
            .await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_f32(&self, tag: &str) -> OpcResult<f32> {
        self.read_single_typed(tag, TagValues::get_f32).await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_i64(&self, tag: &str) -> OpcResult<i64> {
        self.read_single_typed(tag, TagValues::get_i64).await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_u32(&self, tag: &str) -> OpcResult<u32> {
        self.read_single_typed(tag, TagValues::get_u32).await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_u64(&self, tag: &str) -> OpcResult<u64> {
        self.read_single_typed(tag, TagValues::get_u64).await
    }

    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_tag(&self, tag: &str) -> OpcResult<TagValue> {
        let batch = TagBatch::from_str_lenient(tag);
        let values = self.read_tags(batch).await?;
        values.into_iter().next().ok_or_else(|| {
            OpcError::Internal(format!("Tag '{tag}' returned no response from server"))
        })
    }

    #[tracing::instrument(level = "info", skip(self, tags), err)]
    pub async fn read_tags(&self, tags: impl IntoTags) -> OpcResult<TagValues> {
        let endpoint = self.endpoint().clone();
        let batch = tags.into_tag_batch();
        self.dispatch_request(|reply| ComRequest::ReadTagValues {
            endpoint,
            tags: batch,
            reply,
        })
        .await
    }

    #[tracing::instrument(level = "info", skip(self, value), err)]
    pub async fn write_tag(&self, tag: &str, value: impl Into<OpcValue>) -> OpcResult<WriteResult> {
        let endpoint = self.endpoint().clone();
        self.dispatch_request(|reply| ComRequest::WriteTagValue {
            endpoint,
            tag_id: tag.to_string(),
            value: value.into(),
            reply,
        })
        .await
    }

    #[tracing::instrument(level = "info", skip(self, writes), err)]
    pub async fn write_tags(&self, writes: impl IntoWriteBatch) -> OpcResult<Vec<WriteResult>> {
        let endpoint = self.endpoint().clone();
        self.dispatch_request(|reply| ComRequest::WriteTagValues {
            endpoint,
            writes: writes.into_write_batch(),
            reply,
        })
        .await
    }

    #[tracing::instrument(level = "info", skip(self, collector), err)]
    pub async fn browse(&self, collector: TagCollector) -> OpcResult<Vec<String>> {
        let endpoint = self.endpoint().clone();
        self.dispatch_request(|reply| ComRequest::BrowseTags {
            endpoint,
            collector,
            reply,
        })
        .await
    }
}
