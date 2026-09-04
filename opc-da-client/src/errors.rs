//! Error types and HRESULT formatting utilities for OPC DA operations.

use thiserror::Error;
use windows::core::HRESULT;

/// Result type alias for OPC DA operations.
///
/// # Examples
///
/// ```
/// use opc_da_client::OpcResult;
///
/// fn check_health() -> OpcResult<()> {
///     Ok(())
/// }
/// assert!(check_health().is_ok());
/// ```
pub type OpcResult<T> = Result<T, OpcError>;

/// Centralized error enum for the OPC DA client.
///
/// # Examples
///
/// ```
/// use opc_da_client::OpcError;
///
/// let err = OpcError::Connection("Server unreachable".to_string());
/// assert_eq!(err.to_string(), "Connection failed: Server unreachable");
/// ```
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]
pub enum OpcError {
    /// Standard Windows COM/DCOM error.
    ///
    /// This variant wraps a [`windows::core::Error`] and provides a friendly
    /// hint for common OPC-related HRESULT codes.
    #[error("COM error: {source} ({})", friendly_hresult_hint(.source.code()).unwrap_or("No hint available"))]
    Com {
        #[from]
        source: windows::core::Error,
    },

    /// Connection-related errors (e.g., host unreachable, resolution failure).
    #[error("Connection failed: {0}")]
    Connection(String),

    /// Server-specific errors reported via OPC status codes.
    #[error("Server error: {0} (0x{1:08X})")]
    Server(String, u32),

    /// Errors during data type conversion or VARIANT processing.
    #[error("Data conversion failed: {0}")]
    Conversion(String),

    /// Operation attempted in an invalid state (e.g., group already exists).
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// Feature not implemented or supported by the target OPC server.
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Catch-all for unexpected internal failures.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<tokio::task::JoinError> for OpcError {
    fn from(err: tokio::task::JoinError) -> Self {
        Self::Internal(format!("Async task join failed: {err}"))
    }
}

impl From<std::num::TryFromIntError> for OpcError {
    fn from(err: std::num::TryFromIntError) -> Self {
        Self::Conversion(format!("Integer conversion error: {err}"))
    }
}

/// Helper to format HRESULT with friendly hints.
///
/// # Examples
///
/// ```
/// use opc_da_client::format_hresult;
/// use windows::core::HRESULT;
///
/// let formatted = format_hresult(HRESULT(0x8000_4003u32 as i32));
/// assert!(formatted.contains("0x80004003"));
/// assert!(formatted.contains("Invalid pointer"));
/// ```
pub fn format_hresult(hr: HRESULT) -> String {
    let hex = format!("0x{:08X}", hr.0.cast_unsigned());
    match friendly_hresult_hint(hr) {
        Some(hint) => format!("{hex}: {hint}"),
        None => hex,
    }
}

/// Maps known COM/DCOM error codes to actionable user hints.
///
/// # Examples
///
/// ```
/// use opc_da_client::errors::friendly_hresult_hint;
/// use windows::core::HRESULT;
///
/// let hint = friendly_hresult_hint(HRESULT(0x8004_0154u32 as i32));
/// assert_eq!(hint, Some("Server is not registered on this machine"));
/// ```
pub fn friendly_hresult_hint(hr: HRESULT) -> Option<&'static str> {
    match hr.0.cast_unsigned() {
        0x8004_0112 => Some("Server license does not permit OPC client connections"),
        0x8008_0005 => {
            Some("Server process failed to start — check if it is installed and running")
        }
        0x8007_0005 => {
            Some("Access denied — DCOM launch/activation permissions not configured for this user")
        }
        0x8007_06BA => {
            Some("RPC server unavailable — the target host may be offline or blocking RPC")
        }
        0x8007_06F4 => Some("COM marshalling error — try restarting the OPC server"),
        0x8004_0154 => Some("Server is not registered on this machine"),
        0x8000_4003 => Some("Invalid pointer (E_POINTER)"),
        0xC004_0004 => Some("Server rejected write — the item may be read-only (OPC_E_BADRIGHTS)"),
        0xC004_0006 => {
            Some("Data type mismatch — server cannot convert the written value (OPC_E_BADTYPE)")
        }
        0xC004_0007 => Some("Item ID not found in server address space (OPC_E_UNKNOWNITEMID)"),
        0xC004_0008 => Some("Item ID syntax is invalid for this server (OPC_E_INVALIDITEMID)"),
        _ => None,
    }
}

/// Maps an [`OpcError`] to a friendly COM hint if it is a COM error.
///
/// # Examples
///
/// ```
/// use opc_da_client::{friendly_com_hint, OpcError};
///
/// let err = OpcError::Connection("host unreachable".into());
/// assert_eq!(friendly_com_hint(&err), None);
/// ```
pub fn friendly_com_hint(error: &OpcError) -> Option<&'static str> {
    match error {
        OpcError::Com { source: e } => friendly_hresult_hint(e.code()),
        _ => None,
    }
}

/// Emits a structured `tracing::error!` event with machine-parseable fields.
///
/// Extracts the HRESULT code and friendly hint from an [`OpcError`],
/// and logs them as named fields for aggregation by log analysis tools.
///
/// # Arguments
/// * `error` - The OPC error to log
/// * `operation` - Name of the operation that failed (e.g., "read_tag_values")
pub fn log_opc_error(error: &OpcError, operation: &str) {
    let hresult = match error {
        OpcError::Com { source: e } => Some(format!("0x{:08X}", e.code().0.cast_unsigned())),
        _ => None,
    };
    let hint = friendly_com_hint(error);
    let chain = format!("{error:#}");

    tracing::error!(
        operation = %operation,
        hresult = hresult.as_deref().unwrap_or("N/A"),
        hint = hint.unwrap_or("none"),
        chain = %chain,
        "OPC operation failed"
    );
}
