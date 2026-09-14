//! Error types for OPC DA operations.

pub mod conversion;
pub mod hresult;
pub mod worker;

pub use self::conversion::ConversionError;
pub use self::worker::WorkerError;

use std::time::Duration;
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
    /// This variant wraps a [`windows_core::Error`] and provides a friendly
    /// hint for common OPC-related HRESULT codes.
    #[error("COM error: {source}{}", format_com_hint(.source))]
    Com {
        /// The underlying Windows COM/DCOM error.
        #[from]
        source: windows_core::Error,
    },

    /// Connection-related errors (e.g., host unreachable, resolution failure).
    #[error("Connection failed: {0}")]
    Connection(String),

    /// Server-specific errors reported via OPC status codes.
    #[error("Server error: {0} (0x{1:08X})")]
    Server(String, u32),

    /// Errors during data type conversion or VARIANT processing.
    #[error("Conversion error: {0}")]
    Conversion(#[from] ConversionError),

    /// Worker thread or concurrency synchronization failure.
    #[error("Worker error: {0}")]
    Worker(#[from] WorkerError),

    /// Operation attempted in an invalid state (e.g., group already exists).
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// Feature not implemented or supported by the target OPC server.
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Operation timed out.
    #[error("Operation timed out after {0:?}")]
    Timeout(Duration),

    /// Tag was not requested in the read batch.
    #[error("Tag '{0}' was not requested in this read batch")]
    TagNotRequested(String),

    /// Tag value was null or missing in the read batch.
    #[error("Tag '{0}' returned no value (null or missing)")]
    TagNoValue(String),

    /// Catch-all for unexpected internal failures.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<std::num::TryFromIntError> for OpcError {
    fn from(err: std::num::TryFromIntError) -> Self {
        Self::Conversion(ConversionError::IntConversion(err))
    }
}

impl From<windows_core::HRESULT> for OpcError {
    fn from(hr: windows_core::HRESULT) -> Self {
        Self::Com {
            source: windows_core::Error::from_hresult(hr),
        }
    }
}

fn format_com_hint(source: &windows_core::Error) -> String {
    if let Some(hint) = self::hresult::friendly_hresult_hint(source.code()) {
        format!(" ({hint})")
    } else {
        String::new()
    }
}

impl OpcError {
    /// Returns `true` if this error indicates a connection failure, network timeout,
    /// or dropped COM RPC connection that warrants server reconnection or pool eviction.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::OpcError;
    ///
    /// let err = OpcError::Connection("server unreachable".into());
    /// assert!(err.is_connection_error());
    ///
    /// let err = OpcError::InvalidState("already open".into());
    /// assert!(!err.is_connection_error());
    /// ```
    #[must_use]
    pub fn is_connection_error(&self) -> bool {
        match self {
            Self::Connection(_) | Self::Timeout(_) => true,
            Self::Com { source } => self::hresult::is_connection_hresult(source.code()),
            Self::Worker(w) => w.is_connection_error(),
            _ => false,
        }
    }

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
        match self {
            Self::Com { source } => self::hresult::friendly_hresult_hint(source.code()),
            Self::Server(_, code) => {
                self::hresult::friendly_hresult_hint(windows_core::HRESULT((*code).cast_signed()))
            }
            Self::Worker(w) => w.friendly_hint(),
            _ => None,
        }
    }

    /// Creates a new `Conversion` error from any type converting into `ConversionError`.
    #[must_use]
    pub fn conversion(err: impl Into<ConversionError>) -> Self {
        Self::Conversion(err.into())
    }

    /// Constructs an [`OpcError::Connection`] indicating failure to resolve a server name to a CLSID.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::OpcError;
    ///
    /// let err = OpcError::connection_failed("Matrikon.OPC", "invalid ProgID");
    /// assert!(matches!(err, OpcError::Connection(_)));
    /// ```
    #[inline]
    #[must_use]
    pub fn connection_failed(server: impl std::fmt::Display, err: impl std::fmt::Display) -> Self {
        Self::Connection(format!(
            "Failed to resolve ProgID '{server}' to CLSID: {err}"
        ))
    }

    /// Extracts the raw unsigned 32-bit error code from [`OpcError::Com`] or [`OpcError::Server`].
    ///
    /// Returns `Some(code)` if the error contains an underlying COM HRESULT or OPC server
    /// error code, or `None` if the error variant does not carry a numeric status code.
    #[must_use]
    pub fn raw_code(&self) -> Option<u32> {
        match self {
            Self::Com { source } => Some(source.code().0.cast_unsigned()),
            Self::Server(_, code) => Some(*code),
            _ => None,
        }
    }
}

/// Helper adapter for formatting optional 32-bit error codes without heap allocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub struct DisplayRawCode(pub Option<u32>);

impl std::fmt::Display for DisplayRawCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(code) => write!(f, "0x{code:08X}"),
            None => write!(f, "N/A"),
        }
    }
}

/// Emits a structured `tracing::error!` event capturing HRESULT, hint, chain, and optional key-value fields.
#[macro_export]
macro_rules! log_opc_err {
    ($err:expr, $op:expr) => {{
        $crate::log_opc_err!($err, $op,)
    }};
    ($err:expr, $op:expr, $($field:tt)*) => {{
        let error = &$err;
        let operation = &$op;
        let raw_code = $crate::errors::DisplayRawCode(error.raw_code());
        let hint = error.friendly_hint();

        tracing::error!(
            operation = %operation,
            hresult = %raw_code,
            hint = hint.unwrap_or("none"),
            chain = %format_args!("{error:#}"),
            $($field)*
        );
    }};
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
                source: windows_core::Error::from_hresult(windows_core::HRESULT(
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
    fn test_opc_error_raw_code_extraction() {
        let server_err = OpcError::Server("OPC_E_NOTFOUND".into(), 0x8004_0200);
        assert_eq!(server_err.raw_code(), Some(0x8004_0200));

        let conn_err = OpcError::Connection("failed".into());
        assert_eq!(conn_err.raw_code(), None);

        #[cfg(feature = "opc-da-backend")]
        {
            use crate::errors::hresult::RPC_S_SERVER_UNAVAILABLE;
            let com_err = OpcError::Com {
                source: windows_core::Error::from_hresult(RPC_S_SERVER_UNAVAILABLE),
            };
            assert_eq!(com_err.raw_code(), Some(0x8007_06BA));
        }
    }

    #[test]
    fn test_tag_not_requested_and_no_value_variants() {
        let err1 = OpcError::TagNotRequested("Tag1".into());
        assert_eq!(
            err1.to_string(),
            "Tag 'Tag1' was not requested in this read batch"
        );
        let err2 = OpcError::TagNoValue("Tag2".into());
        assert_eq!(
            err2.to_string(),
            "Tag 'Tag2' returned no value (null or missing)"
        );
    }

    #[test]
    #[cfg(feature = "opc-da-backend")]
    fn test_friendly_hint_known_codes() {
        let err = OpcError::Com {
            source: windows_core::Error::from_hresult(windows_core::HRESULT(
                0x8007_06F4_u32.cast_signed(),
            )),
        };
        assert_eq!(
            err.friendly_hint(),
            Some("COM marshalling error — try restarting the OPC server")
        );

        let err = OpcError::Com {
            source: windows_core::Error::from_hresult(windows_core::HRESULT(
                0x8004_0154_u32.cast_signed(),
            )),
        };
        assert_eq!(
            err.friendly_hint(),
            Some("Server is not registered on this machine")
        );

        let err = OpcError::Com {
            source: windows_core::Error::from_hresult(windows_core::HRESULT(
                0xC004_0004_u32.cast_signed(),
            )),
        };
        assert_eq!(
            err.friendly_hint(),
            Some("Server rejected write — the item may be read-only (OPC_E_BADRIGHTS)"),
        );

        let err = OpcError::Com {
            source: windows_core::Error::from_hresult(windows_core::HRESULT(
                0xC004_0006_u32.cast_signed(),
            )),
        };
        assert_eq!(
            err.friendly_hint(),
            Some("Data type mismatch — server cannot convert the written value (OPC_E_BADTYPE)"),
        );

        let err = OpcError::Com {
            source: windows_core::Error::from_hresult(windows_core::HRESULT(
                0xC004_0007_u32.cast_signed(),
            )),
        };
        assert_eq!(
            err.friendly_hint(),
            Some("Item ID not found in server address space (OPC_E_UNKNOWNITEMID)"),
        );

        let err = OpcError::Com {
            source: windows_core::Error::from_hresult(windows_core::HRESULT(
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

    #[test]
    fn test_log_opc_err_macro() {
        let err = OpcError::Connection("server unreachable".into());
        log_opc_err!(&err, "connect", server = "Matrikon.OPC.Simulation.1");
        log_opc_err!(
            &err,
            "read_tag_values:add_items",
            server = "Matrikon.OPC.Simulation.1",
            tag_count = 5
        );
    }

    #[test]
    fn test_log_opc_err_macro_borrowing_no_move() {
        let err = OpcError::Connection("server unreachable".into());
        // Must not move err; err must be usable after macro invocation
        log_opc_err!(&err, "connect", server = "Matrikon.OPC.Simulation.1");
        log_opc_err!(err, "connect", server = "Matrikon.OPC.Simulation.1");
    }

    #[test]
    fn test_connection_failed() {
        let conn_err = OpcError::connection_failed("Matrikon.OPC", "invalid CLSID");
        assert!(matches!(conn_err, OpcError::Connection(msg) if msg.contains("Matrikon.OPC")));
    }

    #[test]
    fn test_is_connection_error() {
        let conn_err = OpcError::Connection("connection lost".into());
        assert!(conn_err.is_connection_error());

        let state_err = OpcError::InvalidState("bad state".into());
        assert!(!state_err.is_connection_error());

        #[cfg(feature = "opc-da-backend")]
        {
            use crate::errors::hresult::{E_POINTER, RPC_S_SERVER_UNAVAILABLE};
            let rpc_err = OpcError::Com {
                source: windows_core::Error::from_hresult(RPC_S_SERVER_UNAVAILABLE),
            };
            assert!(rpc_err.is_connection_error());

            let pointer_err = OpcError::Com {
                source: windows_core::Error::from_hresult(E_POINTER),
            };
            assert!(!pointer_err.is_connection_error());
        }
    }

    #[test]
    fn test_com_error_display_formatting() {
        #[cfg(feature = "opc-da-backend")]
        {
            use crate::errors::hresult::{E_POINTER, RPC_S_SERVER_UNAVAILABLE};
            let rpc_err = OpcError::Com {
                source: windows_core::Error::from_hresult(RPC_S_SERVER_UNAVAILABLE),
            };
            let formatted = rpc_err.to_string();
            assert!(formatted.contains("The RPC server is unavailable"));
            assert!(!formatted.contains("No hint available"));

            let pointer_err = OpcError::Com {
                source: windows_core::Error::from_hresult(E_POINTER),
            };
            let formatted_ptr = pointer_err.to_string();
            assert!(!formatted_ptr.contains("No hint available"));
            assert!(!formatted_ptr.ends_with(" ()"));
        }
    }

    #[test]
    fn test_timeout_error_and_conversions() {
        let timeout_err = OpcError::Timeout(Duration::from_secs(5));
        assert!(timeout_err.is_connection_error());
        assert_eq!(timeout_err.to_string(), "Operation timed out after 5s");

        let hr = windows_core::HRESULT(0x8000_4005_u32.cast_signed()); // E_FAIL
        let com_err: OpcError = hr.into();
        assert!(matches!(com_err, OpcError::Com { .. }));
    }

    #[test]
    fn test_opcerror_clone_partialeq() {
        let err = OpcError::Internal("test".into());
        let cloned = err.clone();
        assert_eq!(err, cloned);

        let conv_err = OpcError::Conversion(crate::errors::ConversionError::InvalidBrowseType(1));
        assert_ne!(err, conv_err);
    }

    #[test]
    fn test_worker_error_taxonomy() {
        use crate::errors::worker::WorkerError;
        let panic_err = WorkerError::Panic("simulated thread crash".into());
        assert!(panic_err.is_connection_error());
        assert!(panic_err.friendly_hint().is_some());

        let term_err = WorkerError::WorkerTerminated;
        assert!(term_err.is_connection_error());
        assert!(term_err.friendly_hint().is_some());

        let init_err = WorkerError::InitializationFailed("E_FAIL".into());
        assert!(!init_err.is_connection_error());
        assert!(init_err.friendly_hint().is_some());

        let opc_err: OpcError = panic_err.into();
        assert!(opc_err.is_connection_error());
        assert_eq!(
            opc_err.friendly_hint(),
            WorkerError::Panic(String::new()).friendly_hint()
        );
    }

    #[test]
    fn test_conversion_error_taxonomy() {
        use crate::errors::conversion::ConversionError;
        let c_err = ConversionError::InvalidBrowseType(99);
        assert_eq!(c_err.to_string(), "Invalid browse type discriminant: 99");

        let opc_err: OpcError = c_err.into();
        assert!(matches!(
            opc_err,
            OpcError::Conversion(ConversionError::InvalidBrowseType(99))
        ));

        let endpoint_err = ConversionError::InvalidEndpoint("opc://bad uri".into());
        assert_eq!(
            endpoint_err.to_string(),
            "Invalid server endpoint: opc://bad uri"
        );
    }
}
