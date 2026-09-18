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

    /// Reads a 64-bit float (`f64`) value for a single tag from the bound server.
    ///
    /// Fetches the tag value and losslessly extracts or coerces it into an `f64`.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication with the server worker fails, the tag was
    /// not returned, or the returned value cannot be coerced to an `f64`.
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
    /// let temp = client.read_f64("Sensor.Temperature").await?;
    /// let _ = temp;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_f64(&self, tag: &str) -> OpcResult<f64> {
        self.read_single_typed(tag, TagValues::get_f64).await
    }

    /// Reads a 32-bit signed integer (`i32`) value for a single tag from the bound server.
    ///
    /// Fetches the tag value and losslessly extracts or coerces it into an `i32`.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication with the server worker fails, the tag was
    /// not returned, or the returned value cannot be coerced to an `i32`.
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
    /// let count = client.read_i32("Production.Counter").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_i32(&self, tag: &str) -> OpcResult<i32> {
        self.read_single_typed(tag, TagValues::get_i32).await
    }

    /// Reads a boolean (`bool`) value for a single tag from the bound server.
    ///
    /// Fetches the tag value and extracts it as a boolean.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication with the server worker fails, the tag was
    /// not returned, or the returned value cannot be coerced to a boolean.
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
    /// let active = client.read_bool("Motor.Running").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_bool(&self, tag: &str) -> OpcResult<bool> {
        self.read_single_typed(tag, TagValues::get_bool).await
    }

    /// Reads a string (`String`) value for a single tag from the bound server.
    ///
    /// Fetches the tag value and formats it into an owned string.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication with the server worker fails, the tag was
    /// not returned, or the tag read outcome contains an error.
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
    /// let status = client.read_string("System.Status").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_string(&self, tag: &str) -> OpcResult<String> {
        self.read_single_typed(tag, |v, t| v.get_str(t).map(ToString::to_string))
            .await
    }

    /// Reads a 32-bit float (`f32`) value for a single tag from the bound server.
    ///
    /// Fetches the tag value and losslessly extracts or coerces it into an `f32`.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication with the server worker fails, the tag was
    /// not returned, or the returned value cannot be coerced to an `f32`.
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
    /// let val = client.read_f32("Sensor.Pressure").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_f32(&self, tag: &str) -> OpcResult<f32> {
        self.read_single_typed(tag, TagValues::get_f32).await
    }

    /// Reads a 64-bit signed integer (`i64`) value for a single tag from the bound server.
    ///
    /// Fetches the tag value and losslessly extracts or coerces it into an `i64`.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication with the server worker fails, the tag was
    /// not returned, or the returned value cannot be coerced to an `i64`.
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
    /// let total = client.read_i64("Machine.TotalCycles").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_i64(&self, tag: &str) -> OpcResult<i64> {
        self.read_single_typed(tag, TagValues::get_i64).await
    }

    /// Reads a 32-bit unsigned integer (`u32`) value for a single tag from the bound server.
    ///
    /// Fetches the tag value and losslessly extracts or coerces it into a `u32`.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication with the server worker fails, the tag was
    /// not returned, or the returned value cannot be coerced to a `u32`.
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
    /// let count = client.read_u32("Events.Count").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_u32(&self, tag: &str) -> OpcResult<u32> {
        self.read_single_typed(tag, TagValues::get_u32).await
    }

    /// Reads a 64-bit unsigned integer (`u64`) value for a single tag from the bound server.
    ///
    /// Fetches the tag value and losslessly extracts or coerces it into a `u64`.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication with the server worker fails, the tag was
    /// not returned, or the returned value cannot be coerced to a `u64`.
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
    /// let ticks = client.read_u64("System.UpTimeMs").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_u64(&self, tag: &str) -> OpcResult<u64> {
        self.read_single_typed(tag, TagValues::get_u64).await
    }

    /// Reads a single tag and returns its complete metadata and value representation.
    ///
    /// Unlike typed read helpers which discard metadata, this method returns a full [`TagValue`]
    /// containing the raw read outcome, quality bitmask ([`crate::types::OpcQuality`]), and timestamp.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication with the server worker fails or the tag
    /// returned no response from the server.
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
    /// let tv = client.read_tag("Sensor.Temperature").await?;
    /// assert_eq!(tv.tag_id, "Sensor.Temperature");
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), err)]
    pub async fn read_tag(&self, tag: &str) -> OpcResult<TagValue> {
        let batch = TagBatch::from_str_lenient(tag);
        let values = self.read_tags(batch).await?;
        values.into_iter().next().ok_or_else(|| {
            OpcError::Internal(format!("Tag '{tag}' returned no response from server"))
        })
    }

    /// Reads multiple tags in a single batch request from the bound server.
    ///
    /// Accepts any type convertible into tags via [`IntoTags`], including array references,
    /// slices, vectors, or pre-constructed [`TagBatch`] instances.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if the worker dispatch or COM transaction fails entirely.
    /// Individual tag failures within the batch are preserved inside the returned [`TagValues`].
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
    /// let values = client.read_tags(["Sensor.1", "Sensor.2"]).await?;
    /// assert_eq!(values.len(), 2);
    /// # Ok(())
    /// # }
    /// ```
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

    /// Writes a single value to the specified tag on the bound server.
    ///
    /// Accepts any value convertible into an [`OpcValue`], including primitive numbers,
    /// booleans, and string references.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if communication fails or the COM server rejects the write request.
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
    /// let res = client.write_tag("Setpoint.Temperature", 72.5f64).await?;
    /// assert!(res.is_success());
    /// # Ok(())
    /// # }
    /// ```
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

    /// Writes a batch of tag-value pairs to the bound server.
    ///
    /// Accepts any type convertible into a [`crate::types::WriteBatch`] via [`IntoWriteBatch`], including
    /// arrays of tuples `[("Tag", value)]`, slices, vectors, or pre-constructed [`crate::types::WriteBatch`].
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if the worker dispatch fails. Individual tag write failures
    /// are recorded in the per-item [`WriteResult`] return vector.
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
    /// let results = client.write_tags([("Tag.A", 10i32), ("Tag.B", 20i32)]).await?;
    /// assert_eq!(results.len(), 2);
    /// # Ok(())
    /// # }
    /// ```
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

    /// Recursively or flatly browses available tags from the bound server.
    ///
    /// Uses the provided [`TagCollector`] to accumulate discovered tag identifiers,
    /// respecting max item limits or branch filters configured on the collector.
    ///
    /// # Errors
    ///
    /// Returns an [`OpcError`] if the server browsing interface fails or worker communication drops.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use opc_da_client::{OpcDaClient, errors::OpcResult, types::TagCollector};
    /// # async fn run<C: opc_da_client::connector::ServerBackend + 'static>(client: &OpcDaClient<C, opc_da_client::client::typestate::Bound>) -> OpcResult<()> {
    /// let tags = client.browse(TagCollector::new(100)).await?;
    /// assert!(!tags.is_empty());
    /// # Ok(())
    /// # }
    /// ```
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
