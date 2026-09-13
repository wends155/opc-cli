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

    /// The request channel was closed because the worker thread stopped.
    #[error("COM worker channel closed (worker stopped)")]
    RequestChannelClosed,

    /// The response channel was dropped before receiving a reply.
    #[error("COM worker shut down during request")]
    ResponseChannelClosed,

    /// The synchronous init channel disconnected before startup completed.
    #[error("COM worker init channel disconnected: {0}")]
    InitChannelDisconnected(String),

    /// An asynchronous task failed to join.
    #[error("Async task join failed: {0}")]
    TaskJoin(String),

    /// A synchronization mutex or lock was poisoned by a panic.
    #[error("Synchronization lock poisoned: {0}")]
    LockPoisoned(String),
}

impl WorkerError {
    /// Returns `true` if the worker error represents a broken connection or crashed worker.
    #[must_use]
    pub fn is_connection_error(&self) -> bool {
        matches!(
            self,
            Self::ResponseChannelClosed | Self::RequestChannelClosed | Self::Panic(_)
        )
    }

    /// Returns an actionable diagnostic hint for the worker error.
    #[must_use]
    pub fn friendly_hint(&self) -> Option<&'static str> {
        match self {
            Self::Panic(_) => Some(
                "The background COM worker thread crashed unexpectedly; inspect previous log lines for panic payload",
            ),
            Self::RequestChannelClosed | Self::ResponseChannelClosed => {
                Some("The background COM worker terminated; reconnect to restore service")
            }
            Self::InitializationFailed(_) | Self::InitChannelDisconnected(_) => Some(
                "Failed to initialize COM runtime or thread apartment for the background worker",
            ),
            Self::LockPoisoned(_) => {
                Some("An internal concurrency lock was poisoned by a previous panic")
            }
            Self::TaskJoin(_) => Some("A spawned background asynchronous task failed to join"),
        }
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for WorkerError {
    fn from(_: tokio::sync::oneshot::error::RecvError) -> Self {
        Self::ResponseChannelClosed
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for WorkerError {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Self::RequestChannelClosed
    }
}

impl From<std::sync::mpsc::RecvError> for WorkerError {
    fn from(err: std::sync::mpsc::RecvError) -> Self {
        Self::InitChannelDisconnected(err.to_string())
    }
}

impl<T> From<std::sync::PoisonError<T>> for WorkerError {
    fn from(err: std::sync::PoisonError<T>) -> Self {
        Self::LockPoisoned(err.to_string())
    }
}

impl From<tokio::task::JoinError> for WorkerError {
    fn from(err: tokio::task::JoinError) -> Self {
        Self::TaskJoin(err.to_string())
    }
}
