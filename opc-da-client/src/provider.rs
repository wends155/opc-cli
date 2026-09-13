//! Canonical asynchronous OPC DA service provider abstractions and domain adapters.
//!
//! Defines the foundational [`OpcProvider`] trait for asynchronous server interaction,
//! tag reading/writing, and address space browsing, along with domain types such as
//! [`TagValue`], [`WriteResult`], and bounded [`TagCollector`].

#![allow(async_fn_in_trait)]

use crate::errors::OpcResult;
pub use crate::types::{
    DisplayOptionOpcValue, DisplayOptionTimestamp, OpcQuality, OpcServerInfo, OpcValue,
    OpcValueOptionExt, QualityLimit, QualityMajor, QualitySubstatus, SystemTimeOptionExt, TagBatch,
    TagCollector, TagValue, TagValues, WriteResult,
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

    fn list_server_details(
        &self,
        host: &str,
    ) -> impl std::future::Future<Output = OpcResult<Vec<OpcServerInfo>>> + Send {
        async {
            let servers = self.list_servers(host).await?;
            let host_opt = crate::types::normalize_host(Some(host));
            Ok(servers
                .into_iter()
                .map(|prog_id| OpcServerInfo {
                    prog_id,
                    clsid: crate::types::Clsid::zeroed(),
                    user_type: None,
                    host: host_opt.clone(),
                })
                .collect())
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
        async {
            let batch = TagBatch::from_str_lenient(tag_id);
            let results = self.read_tag_values(server, batch).await?;
            results.into_vec().pop().ok_or_else(|| {
                crate::errors::OpcError::Internal("Server returned empty tag values".to_string())
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
}
#[cfg(feature = "test-support")]
pub use mock::MockOpcProvider;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_tag_value_helpers_success() {
        let tv = TagValue {
            tag_id: "Tag1".to_string(),
            outcome: Ok(OpcValue::Int(42)),
            quality: OpcQuality::GOOD,
            timestamp: Some(SystemTime::UNIX_EPOCH),
        };
        assert!(tv.is_good());
        assert!(!tv.is_error());
        assert_eq!(tv.display_value(), "42");
        assert_eq!(tv.formatted_timestamp(), "N/A"); // UNIX_EPOCH returns "N/A" in helper
    }

    #[test]
    fn test_tag_value_helpers_failure() {
        let tv = TagValue {
            tag_id: "Tag2".to_string(),
            outcome: Err(crate::errors::OpcError::Internal("Comm failed".into())),
            quality: OpcQuality::BAD_COMM_FAILURE,
            timestamp: None,
        };
        assert!(!tv.is_good());
        assert!(tv.is_error());
        assert_eq!(tv.display_value(), "Error");
        assert_eq!(tv.formatted_timestamp(), "N/A");
    }

    #[test]
    fn test_opc_value_display() {
        assert_eq!(OpcValue::String("hello".into()).to_string(), "hello");
        assert_eq!(OpcValue::Int(100).to_string(), "100");
        assert_eq!(OpcValue::Float(12.34).to_string(), "12.34");
        assert_eq!(OpcValue::Bool(true).to_string(), "true");
        assert_eq!(OpcValue::Bool(false).to_string(), "false");
        assert_eq!(OpcValue::Empty.to_string(), "Empty");
        assert_eq!(OpcValue::Null.to_string(), "Null");
    }

    #[test]
    fn test_opc_value_option_ext_some() {
        let val_opt = Some(OpcValue::Int(42));
        assert_eq!(format!("{}", val_opt.display()), "42");
        assert_eq!(format!("{}", val_opt.display_or("Custom")), "42");

        let val_ref = val_opt.as_ref();
        assert_eq!(format!("{}", val_ref.display()), "42");
        assert_eq!(format!("{}", val_ref.display_or("Custom")), "42");
    }

    #[test]
    fn test_opc_value_option_ext_none() {
        let val_opt: Option<OpcValue> = None;
        assert_eq!(format!("{}", val_opt.display()), "Error");
        assert_eq!(format!("{}", val_opt.display_or("Custom")), "Custom");

        let val_ref = val_opt.as_ref();
        assert_eq!(format!("{}", val_ref.display()), "Error");
        assert_eq!(format!("{}", val_ref.display_or("Custom")), "Custom");
    }

    #[test]
    fn test_system_time_option_ext_some() {
        // Non-epoch time (1700000000 = 2023-11-14 22:13:20 UTC)
        let ts = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let ts_opt = Some(ts);
        let expected = "2023-11-14 22:13:20";
        assert_eq!(format!("{}", ts_opt.display()), expected);
        assert_eq!(format!("{}", ts_opt.display_or("Custom")), expected);
    }

    #[test]
    fn test_system_time_option_ext_none_and_epoch() {
        let ts_none: Option<SystemTime> = None;
        assert_eq!(format!("{}", ts_none.display()), "N/A");
        assert_eq!(format!("{}", ts_none.display_or("Custom")), "Custom");

        let ts_epoch = Some(SystemTime::UNIX_EPOCH);
        assert_eq!(format!("{}", ts_epoch.display()), "N/A");
        assert_eq!(format!("{}", ts_epoch.display_or("Custom")), "Custom");
    }

    #[test]
    fn test_tag_value_display() {
        let tv = TagValue {
            tag_id: "Simulation.Item1".to_string(),
            outcome: Ok(OpcValue::Float(99.5)),
            quality: OpcQuality::GOOD,
            timestamp: Some(SystemTime::UNIX_EPOCH),
        };
        assert_eq!(format!("{tv}"), "Simulation.Item1 = 99.5 [Good] @ N/A");
    }

    #[test]
    fn test_tag_value_destructuring_ergonomics() {
        let tv = TagValue {
            tag_id: "Device1.Tag1".to_string(),
            outcome: Ok(OpcValue::String("Active".into())),
            quality: OpcQuality::GOOD,
            timestamp: None,
        };

        // Exact pattern destructuring
        let TagValue {
            tag_id,
            outcome,
            quality,
            timestamp,
        } = tv;

        let formatted = format!(
            "Tag: {:<15} | Value: {:<10} | Quality: {:<6} | Timestamp: {}",
            tag_id,
            outcome.ok().display(),
            quality,
            timestamp.display_or("N/A")
        );

        assert_eq!(
            formatted,
            "Tag: Device1.Tag1    | Value: Active     | Quality: Good   | Timestamp: N/A"
        );
    }

    #[test]
    fn test_tag_collector_lifecycle() {
        let collector = TagCollector::new(5);
        assert_eq!(collector.len(), 0);
        assert!(collector.is_empty());
        assert_eq!(collector.max_tags(), 5);
        assert!(!collector.is_full());
        assert!(!collector.is_cancelled());

        assert!(collector.push("Tag1".into()));
        assert!(collector.push("Tag2".into()));
        assert_eq!(collector.len(), 2);
        assert!(!collector.is_empty());
        assert!(!collector.is_full());

        let snap = collector.snapshot();
        assert_eq!(snap, vec!["Tag1".to_string(), "Tag2".to_string()]);
        assert_eq!(collector.len(), 2);

        let harvested = collector.harvest();
        assert_eq!(harvested, vec!["Tag1".to_string(), "Tag2".to_string()]);
        assert_eq!(collector.len(), 0);
        assert!(collector.is_empty());
    }

    #[test]
    fn test_tag_collector_capacity_cap() {
        let collector = TagCollector::new(2);
        assert!(collector.push("T1".into()));
        assert!(collector.push("T2".into()));
        assert_eq!(collector.len(), 2);
        assert!(collector.is_full());

        // Further pushes must be rejected
        assert!(!collector.push("T3".into()));
        assert_eq!(collector.len(), 2);
        assert_eq!(
            collector.snapshot(),
            vec!["T1".to_string(), "T2".to_string()]
        );
    }

    #[test]
    fn test_tag_collector_unbounded() {
        let collector = TagCollector::unbounded();
        assert_eq!(collector.max_tags(), usize::MAX);
        assert!(!collector.is_full());
        assert!(collector.push("A".into()));
        assert!(!collector.is_full());
    }

    #[test]
    fn test_tag_collector_cancellation() {
        let collector = TagCollector::new(10);
        let c1 = collector.clone();
        let c2 = collector.clone();

        assert!(!c1.is_cancelled());
        assert!(!c2.is_cancelled());

        collector.cancel();
        assert!(c1.is_cancelled());
        assert!(c2.is_cancelled());
        assert!(collector.is_cancelled());

        // Pushes after cancellation must be rejected
        assert!(!collector.push("T1".into()));
        assert_eq!(collector.len(), 0);
    }

    #[test]
    fn test_tag_collector_multithreaded() {
        let collector = TagCollector::new(400);
        let handles: Vec<_> = (0..4)
            .map(|thread_id| {
                let col = collector.clone();
                std::thread::spawn(move || {
                    for i in 0..100 {
                        assert!(col.push(format!("T_{thread_id}_{i}")));
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(collector.len(), 400);
        assert!(collector.is_full());
        let tags = collector.harvest();
        assert_eq!(tags.len(), 400);
        assert_eq!(collector.len(), 0);
    }

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
        assert_eq!(details[0].prog_id, "Server.A");
        assert_eq!(details[0].clsid, crate::types::Clsid::zeroed());
        assert_eq!(details[0].user_type, None);
        assert_eq!(details[0].host, None);
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
}
