//! Worker thread lifecycle and channel synchronization errors.

/// Errors originating from the background COM worker thread or synchronization channels.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WorkerError {
    /// COM runtime or worker thread initialization failed.
    #[error("COM worker initialization failed: {0}")]
    InitializationFailed(String),

    /// The background COM worker thread crashed with a panic.
    #[error("COM worker thread panicked: {0}")]
    Panic(String),

    /// The background COM worker terminated unexpectedly or shutdown normally.
    #[error("COM worker terminated (channel closed or worker thread stopped)")]
    WorkerTerminated,
}

impl WorkerError {
    /// Returns `true` if the worker error represents a broken connection or crashed worker.
    #[must_use]
    pub fn is_connection_error(&self) -> bool {
        matches!(self, Self::WorkerTerminated | Self::Panic(_))
    }

    /// Returns an actionable diagnostic hint for the worker error.
    #[must_use]
    pub fn friendly_hint(&self) -> Option<&'static str> {
        match self {
            Self::Panic(_) => Some(
                "The background COM worker thread crashed unexpectedly; inspect previous log lines for panic payload",
            ),
            Self::WorkerTerminated => {
                Some("The background COM worker terminated; reconnect to restore service")
            }
            Self::InitializationFailed(_) => Some(
                "Failed to initialize COM runtime or thread apartment for the background worker",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_error_terminated_properties() {
        let err = WorkerError::WorkerTerminated;
        assert!(err.is_connection_error());
        assert_eq!(
            err.friendly_hint(),
            Some("The background COM worker terminated; reconnect to restore service")
        );
        assert_eq!(
            err.to_string(),
            "COM worker terminated (channel closed or worker thread stopped)"
        );
    }

    #[test]
    fn test_worker_error_initialization_failed_properties() {
        let err = WorkerError::InitializationFailed("E_FAIL".to_string());
        assert!(!err.is_connection_error());
        assert_eq!(
            err.friendly_hint(),
            Some("Failed to initialize COM runtime or thread apartment for the background worker")
        );
        assert_eq!(err.to_string(), "COM worker initialization failed: E_FAIL");
    }

    #[test]
    fn test_worker_error_panic_properties() {
        let err = WorkerError::Panic("thread crashed".to_string());
        assert!(err.is_connection_error());
        assert_eq!(
            err.friendly_hint(),
            Some(
                "The background COM worker thread crashed unexpectedly; inspect previous log lines for panic payload"
            )
        );
        assert_eq!(
            err.to_string(),
            "COM worker thread panicked: thread crashed"
        );
    }
}
