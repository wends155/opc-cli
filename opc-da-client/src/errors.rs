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

impl From<std::sync::mpsc::RecvError> for OpcError {
    fn from(err: std::sync::mpsc::RecvError) -> Self {
        Self::Internal(format!("COM worker init channel disconnected: {err}"))
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for OpcError {
    fn from(err: tokio::sync::oneshot::error::RecvError) -> Self {
        Self::Internal(format!("COM worker shut down during request: {err}"))
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for OpcError {
    fn from(err: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Self::Internal(format!("COM worker channel closed (worker stopped): {err}"))
    }
}

impl<T> From<std::sync::PoisonError<T>> for OpcError {
    fn from(err: std::sync::PoisonError<T>) -> Self {
        Self::Internal(format!("Synchronization lock poisoned: {err}"))
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
}

/// Canonical operation identifiers for OPC DA interactions.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum OpcOperation {
    ListServers,
    Connect,
    DispatchConnectionError,
    DispatchReconnect,
    DispatchRetriedOperation,
    DispatchOperation,
    ReadAddGroup,
    ReadAddItems,
    ReadMismatchedResults,
    ReadSync,
    ReadPerItem,
    WriteAddGroup,
    WriteAddItems,
    WriteEmptyItemResults,
    WriteAddItemsRejected,
    WriteSync,
    WriteEmptyWriteErrors,
    WriteServerRejected,
    BrowseQueryOrganization,
    BrowseFlatLeaves,
    BrowseFlatLeafItem,
    BrowseFlatEnumItem,
    BrowseRecursiveBranches,
    BrowseRecursiveBranchItem,
    BrowseRecursiveLeaves,
    BrowseRecursiveLeafItem,
    BrowseRecursiveGetItemId,
    BrowseRecursiveChangePositionDown,
    BrowseRecursiveChildBranch,
    BrowseRecursiveChangePositionUp,
}

impl std::fmt::Display for OpcOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let op_str = match self {
            Self::ListServers => "list_servers",
            Self::Connect => "connect",
            Self::DispatchConnectionError => "dispatch_with_retry:connection_error",
            Self::DispatchReconnect => "dispatch_with_retry:reconnect",
            Self::DispatchRetriedOperation => "dispatch_with_retry:retried_operation",
            Self::DispatchOperation => "dispatch_with_retry:operation",
            Self::ReadAddGroup => "read_tag_values:add_group",
            Self::ReadAddItems => "read_tag_values:add_items",
            Self::ReadMismatchedResults => "read_tag_values:mismatched_results",
            Self::ReadSync => "read_tag_values:group_read",
            Self::ReadPerItem => "read_tag_values:per_item_read",
            Self::WriteAddGroup => "write_tag_value:add_group",
            Self::WriteAddItems => "write_tag_value:add_items",
            Self::WriteEmptyItemResults => "write_tag_value:empty_item_results",
            Self::WriteAddItemsRejected => "write_tag_value:add_items_rejected",
            Self::WriteSync => "write_tag_value:group_write",
            Self::WriteEmptyWriteErrors => "write_tag_value:empty_write_errors",
            Self::WriteServerRejected => "write_tag_value:server_rejected",
            Self::BrowseQueryOrganization => "browse_tags:query_organization",
            Self::BrowseFlatLeaves => "browse_tags:flat_leaves",
            Self::BrowseFlatLeafItem => "browse_tags:flat_leaf_item",
            Self::BrowseFlatEnumItem => "browse_tags:flat_enum_item",
            Self::BrowseRecursiveBranches => "browse_recursive:branches",
            Self::BrowseRecursiveBranchItem => "browse_recursive:branch_item",
            Self::BrowseRecursiveLeaves => "browse_recursive:leaves",
            Self::BrowseRecursiveLeafItem => "browse_recursive:leaf_item",
            Self::BrowseRecursiveGetItemId => "browse_recursive:get_item_id",
            Self::BrowseRecursiveChangePositionDown => "browse_recursive:change_position_down",
            Self::BrowseRecursiveChildBranch => "browse_recursive:child_branch",
            Self::BrowseRecursiveChangePositionUp => "browse_recursive:change_position_up",
        };
        write!(f, "{op_str}")
    }
}

/// Emits a structured `tracing::error!` event capturing HRESULT, hint, chain, and optional key-value fields.
#[macro_export]
macro_rules! log_opc_err {
    ($err:expr, $op:expr) => {{
        $crate::log_opc_err!($err, $op,)
    }};
    ($err:expr, $op:expr, $($field:tt)*) => {{
        let error = $err;
        let operation = $op;
        let hresult = match error {
            $crate::errors::OpcError::Com { source: e } => {
                Some(format!("0x{:08X}", e.code().0.cast_unsigned()))
            }
            _ => None,
        };
        let hint = error.friendly_hint();
        let chain = format!("{error:#}");

        tracing::error!(
            operation = %operation,
            hresult = hresult.as_deref().unwrap_or("N/A"),
            hint = hint.unwrap_or("none"),
            chain = %chain,
            $($field)*
        );
    }};
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
    log_opc_err!(error, operation);
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

    #[test]
    fn test_opc_operation_display() {
        assert_eq!(
            OpcOperation::ReadAddGroup.to_string(),
            "read_tag_values:add_group"
        );
        assert_eq!(
            OpcOperation::WriteSync.to_string(),
            "write_tag_value:group_write"
        );
        assert_eq!(
            OpcOperation::BrowseRecursiveChangePositionDown.to_string(),
            "browse_recursive:change_position_down"
        );
        assert_eq!(
            OpcOperation::DispatchReconnect.to_string(),
            "dispatch_with_retry:reconnect"
        );
    }

    #[test]
    fn test_log_opc_err_macro() {
        let err = OpcError::Connection("server unreachable".into());
        log_opc_err!(
            &err,
            OpcOperation::Connect,
            server = "Matrikon.OPC.Simulation.1"
        );
        log_opc_err!(
            &err,
            OpcOperation::ReadAddItems,
            server = "Matrikon.OPC.Simulation.1",
            tag_count = 5
        );
    }

    #[test]
    fn test_channel_error_conversions_and_lock_poison() {
        let mpsc_err = std::sync::mpsc::RecvError;
        let opc_err: OpcError = mpsc_err.into();
        assert!(matches!(opc_err, OpcError::Internal(msg) if msg.contains("channel disconnected")));

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        drop(tx);
        let oneshot_err = rx.blocking_recv().unwrap_err();
        let opc_err: OpcError = oneshot_err.into();
        assert!(
            matches!(opc_err, OpcError::Internal(msg) if msg.contains("shut down during request"))
        );

        let (tx, rx) = tokio::sync::mpsc::channel::<()>(1);
        drop(rx);
        let send_err = tx.try_send(()).unwrap_err();
        if let tokio::sync::mpsc::error::TrySendError::Closed(e) = send_err {
            let opc_err: OpcError = tokio::sync::mpsc::error::SendError(e).into();
            assert!(matches!(opc_err, OpcError::Internal(msg) if msg.contains("channel closed")));
        }

        let lock = std::sync::Mutex::new(0);
        let opc_err: OpcError = std::sync::PoisonError::new(lock.lock().unwrap()).into();
        assert!(matches!(opc_err, OpcError::Internal(msg) if msg.contains("lock poisoned")));

        let conn_err = OpcError::connection_failed("Matrikon.OPC", "invalid CLSID");
        assert!(matches!(conn_err, OpcError::Connection(msg) if msg.contains("Matrikon.OPC")));
    }
}
