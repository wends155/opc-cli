//! Tag collection accumulators and progress monitors.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

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
    tags: RwLock<Vec<String>>,
    count: AtomicUsize,
    max_tags: usize,
    cancelled: AtomicBool,
}

struct PushBatchGuard<'a> {
    count: &'a AtomicUsize,
    added: usize,
}

impl Drop for PushBatchGuard<'_> {
    fn drop(&mut self) {
        if self.added > 0 {
            self.count.fetch_add(self.added, Ordering::Release);
        }
    }
}

impl TagCollector {
    /// Standard default capacity cap when unconstrained (10,000 tags).
    pub const DEFAULT_MAX_TAGS: usize = 10_000;

    /// Creates a new `TagCollector` with an explicitly pre-allocated buffer capacity
    /// bounded to at most `max_tags` items.
    ///
    /// The initial allocation is clamped to `capacity.min(max_tags)` to prevent unbounded
    /// heap reservation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::with_capacity(256, 1000);
    /// assert_eq!(collector.max_tags(), 1000);
    /// assert_eq!(collector.len(), 0);
    /// assert!(collector.is_empty());
    /// ```
    #[must_use]
    pub fn with_capacity(capacity: usize, max_tags: usize) -> Self {
        Self {
            inner: Arc::new(TagCollectorInner {
                tags: RwLock::new(Vec::with_capacity(capacity.min(max_tags))),
                count: AtomicUsize::new(0),
                max_tags,
                cancelled: AtomicBool::new(false),
            }),
        }
    }

    /// Creates a new `TagCollector` bounded to at most `max_tags` items.
    ///
    /// Initial vector allocation is pre-sized to `max_tags.min(1024)`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::new(500);
    /// assert_eq!(collector.max_tags(), 500);
    /// assert_eq!(collector.len(), 0);
    /// ```
    #[must_use]
    pub fn new(max_tags: usize) -> Self {
        Self::with_capacity(max_tags.min(1024), max_tags)
    }

    /// Creates an unbounded `TagCollector` (`max_tags = usize::MAX`).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::unbounded();
    /// assert_eq!(collector.max_tags(), usize::MAX);
    /// assert!(!collector.is_full());
    /// ```
    #[must_use]
    pub fn unbounded() -> Self {
        Self::new(usize::MAX)
    }

    /// Returns the maximum capacity cap.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::new(42);
    /// assert_eq!(collector.max_tags(), 42);
    /// ```
    #[must_use]
    pub fn max_tags(&self) -> usize {
        self.inner.max_tags
    }

    /// Returns the number of tags collected so far without acquiring a lock.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::new(10);
    /// assert_eq!(collector.len(), 0);
    /// collector.push("Tag1".into());
    /// assert_eq!(collector.len(), 1);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.count.load(Ordering::Acquire)
    }

    /// Returns true if no tags have been collected yet.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::new(10);
    /// assert!(collector.is_empty());
    /// collector.push("Tag1".into());
    /// assert!(!collector.is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if the collector has reached or exceeded its `max_tags` capacity cap.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::new(1);
    /// assert!(!collector.is_full());
    /// collector.push("Tag1".into());
    /// assert!(collector.is_full());
    /// ```
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.len() >= self.inner.max_tags
    }

    /// Signals cancellation to the background browse worker.
    ///
    /// Once cancelled, subsequent pushes and batch operations are immediately rejected.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::new(10);
    /// assert!(!collector.is_cancelled());
    /// collector.cancel();
    /// assert!(collector.is_cancelled());
    /// ```
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::Release);
    }

    /// Returns true if cancellation has been requested.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::new(10);
    /// assert!(!collector.is_cancelled());
    /// collector.cancel();
    /// assert!(collector.is_cancelled());
    /// ```
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::Acquire)
    }

    /// Clears all collected tags in-place, resetting atomic count to 0 while preserving
    /// vector buffer allocation capacity.
    ///
    /// Does not modify the cancellation status of the collector.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::new(10);
    /// collector.push("Tag1".into());
    /// assert_eq!(collector.len(), 1);
    /// collector.clear();
    /// assert_eq!(collector.len(), 0);
    /// assert!(collector.is_empty());
    /// ```
    pub fn clear(&self) {
        let mut guard = match self.inner.tags.write() {
            Ok(g) => g,
            Err(poisoned) => {
                let g = poisoned.into_inner();
                self.inner.count.store(g.len(), Ordering::Release);
                g
            }
        };
        guard.clear();
        drop(guard);
        self.inner.count.store(0, Ordering::Release);
    }

    /// Returns a cloned snapshot of all tags collected so far without draining.
    ///
    /// Acquires a shared `RwLock::read()`, allowing concurrent progress monitoring
    /// without blocking other readers.
    ///
    /// # Performance Warning
    ///
    /// This operation performs an $O(N)$ deep copy of all collected tag strings, allocating
    /// a new heap vector and individual string clones. For terminal consumption where ownership
    /// of tags is handed off to the caller, prefer [`TagCollector::harvest`], which operates
    /// in $O(1)$ time via pointer transfer (`std::mem::take`) with zero allocations.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::new(10);
    /// collector.push("Tag1".into());
    /// let tags = collector.snapshot();
    /// assert_eq!(tags, vec!["Tag1"]);
    /// assert_eq!(collector.len(), 1);
    /// ```
    #[must_use]
    pub fn snapshot(&self) -> Vec<String> {
        let guard = match self.inner.tags.read() {
            Ok(g) => g,
            Err(poisoned) => {
                let g = poisoned.into_inner();
                self.inner.count.store(g.len(), Ordering::Release);
                g
            }
        };
        guard.clone()
    }

    /// Drains and returns all collected tags, resetting the buffer and atomic count in $O(1)$ time.
    ///
    /// Acquires an exclusive write lock and uses `std::mem::take` to swap out the accumulator
    /// vector without heap reallocation or element-wise copying.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::new(10);
    /// collector.push("Tag1".into());
    /// let tags = collector.harvest();
    /// assert_eq!(tags, vec!["Tag1"]);
    /// assert_eq!(collector.len(), 0);
    /// assert!(collector.is_empty());
    /// ```
    #[must_use]
    pub fn harvest(&self) -> Vec<String> {
        let mut guard = match self.inner.tags.write() {
            Ok(g) => g,
            Err(poisoned) => {
                let g = poisoned.into_inner();
                self.inner.count.store(g.len(), Ordering::Release);
                g
            }
        };
        let harvested = std::mem::take(&mut *guard);
        drop(guard);
        self.inner.count.store(0, Ordering::Release);
        harvested
    }

    /// Pushes a tag into the collector if not full or cancelled.
    ///
    /// Employs double-checked cancellation: checks `is_cancelled()` lock-free before
    /// lock acquisition, and re-checks `is_cancelled()` inside the write lock to prevent
    /// race conditions.
    ///
    /// # Returns
    ///
    /// `true` if the tag was accepted, `false` if capacity was reached or cancellation was requested.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::new(2);
    /// assert!(collector.push("Tag1".into()));
    /// assert!(collector.push("Tag2".into()));
    /// assert!(!collector.push("Tag3".into()));
    /// ```
    #[must_use = "Returns false if the collector is full or cancelled"]
    pub fn push(&self, tag: String) -> bool {
        if self.is_cancelled() || self.is_full() {
            return false;
        }
        let mut guard = match self.inner.tags.write() {
            Ok(g) => g,
            Err(poisoned) => {
                let g = poisoned.into_inner();
                self.inner.count.store(g.len(), Ordering::Release);
                g
            }
        };
        if self.is_cancelled() || guard.len() >= self.inner.max_tags {
            return false;
        }
        guard.push(tag);
        drop(guard);
        self.inner.count.fetch_add(1, Ordering::Release);
        true
    }

    /// Batch-inserts tags under a single write lock acquisition.
    ///
    /// Pre-reserves vector capacity clamped to `lower.min(remaining).min(1024)` based on
    /// iterator size hint to prevent unbounded heap reservation from untrusted iterators.
    /// Protects the atomic count with an RAII guard ensuring accurate progress count even
    /// if the input iterator panics during ingestion.
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
        let mut guard = match self.inner.tags.write() {
            Ok(g) => g,
            Err(p) => {
                let g = p.into_inner();
                self.inner.count.store(g.len(), Ordering::Release);
                g
            }
        };
        if self.is_cancelled() {
            return 0;
        }
        let remaining = self.inner.max_tags.saturating_sub(guard.len());
        if remaining == 0 {
            return 0;
        }
        let iter = tags.into_iter();
        let (lower, _) = iter.size_hint();
        let reserve_hint = lower.min(remaining).min(1024);
        if reserve_hint > 0 {
            guard.reserve(reserve_hint);
        }
        let mut sync = PushBatchGuard {
            count: &self.inner.count,
            added: 0,
        };
        for tag in iter {
            if sync.added >= remaining || self.is_cancelled() {
                break;
            }
            guard.push(tag);
            sync.added += 1;
        }
        drop(guard);
        sync.added
    }
}

impl Default for TagCollector {
    /// Creates a default `TagCollector` with `DEFAULT_MAX_TAGS` capacity.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use opc_da_client::types::TagCollector;
    ///
    /// let collector = TagCollector::default();
    /// assert_eq!(collector.max_tags(), TagCollector::DEFAULT_MAX_TAGS);
    /// ```
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

    #[test]
    fn test_collector_clear_preserves_capacity() {
        let collector = TagCollector::with_capacity(64, 100);
        assert_eq!(collector.len(), 0);
        assert!(collector.is_empty());
        assert!(collector.push("Alpha".into()));
        assert!(collector.push("Beta".into()));
        assert_eq!(collector.len(), 2);
        collector.clear();
        assert_eq!(collector.len(), 0);
        assert!(collector.is_empty());
        assert!(collector.push("Gamma".into()));
        assert_eq!(collector.len(), 1);
    }

    #[test]
    fn test_collector_with_capacity_and_size_hint() {
        let collector = TagCollector::with_capacity(512, 1000);
        assert_eq!(collector.max_tags(), 1000);
        assert_eq!(collector.len(), 0);
        let tags: Vec<String> = (0..300).map(|i| format!("BulkTag_{i}")).collect();
        let accepted = collector.push_batch(tags.clone());
        assert_eq!(accepted, 300);
        assert_eq!(collector.len(), 300);
        assert_eq!(collector.snapshot(), tags);
    }

    #[test]
    fn test_collector_concurrent_rwlock_stress() {
        let collector = TagCollector::new(1000);
        let mut handles = Vec::new();
        for w in 0..2 {
            let col = collector.clone();
            handles.push(std::thread::spawn(move || {
                for b in 0..50 {
                    let chunk: Vec<String> = (0..10).map(|i| format!("W{w}_B{b}_T{i}")).collect();
                    let _ = col.push_batch(chunk);
                }
            }));
        }
        for _ in 0..4 {
            let col = collector.clone();
            handles.push(std::thread::spawn(move || {
                while !col.is_full() && !col.is_cancelled() {
                    let snap = col.snapshot();
                    let current_len = col.len();
                    assert!(snap.len() <= current_len);
                    if current_len >= 1000 {
                        break;
                    }
                    std::thread::yield_now();
                }
            }));
        }
        for h in handles {
            h.join().expect("thread join");
        }
        assert_eq!(collector.len(), 1000);
        let harvested = collector.harvest();
        assert_eq!(harvested.len(), 1000);
        assert_eq!(collector.len(), 0);
    }

    #[test]
    fn test_collector_poison_recovery_resync() {
        struct PanickingIter {
            yielded: usize,
            panic_after: usize,
        }
        impl Iterator for PanickingIter {
            type Item = String;
            fn next(&mut self) -> Option<Self::Item> {
                assert!(
                    self.yielded < self.panic_after,
                    "intentional iterator panic"
                );
                self.yielded += 1;
                Some(format!("PanicTag_{}", self.yielded))
            }
        }
        let collector = TagCollector::new(50);
        let col_clone = collector.clone();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _ = col_clone.push_batch(PanickingIter {
                yielded: 0,
                panic_after: 2,
            });
        }));
        let snap = collector.snapshot();
        assert_eq!(snap, vec!["PanicTag_1", "PanicTag_2"]);
        assert_eq!(collector.len(), 2);
        assert!(collector.push("PostPoisonTag".into()));
        assert_eq!(collector.len(), 3);
        let harvested = collector.harvest();
        assert_eq!(harvested, vec!["PanicTag_1", "PanicTag_2", "PostPoisonTag"]);
        assert_eq!(collector.len(), 0);
    }
}
