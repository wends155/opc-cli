//! Write results and tag collection accumulators.

use crate::errors::OpcError;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
    #[must_use]
    pub fn success(tag_id: impl Into<String>) -> Self {
        Self {
            tag_id: tag_id.into(),
            status: Ok(()),
        }
    }

    /// Creates a failed write result with a domain error.
    #[must_use]
    pub fn failure(tag_id: impl Into<String>, error: OpcError) -> Self {
        Self {
            tag_id: tag_id.into(),
            status: Err(error),
        }
    }

    /// Returns `true` if the write succeeded.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.status.is_ok()
    }

    /// Returns `true` if the write failed.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.status.is_err()
    }

    /// Returns the error if the write failed, or `None` if it succeeded.
    #[must_use]
    pub fn error(&self) -> Option<&OpcError> {
        self.status.as_ref().err()
    }
}

/// Thread-safe accumulator, capacity limiter, and progress monitor for OPC tag namespace browsing.
///
/// `TagCollector` encapsulates incremental tag accumulation, explicit `max_tags` bounding,
/// lock-free progress tracking, and cooperative cancellation across thread and async boundaries.
#[derive(Debug, Clone)]
pub struct TagCollector {
    inner: Arc<TagCollectorInner>,
}

#[derive(Debug)]
struct TagCollectorInner {
    tags: std::sync::Mutex<Vec<String>>,
    count: AtomicUsize,
    max_tags: usize,
    cancelled: AtomicBool,
}

impl TagCollector {
    /// Standard default capacity cap when unconstrained (10,000 tags).
    pub const DEFAULT_MAX_TAGS: usize = 10_000;

    /// Creates a new `TagCollector` bounded to at most `max_tags` items.
    #[must_use]
    pub fn new(max_tags: usize) -> Self {
        Self {
            inner: Arc::new(TagCollectorInner {
                tags: std::sync::Mutex::new(Vec::with_capacity(max_tags.min(1024))),
                count: AtomicUsize::new(0),
                max_tags,
                cancelled: AtomicBool::new(false),
            }),
        }
    }

    /// Creates an unbounded `TagCollector` (`max_tags = usize::MAX`).
    #[must_use]
    pub fn unbounded() -> Self {
        Self::new(usize::MAX)
    }

    /// Returns the maximum capacity cap.
    #[must_use]
    pub fn max_tags(&self) -> usize {
        self.inner.max_tags
    }

    /// Returns the number of tags collected so far without acquiring a mutex lock.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.count.load(Ordering::Acquire)
    }

    /// Returns true if no tags have been collected yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if the collector has reached or exceeded its `max_tags` capacity cap.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.len() >= self.inner.max_tags
    }

    /// Signals cancellation to the background browse worker.
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::Release);
    }

    /// Returns true if cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    /// Returns a cloned snapshot of all tags collected so far without draining.
    #[must_use]
    pub fn snapshot(&self) -> Vec<String> {
        let guard = match self.inner.tags.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.clone()
    }

    /// Drains and returns all collected tags, resetting the buffer and atomic count.
    #[must_use]
    pub fn harvest(&self) -> Vec<String> {
        let mut guard = match self.inner.tags.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let harvested = std::mem::take(&mut *guard);
        drop(guard);
        self.inner.count.store(0, Ordering::Release);
        harvested
    }

    /// Pushes a tag into the collector if not full or cancelled.
    #[must_use = "Returns false if the collector is full or cancelled"]
    pub fn push(&self, tag: String) -> bool {
        if self.is_cancelled() || self.is_full() {
            return false;
        }
        let mut guard = match self.inner.tags.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if guard.len() >= self.inner.max_tags {
            return false;
        }
        guard.push(tag);
        drop(guard);
        self.inner.count.fetch_add(1, Ordering::Release);
        true
    }
}

impl Default for TagCollector {
    /// Creates a default `TagCollector` with `DEFAULT_MAX_TAGS` capacity.
    fn default() -> Self {
        Self::new(Self::DEFAULT_MAX_TAGS)
    }
}
