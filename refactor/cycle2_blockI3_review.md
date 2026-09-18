# Modernization Sub-Block I3 Qualitative Review: Collector Concurrency & Telemetry Documentation

> **Document Status:** Active Qualitative Architecture & Code Review  
> **Workspace:** `opc-cli`  
> **Target Crate:** `opc-da-client` (v0.2.0 $\rightarrow$ v0.3.0 Release Candidate)  
> **Evaluation Date:** 2026-09-18  
> **Reference Review:** [`refactor/cycle2_blockI_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockI_review.md) (Sub-Block I3)  
> **Review Pipeline:** Subagent-Orchestrated Multi-Lens Audit (5 Specialized Lens Subagents: Logic, Design, Performance, Security, API)  
> **User Interview Alignment & Technical Decisions:**
> 1. **Terminal Browse Result Handoff:** Approved **Full Move Semantics & Assert Returned Vector**. Update unit and integration tests to assert against the returned `tags` vector and verify `collector.is_empty()` post-harvest, cementing `TagCollector` as a transient streaming accumulator.
> 2. **Hierarchical Recursive Browse Chunking:** Approved **Standardize on 256-Item Chunking**. Adopt `BROWSE_CHUNK_SIZE = 256` chunking with `chunk.drain(..)` in `browse_recursive`, matching flat browsing for incremental TUI progress and fail-fast capacity bounding (CWE-400).
> 3. **Reconnection Retry Tag Duplication:** Approved **Clean Slate Entry Reset**. Call `collector.clear()` on entry in `handle_browse` if `!collector.is_empty()`, guaranteeing that connection retries under `RetryPolicy::Idempotent` start with a pristine accumulator and zero duplicate tags.
> 4. **API Ergonomics & Doc-Tests:** Approved **Implement with_capacity & In-Place clear**. Add `TagCollector::with_capacity(capacity, max_tags)` and in-place `TagCollector::clear(&self)` (preserving backing capacity), plus comprehensive runnable doc-tests across all public collector methods.

---

## 1. Executive Summary & Review Scope

Sub-Block I3 represents the **concurrency, telemetry, and memory lifecycle phase** of Modernization Block I within `opc-da-client`. While Sub-Block I1 cleaned up COM worker thread loops and Sub-Block I2 hardened industrial batch write actuation, Sub-Block I3 focuses on the tag discovery accumulator: [`TagCollector`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs).

`TagCollector` serves as the primary synchronization bridge between the background MTA COM worker thread executing namespace browsing and foreground consumer tasks (e.g., TUI progress bars, CLI telemetry, and streaming subscribers). Sub-Block I3 eliminates read-write lock contention, establishes zero-copy terminal result handoff via move semantics, introduces recursive browse chunking parity, and brings comprehensive rustdoc documentation and runnable doc-tests to the public API.

### 1.1 Scope Boundaries
The review covers three key target areas:
- [`opc-da-client/src/types/collector.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs) (`TagCollector`, `TagCollectorInner`, synchronization primitive upgrade, lock-free telemetry, buffer management, and doc-tests)
- [`opc-da-client/src/com/worker/browse.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs) (Lines 20–65: `handle_browse` terminal handoff and retry guards; Lines 167–260: `browse_recursive` leaf chunking and capacity bounding)
- [`opc-da-client/tests/tag_browsing_integration_test.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/tests/tag_browsing_integration_test.rs) and unit tests in `browse.rs` (Decoupling test assertions from incidental accumulator retention and aligning with zero-copy move semantics)

### 1.2 Core Problems Identified

1. **Terminal Result Deep-Cloning & Memory Retention Asymmetry ($O(N)$ Heap Churn):**
   In `handle_browse` (lines 37 and 56), the worker returns collected tags by calling `collector.snapshot()`. `snapshot()` performs a deep clone of the entire accumulated vector buffer. For a standard 10,000-tag namespace, this executes **10,001 heap allocations** and churns $\approx 860\text{ KB}$ of ephemeral heap memory right before the worker instance is dropped. Furthermore, the cloned strings remain pinned inside the caller's collector instance as zombie duplicates until the next browse operation. Peer worker handlers (`handle_read` and `handle_write_batch`) return owned data structures without retaining duplicate state.
2. **Exclusive `Mutex` Serialization of Observers & Ingest Workers:**
   `TagCollectorInner` synchronizes tag storage using `std::sync::Mutex<Vec<String>>`. While length queries (`len()`) are lock-free via `AtomicUsize`, invoking `collector.snapshot()` acquires an exclusive `Mutex` lock and holds it across all 10,000 string clones. During this allocation window, the background MTA worker thread is blocked from executing `push_batch(chunk.drain(..))`, and concurrent observers (such as TUI rendering frames and telemetry loggers) are completely serialized.
3. **Hierarchical Traversal Progress Starvation & Capacity Bypass (CWE-400 / CWE-770):**
   In `com/worker/browse.rs:182-214`, `browse_recursive` accumulates all leaf tags of a branch into an unbounded `Vec<String>` before pushing to `collector`. This violates the `BROWSE_CHUNK_SIZE = 256` pattern established in `browse_flat_namespace`:
   - TUI progress indicators freeze during branch traversal because `collector.len()` is not incremented incrementally.
   - If a rogue or large OPC server returns 5,000 leaves on a branch when remaining capacity is 10, `collector.is_full()` returns `false` throughout the entire loop. The worker executes 5,000 synchronous COM `get_item_id` DCOM round trips, allocating 5,000 strings, only for `push_batch` to discard 4,990 of them!
4. **Duplicate Tag Accumulation on Transient Reconnection Retry:**
   In `ComWorker::handle_request`, `ComRequest::BrowseTags` is dispatched with `RetryPolicy::Idempotent`. If a transient DCOM transport failure occurs midway through traversal (e.g. `RPC_S_SERVER_UNAVAILABLE`), `handle_browse` exits with `Err`. The connection pool reconnects and retries `handle_browse` with the *same* `collector` instance. Because `handle_browse` does not reset the accumulator on entry, the retried browse appends duplicate tags from the root, doubling memory and prematurely hitting `max_tags`.
5. **Test Assertion Coupling to Accidental Memory Duplication:**
   Six integration tests in `tag_browsing_integration_test.rs` and five unit tests in `browse.rs` assert `assert_eq!(collector.snapshot(), tags)` and `assert_eq!(collector.len(), N)`. Switching to zero-copy `harvest()` transfers ownership via `std::mem::take`, leaving `collector.len() == 0`. Without planned test updates, all 11 assertions break.
6. **Documentation Deficiencies & Missing Doc-Tests:**
   11 of 12 public methods on `TagCollector` lack runnable `# Examples` doc-tests, failing `coding-standard.md §4.5`. Furthermore, `snapshot()` lacks a `# Performance Warning` documenting its $O(N)$ allocation cost, and `TagCollector` lacks a discoverable `with_capacity` constructor and in-place `clear` method.

### 1.3 High-Level Objectives & Goals

- **Objective O1: Zero-Copy Terminal Browse Handoff ($O(1)$ Pointer Swap):**
  Switch `handle_browse` completion paths (lines 37 and 56) from `collector.snapshot()` to `collector.harvest()`. Reduce terminal handoff allocations from 10,001 to 0 (-100%) and eliminate 860 KB of memory churn on 10k tag browses.
- **Objective O2: Reader-Writer Concurrency Upgrade (`Mutex` $\rightarrow$ `RwLock`):**
  Upgrade `TagCollectorInner.tags` to `std::sync::RwLock<Vec<String>>`. Allow multiple concurrent observer threads to call `snapshot()` in parallel without lock contention, while retaining atomic lock-free queries (`len()`, `is_empty()`, `is_full()`, `is_cancelled()`) and symmetrical poison recovery.
- **Objective O3: Hierarchical Traversal Chunking Parity & Capacity Bounding:**
  Refactor `browse_recursive` to chunk leaves using `BROWSE_CHUNK_SIZE = 256` and flush via `collector.push_batch(chunk.drain(..))`, eliminating progress freezing and preventing synchronous DCOM query flooding (CWE-400).
- **Objective O4: Reconnection Retry Accumulator Clean Slate:**
  In `handle_browse`, ensure that any retry under `RetryPolicy::Idempotent` cleans partial accumulated tags from prior failed attempts before beginning traversal.
- **Objective O5: Public Ergonomic Extensions & Capacity Hinting:**
  Provide `TagCollector::with_capacity(capacity: usize, max_tags: usize)` and in-place `TagCollector::clear(&self)`, while querying iterator `size_hint()` in `push_batch` to pre-reserve buffer space.
- **Objective O6: Complete Documentation & Runnable Doc-Tests:**
  Add `# Performance Warning` to `snapshot()`, document move semantics on `harvest()`, and provide runnable `# Examples` doc-tests across all public methods on `TagCollector`.
- **Objective O7: Decoupled Integration Test Suite:**
  Update integration and unit test assertions to validate the returned `tags` vector and verify that the collector was cleanly drained post-harvest.

---

## 2. Unified Findings Matrix

All 5 specialized review lenses (**Logic**, **Design**, **Performance**, **Security**, and **API**) were executed concurrently, yielding **8 consolidated architectural findings**:

| # | Severity | Category | File:Line | Function / Symbol Signature | Summary | Source Lenses |
|:---:|:---|:---|:---|:---|:---|:---:|
| **1** | 🟠 Major | Perf / Design | [`src/com/worker/browse.rs:37, 56`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L37-L56) | `handle_browse<S: ConnectedServer>` | Terminal browse deep-clones accumulated tags via `snapshot()` right before worker drop, causing 10,001 heap allocations and retaining zombie duplicates. | Perf, Design, API, Logic, Security |
| **2** | 🟠 Major | Concurrency / Perf | [`src/types/collector.rs:17, 83`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs#L17-L83) | `struct TagCollectorInner`<br>`TagCollector::snapshot` | `TagCollectorInner` uses exclusive `Mutex` rather than `RwLock`, causing `snapshot()` to serialize all readers and block the MTA worker thread during ingest. | Perf, Concurrency, Design, Security, Logic |
| **3** | 🟠 Major | Security / Perf | [`src/com/worker/browse.rs:182-214`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L182-L214) | `browse_recursive<S: ConnectedServer>` | Unchunked leaf accumulation in hierarchical browse starves UI progress telemetry and executes unconstrained DCOM RPC queries past capacity limits (CWE-400). | Security, Perf, Design |
| **4** | 🟠 Major | Logic / Resilience | [`src/com/worker/browse.rs:23-38`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L23-L38) | `handle_browse<S: ConnectedServer>` | Reconnection retry under `RetryPolicy::Idempotent` appends duplicate tags from the root to an un-reset `TagCollector` on partial browse failure. | Logic |
| **5** | 🟠 Major | API / Governance | [`src/types/collector.rs:29-176`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs#L29-L176) | `struct TagCollector` | Absence of runnable `# Examples` doc-tests across 11 public methods on `TagCollector` violates project coding standard §4.5. | API, Design |
| **6** | 🟡 Minor | Test / Invariant | [`tests/tag_browsing_integration_test.rs:37`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/tests/tag_browsing_integration_test.rs#L37-L38) | `test_tag_browsing_*` | Integration test suite couples to incidental accumulator retention (`collector.snapshot()` / `len()`), breaking when `handle_browse` adopts `harvest()`. | Test, API, Design, Logic, Security |
| **7** | 🟡 Minor | API / Ergonomics | [`src/types/collector.rs:29, 93`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs#L29-L93) | `TagCollector::new`<br>`TagCollector::harvest` | Missing discoverable `with_capacity(capacity, max_tags)` constructor and missing in-place `clear(&self)` method; clamping to 1024 forces 4 buffer doublings. | API, Perf |
| **8** | 🟡 Minor | Logic / Concurrency | [`src/types/collector.rs:143-168`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs#L143-L168) | `TagCollector::push_batch`<br>`TagCollector::push` | Iterator panic during `push_batch` desynchronizes atomic `count` from vector length under poison recovery; missing re-check of `is_cancelled()` in `push`. | Logic, Security |

---

## 3. Detailed Findings

### Finding 1: Terminal Browse Deep-Cloning & Memory Retention Asymmetry in `handle_browse`
- **Severity:** 🟠 Major
- **Category:** Performance / Design
- **File & Line:** [`opc-da-client/src/com/worker/browse.rs:37, 56`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L37-L56)
- **Function Signature:** `handle_browse<S: ConnectedServer>(server_id: &ServerIdentifier, collector: &TagCollector, opc_server: &S) -> OpcResult<Vec<String>>`
- **Source Lenses:** Performance, Design, API, Logic, Security
- **Detail:**  
  In `handle_browse`, the final tag collection result is returned to the caller by invoking `collector.snapshot()` (lines 37 and 56). This induces substantial runtime waste:
  1. `collector.snapshot()` performs a deep clone of the entire accumulated buffer (up to `DEFAULT_MAX_TAGS = 10,000` strings). On a full browse, this executes **10,001 heap allocations** and copies $\approx 860\text{ KB}$ of ephemeral string data.
  2. The cloned `Vec<String>` is returned across the channel to the caller, while the original vector remains pinned inside `TagCollectorInner.tags`.
  3. Because callers hold a clone of `collector` (such as `TaskManager.browse_collector` in `opc-cli`), these 10,000 strings remain duplicate zombie allocations in application heap memory until the next browse operation is initiated.
  4. In `ComWorker::handle_request`, the worker drops its reference to `collector` immediately after returning `result`. Cloning the entire collection right before dropping the worker's reference is pure overhead.
- **Suggestion:**  
  Switch terminal returns in `handle_browse` to `collector.harvest()`. `harvest()` executes `std::mem::take(&mut *guard)` under the write lock in $O(1)$ time, moving the accumulated buffer directly into the return channel with **zero heap allocations** and resetting the collector's internal memory footprint to zero:
  ```rust
  // Line 36:
  if collector.is_cancelled() || collector.is_full() {
      return Ok(collector.harvest());
  }
  // ... namespace traversal ...
  let result = collector.harvest();
  tracing::info!(
      count = result.len(),
      elapsed_ms = super::elapsed_ms(start),
      "browse_tags completed"
  );
  Ok(result)
  ```

---

### Finding 2: Exclusive `Mutex` Serializes Readers and Stalls Background Ingestion
- **Severity:** 🟠 Major
- **Category:** Concurrency / Performance
- **File & Line:** [`opc-da-client/src/types/collector.rs:17, 83`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs#L17-L83)
- **Function Signature:** `struct TagCollectorInner`<br>`TagCollector::snapshot(&self) -> Vec<String>`
- **Source Lenses:** Performance, Concurrency, Design, Security, Logic
- **Detail:**  
  `TagCollectorInner` manages string storage via `std::sync::Mutex<Vec<String>>`. While progress length queries (`len()`) are lock-free via `AtomicUsize::load(Ordering::Acquire)`, any caller requesting a progress snapshot (`collector.snapshot()`) acquires an exclusive `Mutex` lock.
  Holding this exclusive lock across 10,000 string clones ($O(N)$ allocations) creates serious contention:
  1. The background MTA worker thread is blocked from executing `collector.push_batch(chunk.drain(..))` until the clone completes.
  2. Multiple concurrent observer threads (e.g. TUI rendering, telemetry export, and progress logging) are completely serialized behind one another.
- **Suggestion:**  
  Upgrade `TagCollectorInner` to use `std::sync::RwLock<Vec<String>>`. `snapshot()` acquires a shared read lock (`self.inner.tags.read()`), allowing multiple readers to inspect progress concurrently without serializing against each other. Write operations (`push`, `push_batch`, `harvest`, `clear`) acquire a write lock (`self.inner.tags.write()`). Poison recovery remains symmetric using `match lock { Ok(g) => g, Err(p) => p.into_inner() }`:
  ```rust
  struct TagCollectorInner {
      tags: std::sync::RwLock<Vec<String>>,
      count: AtomicUsize,
      max_tags: usize,
      cancelled: AtomicBool,
  }

  impl TagCollector {
      #[must_use]
      pub fn snapshot(&self) -> Vec<String> {
          let guard = match self.inner.tags.read() {
              Ok(g) => g,
              Err(poisoned) => poisoned.into_inner(),
          };
          guard.clone()
      }

      #[must_use]
      pub fn harvest(&self) -> Vec<String> {
          let mut guard = match self.inner.tags.write() {
              Ok(g) => g,
              Err(poisoned) => poisoned.into_inner(),
          };
          let harvested = std::mem::take(&mut *guard);
          self.inner.count.store(0, Ordering::Release);
          drop(guard);
          harvested
      }
  }
  ```

---

### Finding 3: Hierarchical Traversal Progress Starvation & Capacity Bypass (CWE-400)
- **Severity:** 🟠 Major
- **Category:** Security / Performance
- **File & Line:** [`opc-da-client/src/com/worker/browse.rs:182-214`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L182-L214)
- **Function Signature:** `browse_recursive<S: ConnectedServer>(server: &S, collector: &TagCollector, depth: usize) -> OpcResult<()>`
- **Source Lenses:** Security, Performance, Design
- **Detail:**  
  Unlike `browse_flat_namespace` and `try_fast_flat_browse` (which chunk items in `BROWSE_CHUNK_SIZE = 256` batches and flush via `collector.push_batch(chunk.drain(..))`), `browse_recursive` iterates across all leaves of a branch and pushes them into an unbounded local `leaf_ids: Vec<String>` before calling `collector.push_batch(leaf_ids)` at line 210:
  1. **UI Progress Starvation:** Because `collector.push_batch` is only called after all leaves in the branch are resolved, `TagCollector`'s atomic counter does not advance incrementally. TUI loading indicators appear frozen on large branches.
  2. **Capacity Bounding Bypass (CWE-400 / CWE-770):** Inside the loop, `collector.is_full()` is evaluated against the collector's current state. Because items are held in `leaf_ids` and have not yet been pushed to `collector`, `collector.is_full()` returns `false` throughout the entire loop. If `max_tags` is 10 and a branch contains 5,000 leaves, the worker executes 5,000 synchronous COM `server.get_item_id(&leaf_name)` RPC calls, only for `collector.push_batch` to discard 4,990 of them!
  3. **Unbounded Vector Growth:** `leaf_ids` starts at capacity 0 (`Vec::new()`) and repeatedly reallocates ($0 \rightarrow 4 \rightarrow 8 \rightarrow \dots \rightarrow N$).
- **Suggestion:**  
  Symmetrize hierarchical leaf ingestion with flat browsing by using a pre-allocated 256-element chunk and draining it periodically:
  ```rust
  let mut chunk = Vec::with_capacity(BROWSE_CHUNK_SIZE);
  for leaf_res in leaf_iter {
      if collector.is_cancelled() || collector.is_full() {
          break;
      }
      let leaf_name = match leaf_res {
          Ok(name) => name,
          Err(err) => {
              log_opc_err!(&err, "browse_recursive:leaf_item", depth = depth);
              continue;
          }
      };
      let item_id = match server.get_item_id(&leaf_name) {
          Ok(id) => id,
          Err(err) => {
              log_opc_err!(&err, "browse_recursive:get_item_id", depth = depth, leaf = %leaf_name);
              continue;
          }
      };
      chunk.push(item_id);
      if chunk.len() >= BROWSE_CHUNK_SIZE {
          let _ = collector.push_batch(chunk.drain(..));
          if collector.is_cancelled() || collector.is_full() {
              break;
          }
      }
  }
  if !chunk.is_empty() {
      let _ = collector.push_batch(chunk.drain(..));
  }
  ```

---

### Finding 4: Reconnection Retry Under `RetryPolicy::Idempotent` Appends Duplicate Tags
- **Severity:** 🟠 Major
- **Category:** Logic / Resilience
- **File & Line:** [`opc-da-client/src/com/worker/browse.rs:23-38`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L23-L38)
- **Function Signature:** `handle_browse<S: ConnectedServer>(server_id: &ServerIdentifier, collector: &TagCollector, opc_server: &S) -> OpcResult<Vec<String>>`
- **Source Lenses:** Logic
- **Detail:**  
  In `ComWorker::handle_request`, `ComRequest::BrowseTags` is dispatched via `dispatch_pooled_request` with `pool::RetryPolicy::Idempotent`.
  If a transient Win32 COM / DCOM transport failure (e.g. RPC Server Unavailable `0x800706BA`) occurs during namespace traversal after some tags have already been pushed to `collector`, `handle_browse` exits early with `?` without calling `harvest()`.
  Because `retry_policy` is `Idempotent`, `dispatch_with_retry` evicts the stale connection, establishes a fresh connection, and re-invokes `handle_browse` with the **same** `collector` instance.
  Because `handle_browse` currently neither clears nor drains `collector` on entry, the retried browse starts from the root and appends all tags a second time. This causes duplicate tag entries in the harvested output, doubles memory consumption, and may cause the retry run to prematurely hit `collector.max_tags()` and discard valid tags.
- **Suggestion:**  
  Ensure that retrying `handle_browse` starts with a clean accumulator. In `handle_browse`, if `!collector.is_cancelled() && !collector.is_full()`, drain any partial accumulated results from previous attempts before beginning namespace traversal:
  ```rust
  pub fn handle_browse<S: ConnectedServer>(
      server_id: &ServerIdentifier,
      collector: &TagCollector,
      opc_server: &S,
  ) -> OpcResult<Vec<String>> {
      let start = std::time::Instant::now();

      if collector.is_cancelled() || collector.is_full() {
          return Ok(collector.harvest());
      }

      // Discard any partial tags accumulated from a prior failed attempt under dispatch retry
      if !collector.is_empty() {
          collector.clear();
      }
      // ...
  ```

---

### Finding 5: Complete Absence of Runnable `# Examples` Doc-Tests on `TagCollector` Public API
- **Severity:** 🟠 Major
- **Category:** API / Governance
- **File & Line:** [`opc-da-client/src/types/collector.rs:29-176`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs#L29-L176)
- **Function Signature:** `struct TagCollector`
- **Source Lenses:** API, Design
- **Detail:**  
  11 of the 12 public methods/implementations on `TagCollector` lack runnable `# Examples` doc-tests:
  `new`, `unbounded`, `max_tags`, `len`, `is_empty`, `is_full`, `cancel`, `is_cancelled`, `snapshot`, `harvest`, `push`, and `Default::default()`. Only `push_batch` contains an example. This violates `coding-standard.md §4.5` requiring comprehensive doc comments and examples on all public crate items.
- **Suggestion:**  
  Add complete rustdoc comments and runnable doc-tests across all public methods on `TagCollector`.

---

### Finding 6: Integration Test Suite Coupled to Incidental Accumulator Retention
- **Severity:** 🟡 Minor
- **Category:** Test / Invariant
- **File & Line:** [`opc-da-client/tests/tag_browsing_integration_test.rs:37, 74, 133, 179, 229, 286`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/tests/tag_browsing_integration_test.rs#L37-L38)
- **Function Signature:** `test_tag_browsing_*`
- **Source Lenses:** Test, API, Design, Logic, Security
- **Detail:**  
  Six integration tests in `tag_browsing_integration_test.rs` and five unit tests in `src/com/worker/browse.rs` (lines 278, 302, 319, 339, 357) assert `assert_eq!(collector.snapshot(), tags)` and `assert_eq!(collector.len(), tags.len())` post-browse.
  Once `handle_browse` adopts `harvest()`, `collector.len()` will be 0 and `collector.snapshot()` will be empty. These 11 tests will fail unless updated to reflect zero-copy move semantics.
- **Suggestion:**  
  Update test assertions to validate the returned `tags` vector and verify that the collector was drained:
  ```rust
  assert_eq!(tags, vec!["Pump.Speed", "Valve.State", "Tank.Level"]);
  assert_eq!(collector.len(), 0);
  assert!(collector.is_empty());
  ```
  Rename unit test `test_handle_browse_preserves_collector` in `browse.rs` to `test_handle_browse_harvests_tags`.

---

### Finding 7: Missing Discoverable `with_capacity` Constructor and In-Place `clear` Method
- **Severity:** 🟡 Minor
- **Category:** API / Ergonomics
- **File & Line:** [`opc-da-client/src/types/collector.rs:29, 93`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs#L29-L93)
- **Function Signature:** `TagCollector::new`<br>`TagCollector::harvest`
- **Source Lenses:** API, Performance
- **Detail:**  
  1. `TagCollector::new` hardcodes initial allocation to `max_tags.min(1024)`. When ingesting `DEFAULT_MAX_TAGS = 10,000`, the inner vector undergoes 4 successive buffer doublings ($1\text{k} \rightarrow 2\text{k} \rightarrow 4\text{k} \rightarrow 8\text{k} \rightarrow 16\text{k}$), overshooting target capacity by 6,384 elements (~153 KB wasted headroom). There is no `with_capacity` constructor for callers with known batch expectations.
  2. Callers wishing to reset an accumulator (such as between namespace retries) are forced to call `let _ = collector.harvest();`, which drops the allocated capacity. An in-place `clear(&self)` calling `guard.clear()` preserves the allocated vector capacity.
- **Suggestion:**  
  Add `TagCollector::with_capacity(capacity: usize, max_tags: usize)` and `pub fn clear(&self)`.

---

### Finding 8: Iterator Panic Atomic Counter Desynchronization & Cancellation Re-check
- **Severity:** 🟡 Minor
- **Category:** Logic / Concurrency
- **File & Line:** [`opc-da-client/src/types/collector.rs:107-168`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs#L107-L168)
- **Function Signature:** `TagCollector::push_batch`<br>`TagCollector::push`
- **Source Lenses:** Logic, Security
- **Detail:**  
  1. In `push_batch`, `guard.push(tag)` modifies the vector during iteration, but `inner.count` is updated only after the loop completes. If an external iterator panics on `.next()`, the lock is poisoned and `inner.count` is out of sync with `guard.len()`.
  2. In `push`, `self.is_cancelled()` is checked before lock acquisition, but not re-checked inside the write lock.
- **Suggestion:**  
  In lock poison recovery, re-synchronize `inner.count.store(guard.len(), Ordering::Release)`. In `push`, re-check cancellation inside the write guard before appending.

---

## 4. Blast Radius Table & Cross-Subsystem Impact

| Component / File | Direct Callers | Indirect Callers | Breaking Change? | Risk Mitigation |
|---|:---:|:---:|:---:|---|
| [`src/types/collector.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs) | 8 | 14 | No | Public method signatures take `&self`. `RwLock` migration is 100% backwards-compatible. New methods (`with_capacity`, `clear`) are purely additive. |
| [`src/com/worker/browse.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs) | 2 | 4 | No | Internal worker module. Handoff switch to `harvest()` preserves `OpcResult<Vec<String>>` return signature. Retry reset prevents duplicate tag injection. |
| [`tests/tag_browsing_integration_test.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/tests/tag_browsing_integration_test.rs) | 0 | 0 | No | Test assertions updated to validate returned `tags` vector and verify drained collector status (`is_empty()`). |
| `opc-cli/src/app.rs` | 1 | 1 | No | TUI queries `collector.len()` during loading and stores returned `tags` in `ViewState`. Zero impact on UI behavior. |

---

## 5. Critical Things to Watch Out For

1. **Test Assertion Breakages on `harvest()` Move Semantics:**  
   Switching `handle_browse` from `snapshot()` to `harvest()` drains `TagCollector`. All 6 integration tests in `tag_browsing_integration_test.rs` and 5 unit tests in `browse.rs` that currently assert `collector.snapshot() == tags` will fail unless their assertions are updated to verify the returned vector and assert `collector.is_empty()`.
2. **Reconnection Retry Tag Duplication:**  
   Ensure `handle_browse` calls `collector.clear()` on entry if `!collector.is_empty()`, preventing idempotent retries from appending duplicate tags from the root.
3. **Lock Poison Recovery Invariance:**  
   Preserve `match lock { Ok(g) => g, Err(p) => p.into_inner() }` across both shared read locks (`snapshot()`) and exclusive write locks (`push()`, `push_batch()`, `harvest()`, `clear()`). Re-synchronize `inner.count` with `guard.len()` during recovery.
4. **Hierarchical Traversal Chunking Parity:**  
   `browse_recursive` must chunk leaves using `BROWSE_CHUNK_SIZE = 256` and flush via `chunk.drain(..)`, ensuring `collector.is_full()` halts DCOM RPC queries immediately upon reaching capacity.
5. **Lock Duration in `push_batch`:**  
   Pre-reserve vector capacity in `push_batch` using `tags.into_iter().size_hint()` to prevent buffer doublings while holding the exclusive write lock.

---

## 6. Deliverables for Sub-Block I3

1. **Refactored [`src/types/collector.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs):**
   - Upgraded `TagCollectorInner` from `Mutex<Vec<String>>` to `RwLock<Vec<String>>`.
   - Added `TagCollector::with_capacity(capacity: usize, max_tags: usize)` constructor.
   - Added in-place `TagCollector::clear(&self)` method.
   - Added capacity reservation in `push_batch` via `size_hint()`.
   - Added `# Performance Warning` on `snapshot()` and move semantics documentation on `harvest()`.
   - Added runnable `# Examples` doc-tests across all 13 public methods/traits on `TagCollector`.
2. **Refactored [`src/com/worker/browse.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs):**
   - Converted `handle_browse` terminal returns to zero-copy `collector.harvest()`.
   - Added retry reset guard at `handle_browse` entry (`collector.clear()`).
   - Added `BROWSE_CHUNK_SIZE = 256` leaf chunking and early capacity termination to `browse_recursive`.
   - Renamed and inverted `test_handle_browse_preserves_collector` to `test_handle_browse_harvests_tags`.
3. **Updated Test Suites:**
   - Decoupled 6 integration tests in [`tests/tag_browsing_integration_test.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/tests/tag_browsing_integration_test.rs).
   - Added multi-threaded concurrency unit tests validating simultaneous `snapshot()` readers alongside background `push_batch()` writers.
   - Added unit test validating retry reset behavior on simulated connection failure.
4. **Verification Gate:**
   - 100% pass across all 9 quality gates in `pwsh scripts/verify.ps1` with zero warnings under `-D warnings`.

---

## 7. User Clarification Interview & Technical Decisions

The strategic alignment interview with the user confirmed the following decisions:

1. **Terminal Browse Result Handoff: Full Move Semantics & Assert Returned Vector (Approved)**
   - *Decision:* Convert `handle_browse` (lines 37 and 56) to return `collector.harvest()`, which moves the accumulated `Vec<String>` buffer directly to the caller via `std::mem::take` with zero allocations and resets the collector to empty. Decouple integration and unit tests from legacy post-browse accumulator inspection, updating them to validate the returned `tags` vector and verify `collector.is_empty()`.
   - *Rationale:* Eliminates 10,001 heap allocations and 860 KB of ephemeral memory churn per 10k tag browse. Cements `TagCollector` as a transient streaming accumulator rather than an in-memory repository.

2. **Hierarchical Recursive Browse Chunking: Standardize on 256-Item Chunking (Approved)**
   - *Decision:* Refactor `browse_recursive` in `com/worker/browse.rs` to buffer leaves into a 256-element chunk and flush via `collector.push_batch(chunk.drain(..))`, matching the pattern in `browse_flat_namespace`.
   - *Rationale:* Unfreezes TUI progress telemetry (`loading_progress()`) on large hierarchical branches, enables prompt `collector.is_full()` evaluation, and prevents runaway DCOM round-trip RPC flooding (`server.get_item_id`) past capacity limits (CWE-400).

3. **Reconnection Retry Tag Duplication: Clean Slate Entry Reset (Approved)**
   - *Decision:* Call `collector.clear()` on entry in `handle_browse` if `!collector.is_empty() && !collector.is_cancelled() && !collector.is_full()`.
   - *Rationale:* Guarantees that when `ComWorker` retries `handle_browse` under `RetryPolicy::Idempotent` following a transient transport disconnection, the retried browse starts from a clean slate rather than appending duplicate tags from the root.

4. **Public API Ergonomics & Doc-Tests: Implement with_capacity & In-Place clear (Approved)**
   - *Decision:* Implement `TagCollector::with_capacity(capacity: usize, max_tags: usize)` and in-place `TagCollector::clear(&self)` (preserving allocated backing vector capacity), add capacity pre-reservation in `push_batch` via `size_hint()`, and author comprehensive rustdoc comments with runnable `# Examples` doc-tests across all public methods.
   - *Rationale:* Eliminates 4 vector buffer doublings on 10k tag collections, provides idiomatic capacity configuration, and ensures 100% compliance with `coding-standard.md §4.5`.

---

## 8. Next Steps & Planning Gate

📋 **Sub-Block I3 Qualitative Review Complete & Aligned.**  
Artifact saved to: [**`refactor/cycle2_blockI3_review.md`**](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockI3_review.md).

Recommended next step: Proceed to `/plan-making` for **Sub-Block I3 (Collector Concurrency & Telemetry Documentation)**.
