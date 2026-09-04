//! Error types for OPC DA operations.

use thiserror::Error;

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
    #[error("COM error: {source} ({})", com_error_hint(.source))]
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

fn com_error_hint(source: &windows::core::Error) -> &'static str {
    #[cfg(feature = "opc-da-backend")]
    {
        crate::raw::hresult::friendly_hresult_hint(source.code()).unwrap_or("No hint available")
    }
    #[cfg(not(feature = "opc-da-backend"))]
    {
        let _ = source;
        "No hint available"
    }
}

impl OpcError {
    /// Returns an actionable user-friendly hint if this error is caused by a known COM or OPC failure.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::OpcError;
    ///
    /// let err = OpcError::Connection("host unreachable".into());
    /// assert_eq!(err.friendly_hint(), None);
    /// ```
    #[must_use]
    pub fn friendly_hint(&self) -> Option<&'static str> {
        #[cfg(feature = "opc-da-backend")]
        {
            match self {
                Self::Com { source } => crate::raw::hresult::friendly_hresult_hint(source.code()),
                Self::Server(_, code) => crate::raw::hresult::friendly_hresult_hint(
                    windows::core::HRESULT((*code).cast_signed()),
                ),
                _ => None,
            }
        }
        #[cfg(not(feature = "opc-da-backend"))]
        {
            None
        }
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
#[allow(dead_code)]
pub(crate) fn log_opc_error(error: &OpcError, operation: &str) {
    let hresult = match error {
        OpcError::Com { source: e } => Some(format!("0x{:08X}", e.code().0.cast_unsigned())),
        _ => None,
    };
    let hint = error.friendly_hint();
    let chain = format!("{error:#}");

    tracing::error!(
        operation = %operation,
        hresult = hresult.as_deref().unwrap_or("N/A"),
        hint = hint.unwrap_or("none"),
        chain = %chain,
        "OPC operation failed"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opc_error_friendly_hint() {
        let err = OpcError::Connection("host unreachable".into());
        assert_eq!(err.friendly_hint(), None);

        #[cfg(feature = "opc-da-backend")]
        {
            let com_err = OpcError::Com {
                source: windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x8004_0154_u32.cast_signed(),
                )),
            };
            assert_eq!(
                com_err.friendly_hint(),
                Some("Server is not registered on this machine")
            );
        }
    }

    #[test]
    #[cfg(feature = "opc-da-backend")]
    fn test_friendly_hint_known_codes() {
        let err = OpcError::Com {
            source: windows::core::Error::from_hresult(windows::core::HRESULT(
                0x8007_06F4_u32.cast_signed(),
            )),
        };
        assert_eq!(
            err.friendly_hint(),
            Some("COM marshalling error — try restarting the OPC server")
        );

        let err = OpcError::Com {
            source: windows::core::Error::from_hresult(windows::core::HRESULT(
                0x8004_0154_u32.cast_signed(),
            )),
        };
        assert_eq!(
            err.friendly_hint(),
            Some("Server is not registered on this machine")
        );

        let err = OpcError::Com {
            source: windows::core::Error::from_hresult(windows::core::HRESULT(
                0xC004_0004_u32.cast_signed(),
            )),
        };
        assert_eq!(
            err.friendly_hint(),
            Some("Server rejected write — the item may be read-only (OPC_E_BADRIGHTS)"),
        );

        let err = OpcError::Com {
            source: windows::core::Error::from_hresult(windows::core::HRESULT(
                0xC004_0006_u32.cast_signed(),
            )),
        };
        assert_eq!(
            err.friendly_hint(),
            Some("Data type mismatch — server cannot convert the written value (OPC_E_BADTYPE)"),
        );

        let err = OpcError::Com {
            source: windows::core::Error::from_hresult(windows::core::HRESULT(
                0xC004_0007_u32.cast_signed(),
            )),
        };
        assert_eq!(
            err.friendly_hint(),
            Some("Item ID not found in server address space (OPC_E_UNKNOWNITEMID)"),
        );

        let err = OpcError::Com {
            source: windows::core::Error::from_hresult(windows::core::HRESULT(
                0xC004_0008_u32.cast_signed(),
            )),
        };
        assert_eq!(
            err.friendly_hint(),
            Some("Item ID syntax is invalid for this server (OPC_E_INVALIDITEMID)"),
        );
    }

    #[test]
    fn test_friendly_hint_unknown_code() {
        let err = OpcError::Internal("Some other error".to_string());
        assert_eq!(err.friendly_hint(), None);
    }
}
