//! Tag collection accumulators and progress monitors.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
        self.inner.count.store(0, Ordering::Release);
        drop(guard);
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
        self.inner.count.fetch_add(1, Ordering::Release);
        drop(guard);
        true
    }

    /// Batch-inserts tags, acquiring the Mutex once for the entire batch.
    ///
    /// # Arguments
    ///
    /// * `tags` - An iterator of tag identifier strings to insert.
    ///
    /// # Returns
    ///
    /// The number of tags successfully accepted before capacity or cancellation was reached.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::new(10);
    /// let count = collector.push_batch(vec!["Tag1".into(), "Tag2".into()]);
    /// assert_eq!(count, 2);
    /// ```
    #[must_use = "Returns the number of tags successfully accepted"]
    pub fn push_batch(&self, tags: impl IntoIterator<Item = String>) -> usize {
        if self.is_cancelled() || self.is_full() {
            return 0;
        }
        let mut guard = match self.inner.tags.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let remaining = self.inner.max_tags.saturating_sub(guard.len());
        if remaining == 0 {
            return 0;
        }
        let mut count = 0;
        for tag in tags {
            if count >= remaining || self.is_cancelled() {
                break;
            }
            guard.push(tag);
            count += 1;
        }
        if count > 0 {
            self.inner.count.fetch_add(count, Ordering::Release);
        }
        drop(guard);
        count
    }
}

impl Default for TagCollector {
    /// Creates a default `TagCollector` with `DEFAULT_MAX_TAGS` capacity.
    fn default() -> Self {
        Self::new(Self::DEFAULT_MAX_TAGS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collector_push_batch() {
        let collector = TagCollector::new(5);
        let tags = vec!["Tag1".to_string(), "Tag2".to_string(), "Tag3".to_string()];
        let inserted = collector.push_batch(tags);
        assert_eq!(inserted, 3);
        assert_eq!(collector.len(), 3);
    }

    #[test]
    fn test_collector_push_batch_respects_capacity() {
        let collector = TagCollector::new(2);
        let tags = vec!["T1".to_string(), "T2".to_string(), "T3".to_string()];
        let inserted = collector.push_batch(tags);
        assert_eq!(inserted, 2);
        assert_eq!(collector.len(), 2);
    }

    #[test]
    fn test_collector_push_batch_after_cancel_inserts_nothing() {
        let collector = TagCollector::new(10);
        collector.cancel();
        let inserted = collector.push_batch(vec!["Tag1".to_string()]);
        assert_eq!(inserted, 0);
        assert_eq!(collector.len(), 0);
    }

    #[test]
    fn test_tag_collector_lifecycle() {
        let collector = TagCollector::new(5);
        assert_eq!(collector.len(), 0);
        assert!(collector.is_empty());
        assert_eq!(collector.max_tags(), 5);
        assert!(!collector.is_full());
        assert!(!collector.is_cancelled());

        assert!(collector.push("Tag1".into()));
        assert!(collector.push("Tag2".into()));
        assert_eq!(collector.len(), 2);
        assert!(!collector.is_empty());
        assert!(!collector.is_full());

        let snap = collector.snapshot();
        assert_eq!(snap, vec!["Tag1".to_string(), "Tag2".to_string()]);
        assert_eq!(collector.len(), 2);

        let harvested = collector.harvest();
        assert_eq!(harvested, vec!["Tag1".to_string(), "Tag2".to_string()]);
        assert_eq!(collector.len(), 0);
        assert!(collector.is_empty());
    }

    #[test]
    fn test_tag_collector_capacity_cap() {
        let collector = TagCollector::new(2);
        assert!(collector.push("T1".into()));
        assert!(collector.push("T2".into()));
        assert_eq!(collector.len(), 2);
        assert!(collector.is_full());

        // Further pushes must be rejected
        assert!(!collector.push("T3".into()));
        assert_eq!(collector.len(), 2);
        assert_eq!(
            collector.snapshot(),
            vec!["T1".to_string(), "T2".to_string()]
        );
    }

    #[test]
    fn test_tag_collector_unbounded() {
        let collector = TagCollector::unbounded();
        assert_eq!(collector.max_tags(), usize::MAX);
        assert!(!collector.is_full());
        assert!(collector.push("A".into()));
        assert!(!collector.is_full());
    }

    #[test]
    fn test_tag_collector_cancellation() {
        let collector = TagCollector::new(10);
        let c1 = collector.clone();
        let c2 = collector.clone();

        assert!(!c1.is_cancelled());
        assert!(!c2.is_cancelled());

        collector.cancel();
        assert!(c1.is_cancelled());
        assert!(c2.is_cancelled());
        assert!(collector.is_cancelled());

        // Pushes after cancellation must be rejected
        assert!(!collector.push("T1".into()));
        assert_eq!(collector.len(), 0);
    }

    #[test]
    fn test_tag_collector_multithreaded() {
        let collector = TagCollector::new(400);
        let handles: Vec<_> = (0..4)
            .map(|thread_id| {
                let col = collector.clone();
                std::thread::spawn(move || {
                    for i in 0..100 {
                        assert!(col.push(format!("T_{thread_id}_{i}")));
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(collector.len(), 400);
        assert!(collector.is_full());
        let tags = collector.harvest();
        assert_eq!(tags.len(), 400);
        assert_eq!(collector.len(), 0);
    }
}
