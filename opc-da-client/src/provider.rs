use crate::opc_da::errors::OpcResult;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

#[cfg(feature = "test-support")]
use mockall::automock;

/// A single tag's read result.
///
/// Returned by [`OpcProvider::read_tag_values`].
///
/// # Examples
///
/// ```
/// use opc_da_client::TagValue;
///
/// let tv = TagValue {
///     tag_id: "Simulation.Random.1".to_string(),
///     value: "42.5".to_string(),
///     quality: "Good".to_string(),
///     timestamp: "2026-01-01 00:00:00".to_string(),
/// };
/// assert_eq!(tv.tag_id, "Simulation.Random.1");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagValue {
    /// The fully qualified tag identifier (e.g., `"Channel1.Device1.Tag1"`).
    pub tag_id: String,
    /// The current value as a display string.
    pub value: String,
    /// OPC quality indicator (e.g., `"Good"`, `"Bad"`, or `"Uncertain"`).
    pub quality: String,
    /// Timestamp of the last value change, formatted as a local time string.
    pub timestamp: String,
}

/// Typed value to write to an OPC DA tag.
///
/// # Examples
///
/// ```
/// use opc_da_client::OpcValue;
///
/// let v = OpcValue::Float(3.14);
/// assert_eq!(v, OpcValue::Float(3.14));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum OpcValue {
    /// String value (`VT_BSTR`) — server may coerce to target type.
    String(String),
    /// 32-bit integer (`VT_I4`).
    Int(i32),
    /// 64-bit float (`VT_R8`).
    Float(f64),
    /// Boolean (`VT_BOOL`).
    Bool(bool),
}

/// Result of a single write operation.
///
/// # Examples
///
/// ```
/// use opc_da_client::WriteResult;
///
/// let wr = WriteResult {
///     tag_id: "Tag1".to_string(),
///     success: true,
///     error: None,
/// };
/// assert!(wr.success);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteResult {
    /// The tag that was written to.
    pub tag_id: String,
    /// Whether the write succeeded.
    pub success: bool,
    /// Error message if the write failed, `None` on success.
    pub error: Option<String>,
}

/// Async trait for OPC DA operations.
///
/// This is the stable public API. Backend implementations provide
/// the actual COM/DCOM interaction.
#[cfg_attr(feature = "test-support", automock)]
#[async_trait]
pub trait OpcProvider: Send + Sync {
    /// List available OPC DA servers on the given host.
    ///
    /// # Errors
    /// Returns `Err` if COM initialization fails or the server registry
    /// cannot be enumerated.
    async fn list_servers(&self, host: &str) -> OpcResult<Vec<String>>;

    /// Browse tags recursively, pushing discoveries to `tags_sink`.
    ///
    /// # Errors
    /// Returns `Err` if the server connection fails, the `ProgID` cannot be
    /// resolved, or the namespace walk encounters an unrecoverable error.
    async fn browse_tags(
        &self,
        server: &str,
        max_tags: usize,
        progress: Arc<AtomicUsize>,
        tags_sink: Arc<std::sync::Mutex<Vec<String>>>,
    ) -> OpcResult<Vec<String>>;

    /// Read current values for the given tag IDs.
    ///
    /// # Errors
    /// Returns `Err` if the server connection fails, no items can be added
    /// to the OPC group, or the synchronous read operation fails.
    async fn read_tag_values(&self, server: &str, tag_ids: Vec<String>)
    -> OpcResult<Vec<TagValue>>;

    /// Write a value to a single OPC DA tag.
    ///
    /// # Errors
    /// Returns `Err` if the server connection fails, the tag cannot be added
    /// to the OPC group, or the synchronous write operation fails.
    async fn write_tag_value(
        &self,
        server: &str,
        tag_id: &str,
        value: OpcValue,
    ) -> OpcResult<WriteResult>;
}
