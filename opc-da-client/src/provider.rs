use crate::errors::OpcResult;
pub use crate::types::{OpcQuality, QualityLimit, QualityMajor, QualitySubstatus};
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
/// use opc_da_client::{OpcQuality, OpcValue, TagValue};
/// use std::time::SystemTime;
///
/// let tv = TagValue {
///     tag_id: "Simulation.Random.1".to_string(),
///     value: Some(OpcValue::Float(42.5)),
///     quality: OpcQuality::GOOD,
///     timestamp: Some(SystemTime::UNIX_EPOCH),
/// };
/// assert_eq!(tv.tag_id, "Simulation.Random.1");
/// assert!(tv.is_good());
/// assert_eq!(tv.display_value(), "42.5");
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TagValue {
    /// The fully qualified tag identifier (e.g., `"Channel1.Device1.Tag1"`).
    pub tag_id: String,
    /// The decoded value, or `None` if the tag read failed or value is unavailable.
    pub value: Option<OpcValue>,
    /// OPC quality status, decomposed into major quality, substatus, and limit bits.
    pub quality: OpcQuality,
    /// Timestamp of the last value change (UTC-based), or `None` if unavailable.
    pub timestamp: Option<std::time::SystemTime>,
}

impl TagValue {
    /// Returns `true` if quality is good and a value is present.
    #[must_use]
    pub fn is_good(&self) -> bool {
        self.quality.is_good() && self.value.is_some()
    }

    /// Returns `true` if quality is bad or value is missing.
    #[must_use]
    pub fn is_error(&self) -> bool {
        !self.is_good()
    }

    /// Returns a human-readable display string for the value (or `"Error"` if missing).
    #[must_use]
    pub fn display_value(&self) -> String {
        match &self.value {
            Some(v) => v.to_string(),
            None => "Error".to_string(),
        }
    }

    /// Returns a human-readable formatted local timestamp string (or `"N/A"` if missing).
    #[must_use]
    pub fn formatted_timestamp(&self) -> String {
        match self.timestamp {
            Some(ts) => crate::helpers::system_time_to_string(ts),
            None => "N/A".to_string(),
        }
    }
}

/// Typed value to write to or read from an OPC DA tag.
///
/// # Examples
///
/// ```
/// use opc_da_client::OpcValue;
///
/// let v = OpcValue::Float(3.14);
/// assert_eq!(v, OpcValue::Float(3.14));
/// assert_eq!(v.to_string(), "3.14");
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
    /// Empty value (`VT_EMPTY`) — uninitialized or absent variant.
    Empty,
    /// Null value (`VT_NULL`) — explicitly null variant.
    Null,
}

impl std::fmt::Display for OpcValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => write!(f, "{s}"),
            Self::Int(i) => write!(f, "{i}"),
            Self::Float(fl) => write!(f, "{fl}"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Empty => write!(f, "Empty"),
            Self::Null => write!(f, "Null"),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_tag_value_helpers_success() {
        let tv = TagValue {
            tag_id: "Tag1".to_string(),
            value: Some(OpcValue::Int(42)),
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
            value: None,
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
}
