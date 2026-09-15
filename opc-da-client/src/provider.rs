//! Canonical asynchronous OPC DA service provider abstractions and domain adapters.
//!
//! Defines the foundational [`OpcProvider`] trait for asynchronous server interaction,
//! tag reading/writing, and address space browsing, along with domain types such as
//! [`TagValue`], [`WriteResult`], and bounded [`TagCollector`].

#![allow(async_fn_in_trait)]

use crate::errors::OpcResult;
use crate::types::{
    OpcServerInfo, OpcValue, TagBatch, TagCollector, TagValue, TagValues, WriteResult,
};
/// Role trait for discovering OPC DA servers and querying catalog details.
pub trait ServerDiscovery: Send + Sync {
    /// List available OPC DA servers on the given host.
    ///
    /// # Arguments
    /// * `host` - Hostname or IP address to target (e.g., `"localhost"`).
    ///
    /// # Returns
    /// A list of server ProgIDs sorted alphabetically.
    ///
    /// # Errors
    /// Returns [`crate::errors::OpcError`] if COM initialization fails or the server
    /// registry cannot be enumerated.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// # let mut mock = opc_da_client::MockOpcProvider::new();
    /// # mock.expect_list_servers().returning(|_| Ok(vec!["Matrikon.OPC.Simulation.1".into()]));
    /// # let client = &mock;
    /// use opc_da_client::{OpcProvider, OpcResult, ServerDiscovery};
    ///
    /// let servers = client.list_servers("localhost").await?;
    /// assert_eq!(servers, vec!["Matrikon.OPC.Simulation.1"]);
    /// # Ok(())
    /// # }
    /// ```
    fn list_servers(
        &self,
        host: &str,
    ) -> impl std::future::Future<Output = OpcResult<Vec<String>>> + Send;

    /// Discover detailed information for all registered OPC DA servers on the target host.
    ///
    /// Queries the server list using [`ServerDiscovery::list_servers`] and returns structured
    /// [`OpcServerInfo`] records with normalized host names.
    ///
    /// # Arguments
    ///
    /// * `host` - Hostname, IP address, or `"localhost"`.
    ///
    /// # Returns
    ///
    /// A vector of [`OpcServerInfo`] records containing ProgIDs and server metadata.
    ///
    /// # Errors
    ///
    /// Returns [`crate::errors::OpcError`] if COM library initialization fails,
    /// OPC Enum cannot be instantiated on the target host, or server enumeration fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// # let mut mock = opc_da_client::MockOpcProvider::new();
    /// # mock.expect_list_server_details().returning(|host| Ok(vec![opc_da_client::OpcServerInfo::new(
    /// #     "Matrikon.OPC.Simulation.1",
    /// #     opc_da_client::Clsid::zeroed(),
    /// #     None,
    /// #     Some(host.to_string()),
    /// # )]));
    /// # let client = &mock;
    /// use opc_da_client::{OpcProvider, OpcResult, ServerDiscovery};
    ///
    /// let servers = client.list_server_details("localhost").await?;
    /// assert_eq!(servers.len(), 1);
    /// assert_eq!(servers[0].prog_id(), "Matrikon.OPC.Simulation.1");
    /// # Ok(())
    /// # }
    /// ```
    fn list_server_details(
        &self,
        host: &str,
    ) -> impl std::future::Future<Output = OpcResult<Vec<OpcServerInfo>>> + Send {
        async {
            let servers = self.list_servers(host).await?;
            Ok(crate::types::server_info_from_prog_ids(servers, Some(host)))
        }
    }
}

/// Role trait for browsing OPC DA address spaces.
pub trait TagBrowser: Send + Sync {
    /// Browse tags recursively using the supplied [`TagCollector`].
    ///
    /// The collector controls the capacity limit, tracks incremental discovery counts
    /// lock-free, and supports cooperative cancellation.
    ///
    /// # Arguments
    /// * `server` - ProgID or CLSID of the OPC server.
    /// * `collector` - Configured [`TagCollector`] instance.
    ///
    /// # Returns
    /// The complete list of discovered tag identifiers.
    ///
    /// # Errors
    /// Returns [`crate::errors::OpcError`] if the server connection fails, the `ProgID`
    /// cannot be resolved, or the namespace walk encounters an unrecoverable error.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// # let mut mock = opc_da_client::MockOpcProvider::new();
    /// # mock.expect_browse_tags().returning(|_, collector| {
    /// #     let _ = collector.push("Random.Int4".into());
    /// #     Ok(collector.snapshot())
    /// # });
    /// # let client = &mock;
    /// use opc_da_client::{OpcProvider, OpcResult, TagBrowser, TagCollector};
    ///
    /// let collector = TagCollector::new(100);
    /// let tags = client.browse_tags("Matrikon.OPC.Simulation.1", collector).await?;
    /// assert_eq!(tags, vec!["Random.Int4"]);
    /// # Ok(())
    /// # }
    /// ```
    fn browse_tags(
        &self,
        server: &str,
        collector: TagCollector,
    ) -> impl std::future::Future<Output = OpcResult<Vec<String>>> + Send;
}

/// Role trait for reading OPC DA tag values.
pub trait TagReader: Send + Sync {
    /// Read current values for the given tag batch.
    ///
    /// # Arguments
    /// * `server` - ProgID of the OPC server.
    /// * `tags` - [`TagBatch`] containing tag identifiers to read.
    ///
    /// # Returns
    /// A [`TagValues`] collection preserving input tag order.
    ///
    /// # Errors
    /// Returns [`crate::errors::OpcError`] if the server connection fails, no items
    /// can be added to the OPC group, or the synchronous read operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// # let mut mock = opc_da_client::MockOpcProvider::new();
    /// # mock.expect_read_tag_values().returning(|_, tags| {
    /// #     Ok(opc_da_client::TagValues::new(tags.iter_str().map(|id| opc_da_client::TagValue::new(
    /// #         id.to_string(),
    /// #         Some(opc_da_client::OpcValue::Int(42)),
    /// #         opc_da_client::OpcQuality::GOOD,
    /// #         None,
    /// #     )).collect()))
    /// # });
    /// # let client = &mock;
    /// use opc_da_client::{OpcProvider, OpcResult, TagBatch, TagReader, TagValue};
    ///
    /// let tags = TagBatch::from(vec!["Random.Int4".to_string(), "Random.Real8".to_string()]);
    /// let values = client.read_tag_values("Matrikon.OPC.Simulation.1", tags).await?;
    /// for v in &values {
    ///     let _val = v.display_value();
    /// }
    /// # Ok(())
    /// # }
    /// ```
    fn read_tag_values(
        &self,
        server: &str,
        tags: TagBatch,
    ) -> impl std::future::Future<Output = OpcResult<TagValues>> + Send;

    /// Read a value from a single OPC DA tag.
    ///
    /// Default implementation delegates to [`TagReader::read_tag_values`].
    ///
    /// # Arguments
    /// * `server` - ProgID or identifier of the OPC server.
    /// * `tag_id` - Tag identifier to read.
    ///
    /// # Returns
    /// A [`TagValue`] containing the read value, quality, and timestamp on success.
    ///
    /// # Errors
    /// Returns [`crate::errors::OpcError`] if the underlying read fails or returns empty results.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// # let mut mock = opc_da_client::MockOpcProvider::new();
    /// # mock.expect_read_tag_value().returning(|_, id| {
    /// #     Ok(opc_da_client::TagValue::new(
    /// #         id.to_string(),
    /// #         Some(opc_da_client::OpcValue::Int(42)),
    /// #         opc_da_client::OpcQuality::GOOD,
    /// #         None,
    /// #     ))
    /// # });
    /// # let client = &mock;
    /// use opc_da_client::{OpcProvider, OpcResult, TagReader, TagValue};
    ///
    /// let tag = client.read_tag_value("Matrikon.OPC.Simulation.1", "Random.Int4").await?;
    /// assert_eq!(tag.tag_id, "Random.Int4");
    /// # Ok(())
    /// # }
    /// ```
    fn read_tag_value(
        &self,
        server: &str,
        tag_id: &str,
    ) -> impl std::future::Future<Output = OpcResult<TagValue>> + Send {
        async move {
            let batch = TagBatch::from_str_lenient(tag_id);
            let results = self.read_tag_values(server, batch).await?;
            results.into_iter().next().ok_or_else(|| {
                crate::errors::OpcError::Internal(format!(
                    "reader returned empty results for single tag read: {tag_id}"
                ))
            })
        }
    }
}

/// Role trait for writing OPC DA tag values.
pub trait TagWriter: Send + Sync {
    /// Write a value to a single OPC DA tag.
    ///
    /// # Arguments
    /// * `server` - ProgID of the OPC server.
    /// * `tag_id` - Tag identifier to write to.
    /// * `value` - Strongly-typed [`OpcValue`] to write.
    ///
    /// # Returns
    /// A [`WriteResult`] indicating per-tag write success or failure.
    ///
    /// # Errors
    /// Returns [`crate::errors::OpcError`] if the server connection fails, the tag
    /// cannot be added to the OPC group, or the synchronous write operation fails.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[tokio::main]
    /// # async fn main() -> opc_da_client::OpcResult<()> {
    /// # let mut mock = opc_da_client::MockOpcProvider::new();
    /// # mock.expect_write_tag_value().returning(|_, id, _| {
    /// #     Ok(opc_da_client::WriteResult::success(id.to_string()))
    /// # });
    /// # let client = &mock;
    /// use opc_da_client::{OpcProvider, OpcResult, OpcValue, TagWriter, WriteResult};
    ///
    /// let result = client
    ///     .write_tag_value("Matrikon.OPC.Simulation.1", "Bucket Brigade.Int4", OpcValue::Int(42))
    ///     .await?;
    /// assert!(result.is_success());
    /// # Ok(())
    /// # }
    /// ```
    fn write_tag_value(
        &self,
        server: &str,
        tag_id: &str,
        value: OpcValue,
    ) -> impl std::future::Future<Output = OpcResult<WriteResult>> + Send;

    /// Write typed values to multiple OPC DA tags in a batch using [`crate::types::WriteBatch`].
    ///
    /// Default implementation iteratively invokes [`TagWriter::write_tag_value`].
    /// Note: Partial failures do NOT abort the remaining writes; all writes are attempted
    /// and per-item outcomes are preserved.
    ///
    /// # Arguments
    /// * `server` - ProgID or identifier of the OPC server.
    /// * `writes` - Batch of tag writes.
    ///
    /// # Returns
    /// A vector of [`WriteResult`] structs corresponding to each tag write attempt.
    ///
    /// # Errors
    /// Returns [`crate::errors::OpcError`] if an unrecoverable error occurs.
    fn write_tag_batch(
        &self,
        server: &str,
        writes: crate::types::WriteBatch,
    ) -> impl std::future::Future<Output = OpcResult<Vec<WriteResult>>> + Send {
        async {
            let mut results = Vec::with_capacity(writes.len());
            for (tag_id, value) in writes {
                let res = match self.write_tag_value(server, &tag_id, value).await {
                    Ok(r) => r,
                    Err(e) => WriteResult::failure(tag_id, e),
                };
                results.push(res);
            }
            Ok(results)
        }
    }

    /// Write typed values to multiple OPC DA tags in a batch.
    ///
    /// # Deprecated
    /// Use [`TagWriter::write_tag_batch`] instead.
    #[deprecated(since = "0.2.0", note = "Use write_tag_batch instead")]
    fn write_tag_values(
        &self,
        server: &str,
        writes: &[(String, OpcValue)],
    ) -> impl std::future::Future<Output = OpcResult<Vec<WriteResult>>> + Send {
        async {
            self.write_tag_batch(server, crate::types::WriteBatch::Owned(writes.to_vec()))
                .await
        }
    }
}

/// Composite asynchronous OPC DA service provider abstraction.
///
/// Blends [`ServerDiscovery`], [`TagBrowser`], [`TagReader`], and [`TagWriter`].
///
/// All asynchronous role methods are bound by Return Type Notation (RTN) to yield [`Send`]
/// futures so they can safely cross thread boundaries in asynchronous runtimes.
#[allow(deprecated)]
pub trait OpcProvider:
    ServerDiscovery + TagBrowser + TagReader + TagWriter + Send + Sync + 'static
{
}

#[allow(deprecated)]
impl<T> OpcProvider for T where
    T: ServerDiscovery + TagBrowser + TagReader + TagWriter + Send + Sync + 'static
{
}

#[cfg(feature = "test-support")]
mod mock {
    #![allow(clippy::struct_field_names, deprecated)]
    use super::{
        OpcResult, OpcServerInfo, OpcValue, ServerDiscovery, TagBatch, TagBrowser, TagCollector,
        TagReader, TagValue, TagValues, TagWriter, WriteResult,
    };

    mockall::mock! {
        /// Pure-Rust mock implementation of [`OpcProvider`] for unit and integration tests.
        pub OpcProvider {}

        impl ServerDiscovery for OpcProvider {
            async fn list_servers(&self, host: &str) -> OpcResult<Vec<String>>;
            async fn list_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>>;
        }

        impl TagBrowser for OpcProvider {
            async fn browse_tags(&self, server: &str, collector: TagCollector) -> OpcResult<Vec<String>>;
        }

        impl TagReader for OpcProvider {
            async fn read_tag_values(&self, server: &str, tags: TagBatch) -> OpcResult<TagValues>;
            async fn read_tag_value(&self, server: &str, tag_id: &str) -> OpcResult<TagValue>;
        }

        impl TagWriter for OpcProvider {
            async fn write_tag_value(
                &self,
                server: &str,
                tag_id: &str,
                value: OpcValue,
            ) -> OpcResult<WriteResult>;
            async fn write_tag_batch(
                &self,
                server: &str,
                writes: crate::types::WriteBatch,
            ) -> OpcResult<Vec<WriteResult>>;
            async fn write_tag_values(
                &self,
                server: &str,
                writes: &[(String, OpcValue)],
            ) -> OpcResult<Vec<WriteResult>>;
        }
    }

    mockall::mock! {
        /// Pure-Rust mock implementation of [`ServerDiscovery`] for unit and integration tests.
        pub ServerDiscovery {}

        impl ServerDiscovery for ServerDiscovery {
            async fn list_servers(&self, host: &str) -> OpcResult<Vec<String>>;
            async fn list_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>>;
        }
    }

    mockall::mock! {
        /// Pure-Rust mock implementation of [`TagBrowser`] for unit and integration tests.
        pub TagBrowser {}

        impl TagBrowser for TagBrowser {
            async fn browse_tags(&self, server: &str, collector: TagCollector) -> OpcResult<Vec<String>>;
        }
    }

    mockall::mock! {
        /// Pure-Rust mock implementation of [`TagReader`] for unit and integration tests.
        pub TagReader {}

        impl TagReader for TagReader {
            async fn read_tag_values(&self, server: &str, tags: TagBatch) -> OpcResult<TagValues>;
            async fn read_tag_value(&self, server: &str, tag_id: &str) -> OpcResult<TagValue>;
        }
    }

    mockall::mock! {
        /// Pure-Rust mock implementation of [`TagWriter`] for unit and integration tests.
        pub TagWriter {}

        impl TagWriter for TagWriter {
            async fn write_tag_value(
                &self,
                server: &str,
                tag_id: &str,
                value: OpcValue,
            ) -> OpcResult<WriteResult>;
            async fn write_tag_batch(
                &self,
                server: &str,
                writes: crate::types::WriteBatch,
            ) -> OpcResult<Vec<WriteResult>>;
            async fn write_tag_values(
                &self,
                server: &str,
                writes: &[(String, OpcValue)],
            ) -> OpcResult<Vec<WriteResult>>;
        }
    }
}
#[cfg(feature = "test-support")]
pub use mock::{
    MockOpcProvider, MockServerDiscovery, MockTagBrowser, MockTagReader, MockTagWriter,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::OpcQuality;

    #[tokio::test]
    async fn test_provider_default_list_server_details() {
        struct TestProvider;
        impl ServerDiscovery for TestProvider {
            async fn list_servers(&self, _host: &str) -> OpcResult<Vec<String>> {
                Ok(vec!["Server.A".into(), "Server.B".into()])
            }
        }
        impl TagBrowser for TestProvider {
            async fn browse_tags(&self, _s: &str, _c: TagCollector) -> OpcResult<Vec<String>> {
                Ok(vec![])
            }
        }
        impl TagReader for TestProvider {
            async fn read_tag_values(&self, _s: &str, _t: TagBatch) -> OpcResult<TagValues> {
                Ok(TagValues::default())
            }
        }
        impl TagWriter for TestProvider {
            async fn write_tag_value(
                &self,
                _s: &str,
                _t: &str,
                _v: OpcValue,
            ) -> OpcResult<WriteResult> {
                Ok(WriteResult::success("tag"))
            }
        }

        let p = TestProvider;
        let details = p.list_server_details("localhost").await.unwrap();
        assert_eq!(details.len(), 2);
        assert_eq!(details[0].prog_id(), "Server.A");
        assert_eq!(details[0].clsid(), crate::types::Clsid::zeroed());
        assert_eq!(details[0].user_type(), None);
        assert_eq!(details[0].host(), None);
    }

    #[tokio::test]
    #[allow(deprecated)]
    async fn test_provider_default_read_tag_value() {
        struct TestProvider;
        impl ServerDiscovery for TestProvider {
            async fn list_servers(&self, _host: &str) -> OpcResult<Vec<String>> {
                Ok(vec![])
            }
        }
        impl TagBrowser for TestProvider {
            async fn browse_tags(&self, _s: &str, _c: TagCollector) -> OpcResult<Vec<String>> {
                Ok(vec![])
            }
        }
        impl TagReader for TestProvider {
            async fn read_tag_values(&self, _s: &str, tags: TagBatch) -> OpcResult<TagValues> {
                Ok(TagValues::new(
                    tags.iter_str()
                        .map(|t| {
                            TagValue::new(
                                t.to_string(),
                                Some(OpcValue::Int(42)),
                                OpcQuality::GOOD,
                                None,
                            )
                        })
                        .collect(),
                ))
            }
        }
        impl TagWriter for TestProvider {
            async fn write_tag_value(
                &self,
                _s: &str,
                tag: &str,
                _v: OpcValue,
            ) -> OpcResult<WriteResult> {
                Ok(WriteResult::success(tag))
            }
        }

        let p = TestProvider;
        let val = p.read_tag_value("Server.A", "Tag.1").await.unwrap();
        assert_eq!(val.tag_id, "Tag.1");
        assert_eq!(val.value(), Some(&OpcValue::Int(42)));

        let batch_write = p
            .write_tag_values(
                "Server.A",
                &[
                    ("Tag.1".into(), OpcValue::Int(10)),
                    ("Tag.2".into(), OpcValue::Int(20)),
                ],
            )
            .await
            .unwrap();
        assert_eq!(batch_write.len(), 2);
        assert!(batch_write[0].is_success());
        assert!(batch_write[1].is_success());

        let batch_direct = p
            .write_tag_batch(
                "Server.A",
                crate::types::WriteBatch::Owned(vec![
                    ("Tag.1".into(), OpcValue::Int(10)),
                    ("Tag.2".into(), OpcValue::Int(20)),
                ]),
            )
            .await
            .unwrap();
        assert_eq!(batch_direct.len(), 2);
        assert!(batch_direct[0].is_success());
        assert!(batch_direct[1].is_success());
    }

    #[tokio::test]
    #[allow(deprecated)]
    async fn test_provider_default_write_tag_values_partial_failure() {
        struct FailingProvider;
        impl ServerDiscovery for FailingProvider {
            async fn list_servers(&self, _host: &str) -> OpcResult<Vec<String>> {
                Ok(vec![])
            }
        }
        impl TagBrowser for FailingProvider {
            async fn browse_tags(&self, _s: &str, _c: TagCollector) -> OpcResult<Vec<String>> {
                Ok(vec![])
            }
        }
        impl TagReader for FailingProvider {
            async fn read_tag_values(&self, _s: &str, _tags: TagBatch) -> OpcResult<TagValues> {
                Ok(TagValues::default())
            }
        }
        impl TagWriter for FailingProvider {
            async fn write_tag_value(
                &self,
                _s: &str,
                tag: &str,
                _v: OpcValue,
            ) -> OpcResult<WriteResult> {
                if tag == "Tag.Fail" {
                    Err(crate::errors::OpcError::Internal("Write rejected".into()))
                } else {
                    Ok(WriteResult::success(tag))
                }
            }
        }

        let p = FailingProvider;
        let results = p
            .write_tag_values(
                "Server.A",
                &[
                    ("Tag.Fail".into(), OpcValue::Int(1)),
                    ("Tag.Pass".into(), OpcValue::Int(2)),
                ],
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(!results[0].is_success());
        assert!(results[1].is_success());
    }

    #[tokio::test]
    async fn test_tag_reader_default_read_tag_value_fifo() {
        use crate::errors::OpcResult;
        use crate::provider::TagReader;
        use crate::types::{OpcQuality, OpcValue, TagBatch, TagValue, TagValues};

        struct FifoReader;
        impl TagReader for FifoReader {
            async fn read_tag_values(
                &self,
                _server: &str,
                _tags: TagBatch,
            ) -> OpcResult<TagValues> {
                Ok(TagValues::new(vec![
                    TagValue::new(
                        "Sensor.First".to_string(),
                        Some(OpcValue::Int(1)),
                        OpcQuality::GOOD,
                        None,
                    ),
                    TagValue::new(
                        "Sensor.Second".to_string(),
                        Some(OpcValue::Int(2)),
                        OpcQuality::GOOD,
                        None,
                    ),
                ]))
            }
        }

        let reader = FifoReader;
        let val = reader
            .read_tag_value("TestServer", "Sensor.First")
            .await
            .expect("Default read_tag_value should succeed");

        assert_eq!(val.tag_id, "Sensor.First");
        assert_eq!(val.value(), Some(&OpcValue::Int(1)));

        struct EmptyReader;
        impl TagReader for EmptyReader {
            async fn read_tag_values(
                &self,
                _server: &str,
                _tags: TagBatch,
            ) -> OpcResult<TagValues> {
                Ok(TagValues::new(vec![]))
            }
        }
        let empty_reader = EmptyReader;
        let err = empty_reader
            .read_tag_value("TestServer", "Sensor.First")
            .await
            .expect_err("Empty tag values must return Err");
        assert!(matches!(err, crate::errors::OpcError::Internal(_)));
    }
}
