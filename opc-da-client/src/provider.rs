use crate::errors::{OpcError, OpcResult};
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

impl std::fmt::Display for TagValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} = {} [{}] @ {}",
            self.tag_id,
            self.value.display(),
            self.quality,
            self.timestamp.display()
        )
    }
}

/// Zero-allocation display adapter for [`Option<OpcValue>`].
///
/// Implements [`std::fmt::Display`] to stream the formatted inner value or
/// a fallback string directly into the output formatter without heap allocations.
///
/// # Examples
///
/// ```
/// use opc_da_client::{OpcValue, OpcValueOptionExt};
///
/// let some_val = Some(OpcValue::Int(42));
/// assert_eq!(format!("{}", some_val.display()), "42");
/// assert_eq!(format!("{}", some_val.display_or("Missing")), "42");
///
/// let none_val: Option<OpcValue> = None;
/// assert_eq!(format!("{}", none_val.display()), "Error");
/// assert_eq!(format!("{}", none_val.display_or("Missing")), "Missing");
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayOptionOpcValue<'a> {
    opt: Option<&'a OpcValue>,
    fallback: &'a str,
}

impl<'a> std::fmt::Display for DisplayOptionOpcValue<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.opt {
            Some(v) => {
                if f.width().is_some() {
                    let s = v.to_string();
                    f.pad(&s)
                } else {
                    write!(f, "{v}")
                }
            }
            None => f.pad(self.fallback),
        }
    }
}

/// Zero-allocation display adapter for [`Option<std::time::SystemTime>`].
///
/// Implements [`std::fmt::Display`] to stream a local formatted timestamp or
/// a fallback string directly into the output formatter without heap allocations.
///
/// # Examples
///
/// ```
/// use opc_da_client::SystemTimeOptionExt;
/// use std::time::SystemTime;
///
/// let none_time: Option<SystemTime> = None;
/// assert_eq!(format!("{}", none_time.display()), "N/A");
/// assert_eq!(format!("{}", none_time.display_or("None")), "None");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayOptionTimestamp<'a> {
    opt: Option<std::time::SystemTime>,
    fallback: &'a str,
}

impl<'a> std::fmt::Display for DisplayOptionTimestamp<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.opt {
            Some(ts) if ts != std::time::SystemTime::UNIX_EPOCH => {
                let dt: chrono::DateTime<chrono::Local> = ts.into();
                let formatted = dt.format("%Y-%m-%d %H:%M:%S");
                if f.width().is_some() {
                    let s = formatted.to_string();
                    f.pad(&s)
                } else {
                    write!(f, "{formatted}")
                }
            }
            _ => f.pad(self.fallback),
        }
    }
}

/// Extension trait providing zero-allocation formatting helpers for [`Option<OpcValue>`].
pub trait OpcValueOptionExt {
    /// Returns a zero-allocation display adapter with custom fallback text.
    ///
    /// # Arguments
    ///
    /// * `fallback` - Text to render when the option is `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcValue, OpcValueOptionExt};
    ///
    /// let val = Some(OpcValue::Int(10));
    /// assert_eq!(format!("{}", val.display_or("Err")), "10");
    /// ```
    fn display_or<'a>(&'a self, fallback: &'a str) -> DisplayOptionOpcValue<'a>;

    /// Returns a zero-allocation display adapter with the canonical default fallback (`"Error"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcValue, OpcValueOptionExt};
    ///
    /// let val: Option<OpcValue> = None;
    /// assert_eq!(format!("{}", val.display()), "Error");
    /// ```
    fn display(&self) -> DisplayOptionOpcValue<'_> {
        self.display_or("Error")
    }
}

impl OpcValueOptionExt for Option<OpcValue> {
    fn display_or<'a>(&'a self, fallback: &'a str) -> DisplayOptionOpcValue<'a> {
        DisplayOptionOpcValue {
            opt: self.as_ref(),
            fallback,
        }
    }
}

impl<'b> OpcValueOptionExt for Option<&'b OpcValue> {
    fn display_or<'a>(&'a self, fallback: &'a str) -> DisplayOptionOpcValue<'a> {
        DisplayOptionOpcValue {
            opt: *self,
            fallback,
        }
    }
}

/// Extension trait providing zero-allocation formatting helpers for [`Option<std::time::SystemTime>`].
pub trait SystemTimeOptionExt {
    /// Returns a zero-allocation display adapter with custom fallback text.
    ///
    /// # Arguments
    ///
    /// * `fallback` - Text to render when the timestamp is `None` or [`std::time::SystemTime::UNIX_EPOCH`].
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::SystemTimeOptionExt;
    /// use std::time::SystemTime;
    ///
    /// let ts: Option<SystemTime> = None;
    /// assert_eq!(format!("{}", ts.display_or("Unavailable")), "Unavailable");
    /// ```
    fn display_or<'a>(&'a self, fallback: &'a str) -> DisplayOptionTimestamp<'a>;

    /// Returns a zero-allocation display adapter with the canonical default fallback (`"N/A"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::SystemTimeOptionExt;
    /// use std::time::SystemTime;
    ///
    /// let ts: Option<SystemTime> = None;
    /// assert_eq!(format!("{}", ts.display()), "N/A");
    /// ```
    fn display(&self) -> DisplayOptionTimestamp<'_> {
        self.display_or("N/A")
    }
}

impl SystemTimeOptionExt for Option<std::time::SystemTime> {
    fn display_or<'a>(&'a self, fallback: &'a str) -> DisplayOptionTimestamp<'a> {
        DisplayOptionTimestamp {
            opt: *self,
            fallback,
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
/// use opc_da_client::{OpcError, WriteResult};
///
/// let ok_res = WriteResult::success("Tag1");
/// assert!(ok_res.is_success());
/// assert!(ok_res.status.is_ok());
/// assert!(ok_res.error().is_none());
///
/// let err_res = WriteResult::failure("Tag2", OpcError::Connection("Lost".into()));
/// assert!(err_res.is_error());
/// assert!(err_res.status.is_err());
/// assert_eq!(err_res.error(), Some(&OpcError::Connection("Lost".into())));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct WriteResult {
    /// The tag that was written to.
    pub tag_id: String,
    /// Outcome of the write operation: `Ok(())` on success, or `Err(OpcError)` on failure.
    pub status: Result<(), OpcError>,
}

impl WriteResult {
    /// Creates a successful write result.
    ///
    /// # Arguments
    ///
    /// * `tag_id` - Identifier of the tag that was successfully written.
    ///
    /// # Returns
    ///
    /// A [`WriteResult`] with [`WriteResult::status`] set to `Ok(())`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::WriteResult;
    ///
    /// let res = WriteResult::success("Channel1.Device1.Tag1");
    /// assert!(res.is_success());
    /// assert_eq!(res.tag_id, "Channel1.Device1.Tag1");
    /// ```
    #[must_use]
    pub fn success(tag_id: impl Into<String>) -> Self {
        Self {
            tag_id: tag_id.into(),
            status: Ok(()),
        }
    }

    /// Creates a failed write result with a domain error.
    ///
    /// # Arguments
    ///
    /// * `tag_id` - Identifier of the tag whose write operation failed.
    /// * `error` - Concrete [`OpcError`] describing the reason for failure.
    ///
    /// # Returns
    ///
    /// A [`WriteResult`] with [`WriteResult::status`] set to `Err(error)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcError, WriteResult};
    ///
    /// let res = WriteResult::failure("Channel1.Device1.Tag1", OpcError::Connection("Unreachable".into()));
    /// assert!(res.is_error());
    /// assert_eq!(res.error(), Some(&OpcError::Connection("Unreachable".into())));
    /// ```
    #[must_use]
    pub fn failure(tag_id: impl Into<String>, error: OpcError) -> Self {
        Self {
            tag_id: tag_id.into(),
            status: Err(error),
        }
    }

    /// Returns `true` if the write succeeded.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::WriteResult;
    ///
    /// let res = WriteResult::success("Tag1");
    /// assert!(res.is_success());
    /// ```
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.status.is_ok()
    }

    /// Returns `true` if the write failed.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcError, WriteResult};
    ///
    /// let res = WriteResult::failure("Tag1", OpcError::Connection("Failed".into()));
    /// assert!(res.is_error());
    /// ```
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.status.is_err()
    }

    /// Returns the error if the write failed, or `None` if it succeeded.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcError, WriteResult};
    ///
    /// let res = WriteResult::failure("Tag1", OpcError::Connection("Failed".into()));
    /// if let Some(err) = res.error() {
    ///     assert_eq!(err.to_string(), "Connection failed: Failed");
    /// }
    /// ```
    #[must_use]
    pub fn error(&self) -> Option<&OpcError> {
        self.status.as_ref().err()
    }
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
    /// ```rust,no_run
    /// use opc_da_client::{OpcDaClient, OpcProvider, OpcResult};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> OpcResult<()> {
    /// let client = OpcDaClient::default();
    /// let servers = client.list_servers("localhost").await?;
    /// assert!(servers.is_empty() || !servers.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    async fn list_servers(&self, host: &str) -> OpcResult<Vec<String>>;

    /// Browse tags recursively, pushing discoveries to `tags_sink`.
    ///
    /// # Arguments
    /// * `server` - ProgID of the OPC server.
    /// * `max_tags` - Maximum number of tags to discover before terminating.
    /// * `progress` - Atomic counter updated with each discovered tag count.
    /// * `tags_sink` - Shared mutex-protected vector storing discovered tag names.
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
    /// ```rust,no_run
    /// use opc_da_client::{OpcDaClient, OpcProvider, OpcResult};
    /// use std::sync::{Arc, Mutex, atomic::AtomicUsize};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> OpcResult<()> {
    /// let client = OpcDaClient::default();
    /// let sink = Arc::new(Mutex::new(Vec::new()));
    /// let progress = Arc::new(AtomicUsize::new(0));
    /// let tags = client.browse_tags("Matrikon.OPC.Simulation.1", 100, progress, sink).await?;
    /// assert!(tags.len() <= 100);
    /// # Ok(())
    /// # }
    /// ```
    async fn browse_tags(
        &self,
        server: &str,
        max_tags: usize,
        progress: Arc<AtomicUsize>,
        tags_sink: Arc<std::sync::Mutex<Vec<String>>>,
    ) -> OpcResult<Vec<String>>;

    /// Read current values for the given tag IDs.
    ///
    /// # Arguments
    /// * `server` - ProgID of the OPC server.
    /// * `tag_ids` - List of fully qualified tag identifiers to read.
    ///
    /// # Returns
    /// A vector of [`TagValue`] items preserving input tag order.
    ///
    /// # Errors
    /// Returns [`crate::errors::OpcError`] if the server connection fails, no items
    /// can be added to the OPC group, or the synchronous read operation fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use opc_da_client::{OpcDaClient, OpcProvider, OpcResult, TagValue};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> OpcResult<()> {
    /// let client = OpcDaClient::default();
    /// let tags = vec!["Random.Int4".to_string(), "Random.Real8".to_string()];
    /// let values = client.read_tag_values("Matrikon.OPC.Simulation.1", tags).await?;
    /// for v in &values {
    ///     let _val = v.display_value();
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn read_tag_values(&self, server: &str, tag_ids: Vec<String>)
    -> OpcResult<Vec<TagValue>>;

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
    /// ```rust,no_run
    /// use opc_da_client::{OpcDaClient, OpcProvider, OpcResult, OpcValue, WriteResult};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> OpcResult<()> {
    /// let client = OpcDaClient::default();
    /// let result = client
    ///     .write_tag_value("Matrikon.OPC.Simulation.1", "Bucket Brigade.Int4", OpcValue::Int(42))
    ///     .await?;
    /// let _success = result.is_success();
    /// # Ok(())
    /// # }
    /// ```
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
        // Non-epoch time
        let ts = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        let ts_opt = Some(ts);
        let expected = crate::helpers::system_time_to_string(ts);
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
            value: Some(OpcValue::Float(99.5)),
            quality: OpcQuality::GOOD,
            timestamp: Some(SystemTime::UNIX_EPOCH),
        };
        assert_eq!(format!("{tv}"), "Simulation.Item1 = 99.5 [Good] @ N/A");
    }

    #[test]
    fn test_tag_value_destructuring_ergonomics() {
        let tv = TagValue {
            tag_id: "Device1.Tag1".to_string(),
            value: Some(OpcValue::String("Active".into())),
            quality: OpcQuality::GOOD,
            timestamp: None,
        };

        // Exact pattern destructuring
        let TagValue {
            tag_id,
            value,
            quality,
            timestamp,
        } = tv;

        let formatted = format!(
            "Tag: {:<15} | Value: {:<10} | Quality: {:<6} | Timestamp: {}",
            tag_id,
            value.display(),
            quality,
            timestamp.display_or("N/A")
        );

        assert_eq!(
            formatted,
            "Tag: Device1.Tag1    | Value: Active     | Quality: Good   | Timestamp: N/A"
        );
    }
}
