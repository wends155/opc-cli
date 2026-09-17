# Modernization Sub-Block I1 Qualitative Review: COM Worker Hygiene & Buffer Reuse

> **Document Status:** Active Qualitative Architecture & Code Review  
> **Workspace:** `opc-cli`  
> **Target Crate:** `opc-da-client` (v0.2.0 $\rightarrow$ v0.3.0 Release Candidate)  
> **Evaluation Date:** 2026-09-17  
> **Reference Documents:**  
> - [`refactor/cycle2_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_review.md) (Block I Findings #3, #6, #10, #13, #14, #20)  
> - [`refactor/cycle2_blockI_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockI_review.md) (Master Block I Review & 4-Sub-Block Decomposition)  
> **Review Pipeline:** Subagent-Orchestrated Multi-Lens Audit (5 Specialized Lens Subagents: Logic, Design, Performance, Security, API)  

---

## 1. Executive Summary & Scope

Sub-Block **I1** addresses internal memory allocation hygiene, buffer lifecycle management, and dead code excision within the dedicated COM worker thread subsystem. The COM worker thread isolates Windows COM Multi-Threaded Apartment (MTA) state from asynchronous Tokio tasks. However, iterative feature additions across Cycle 2 left several hotspots of heap allocation churn on high-frequency paths, unnecessary defensive clones, uncalled legacy entry points, and diagnostic probes exposed under `#[allow(dead_code)]`.

This qualitative review consolidates findings across all 5 analytical lenses (Logic & Correctness, Software Architecture & Design, Performance & Allocation, Security & Defensive Invariants, and API Ergonomics). All 5 lenses confirm that Sub-Block I1 is strictly contained within internal, crate-private modules (`opc_da_client::com::worker`). Downstream public traits and facade methods (`OpcDaClient`, `OpcProvider`, CLI commands) experience **zero breaking changes** and **zero API surface modifications**.

### Key Technical Objectives
1. **Browse Buffer Reuse (`browse.rs`):** Replace repeated vector reallocations (`std::mem::replace`) with in-place draining (`chunk.drain(..)`), amortizing heap allocation over the entire flat namespace traversal.
2. **Read Error Move Semantics & Zero-Allocation Tracing (`read.rs`):** Pass registration results by value (`Vec<GroupItemResult>`) to move `OpcError` directly into `rejected_errors` without deep-cloning, and stream errors directly into tracing spans (`error = %err`) to eliminate intermediate heap `String` allocations.
3. **Parity Validation Guard (`read.rs`):** Add an upfront tag count parity assertion (`tag_count == valid_indices.len() + rejected_errors.len()`) in [`assemble_tag_values`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L215) to prevent silent state dropping on index divergence.
4. **Hot-Path Dispatch Zero-Allocation (`worker.rs`):** Eliminate eager `endpoint.clone()` and `host.to_string()` allocations executed prior to `catch_unwind` on every dispatched request, referencing borrows directly in the cold panic-recovery path.
5. **Clean Slate Excision & Scoping (`worker.rs`, `pool.rs`):** Excise uncalled async worker constructors (`start_async`, `start_async_with_initializer`), remove the hazardous `PriorityRequestQueue::clear` method, delete singular alias `clear_active_group`, scope test assertion probes (`len`, `sender`) to `#[cfg(test)]`, and eliminate all `#[allow(dead_code)]` suppressions.

---

## 2. Alignment with Upstream Review & User Interview Decisions

During the architectural alignment interview, four critical technical nuances were analyzed and decided:

| # | Technical Question | Decision / Consensus | Architectural Rationale |
|---|---|---|---|
| **A1** | **Browse Buffer Chunk Sizing** | Keep `BROWSE_CHUNK_SIZE = 256` fixed. | Balances mutex lock contention on `TagCollector` with cancellation responsiveness ($\le 256$ items). Draining reuses the 6 KiB buffer indefinitely without reallocations. |
| **A2** | **Read Error Logging Overhead** | Stream directly via `error = %err`. | `OpcError` implements `Display`. Eliminates intermediate `err.to_string()` heap allocation entirely on rejected item logging. |
| **A3** | **Singular Alias Deprecation** | Strict Clean Slate: Delete `clear_active_group`. | Migrate the 3 internal call sites to [`clear_active_groups()`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L115) and excise the alias to eliminate naming ambiguity on the multi-group LRU cache. |
| **A4** | **Connection Pool Diagnostics** | Scope `len` to `#[cfg(test)]`, delete `is_empty`. | `len` is called in 12 unit tests but 0 production sites. `is_empty` has 0 callers workspace-wide. Eliminates `#[allow(dead_code)]` across pool diagnostic methods. |

---

## 3. Unified Findings Matrix

Across the 5 specialized review lenses, **12 unique findings** were identified and synthesized:

| # | Severity | Lens | Target File:Line | Affected Item | Summary |
|---|---|---|---|---|---|
| **1** | 🟠 Major | **Logic** | [`read.rs:226`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L226) | [`assemble_tag_values`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L215) | Missing upfront parity validation between total tags and sum of valid + rejected indices allows silent state drops. |
| **2** | 🟠 Major | **Perf / API** | [`read.rs:184-210`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L184) | [`partition_item_results`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L184) | Borrowed results slice forces `err.clone()` and `err.to_string()` heap allocation on discarded registration results. |
| **3** | 🟠 Major | **Perf / Design** | [`browse.rs:97-100, 131-134`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L97) | [`browse_flat_namespace`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L65) / [`try_fast_flat_browse`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L112) | Repeated vector reallocations via `std::mem::replace` force ~39 throwaway 6 KiB vectors (~240 KB) during 10k browse. |
| **4** | 🟡 Minor | **Perf** | [`worker.rs:568`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L568) | [`dispatch_pooled_request`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L552) | Eager `endpoint.clone()` heap allocation on hot request dispatch executed solely for cold panic-recovery path. |
| **5** | 🟡 Minor | **API / Design** | [`worker.rs:268-302`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L268) | [`ComWorker::start_async`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L270) | Uncalled dead async worker constructors retained under `#[allow(dead_code)]` violate SRP and Clean Slate hygiene. |
| **6** | 🟡 Minor | **API / Design** | [`worker.rs:404-408`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L404) | [`PriorityRequestQueue::clear`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L405) | Hazardous dead queue clearer drops pending caller reply channels without error responses, causing broken pipe errors. |
| **7** | 🟡 Minor | **API / Design** | [`pool.rs:125-128`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L125) | [`PooledServer::clear_active_group`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L126) | Misleading singular alias for multi-group LRU cache clearing causes internal API ambiguity. |
| **8** | ⚪ Nitpick | **Security** | [`pool.rs:87-95`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L87) | [`PooledServer::insert_active_group`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L87) | Non-looping `if` capacity check relies on external invariants; `while` provides self-healing capacity bounding. |
| **9** | ⚪ Nitpick | **API / Design** | [`pool.rs:240-251`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L240) | [`ConnectionPool::len`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L241) / [`is_empty`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L248) | Sizing probes in production struct: `is_empty` has 0 callers; `len` called only in unit test assertions. |
| **10** | ⚪ Nitpick | **API / Design** | [`worker.rs:229-233`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L229) | [`ComWorker::sender`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L231) | Internal request sender channel accessor compiled in production under `#[allow(dead_code)]` instead of `#[cfg(test)]`. |
| **11** | ⚪ Nitpick | **API / Design** | [`pool.rs:26`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L26) | [`struct CachedGroup<G>`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L26) | Spurious `#[allow(dead_code)]` attribute on actively utilized 6-field cache structure. |
| **12** | ⚪ Nitpick | **Logic / Perf** | [`worker.rs:12, 531`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L12) | [`dispatch_discovery_request`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L519) | Unused `ConnectedGroup` import in `worker.rs`; eager `host.to_string()` allocation prior to `catch_unwind`. |

---

## 4. Deep-Dive Qualitative Analyses & Technical Blueprints

### Module A: Browse Buffer In-Place Draining ([`opc-da-client/src/com/worker/browse.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs))

#### Problem Analysis
In [`browse_flat_namespace`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L65) and [`try_fast_flat_browse`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L112), tag batching into `BROWSE_CHUNK_SIZE = 256` allocates a fresh vector on every chunk boundary:
```rust
// Current anti-pattern (browse.rs:97-100 & 131-134):
let _ = collector.push_batch(std::mem::replace(
    &mut chunk,
    Vec::with_capacity(BROWSE_CHUNK_SIZE),
));
```
Each 256-element vector of `String` consumes $256 \times 24 = 6,144$ bytes (6 KiB) of heap storage for `String` handles, plus heap memory for tag strings. For a typical industrial server with 10,000 tags, this triggers $\approx 39$ heap allocations and deallocations (~240 KiB of throwaway vector buffers).

#### Invariant Verification
[`TagCollector::push_batch`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collector.rs#L143) accepts `impl IntoIterator<Item = String>`. The standard library `std::vec::Drain<'_, String>` implements `Iterator<Item = String>` and `ExactSizeIterator`.
1. **Buffer Reuse:** Calling `collector.push_batch(chunk.drain(..))` moves all accumulated `String`s into the collector while resetting `chunk.len()` to 0 without releasing the allocated 6 KiB buffer capacity.
2. **Cancellation Semantics:** If `push_batch` returns early or accepts fewer items than available (e.g. `max_tags` reached or `collector.cancel()`), the `Drain` destructor automatically drops all unconsumed strings and resets `chunk.len() = 0`.
3. **Loop Termination:** The loop checks `if collector.is_cancelled() || collector.is_full() { break; }` immediately after `push_batch`. Any remaining elements ($\le 255$) at loop exit are flushed with `if !chunk.is_empty() { let _ = collector.push_batch(chunk.drain(..)); }`.

#### Technical Blueprint
```rust
// In browse_flat_namespace (lines 96-103) & try_fast_flat_browse (lines 130-137):
if chunk.len() >= BROWSE_CHUNK_SIZE {
    let _ = collector.push_batch(chunk.drain(..));
    if collector.is_cancelled() || collector.is_full() {
        break;
    }
}

// In terminal flush (lines 106-108 & 149-151):
if !chunk.is_empty() {
    let _ = collector.push_batch(chunk.drain(..));
}
```

---

### Module B: Read Error Ownership & Tracing Stream ([`opc-da-client/src/com/worker/read.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs))

#### Problem Analysis
1. In [`handle_read`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L26), [`register_item_group`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L62) yields an owned `RegisteredItemGroup` containing `item_results: Vec<GroupItemResult>`. Line 137 passes `&results` by borrowed slice into [`partition_item_results`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L184). Because `results` is borrowed, line 202 is forced to call `err.clone()`: `rejected_errors.push((idx, err.clone()))`.
2. Line 195 executes `let err_msg = err.to_string();`, allocating a heap string purely to format `error = %err_msg` into `tracing::warn!`.
3. In [`assemble_tag_values`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L215), the function verifies `states.len() == valid_indices.len()`, but fails to verify upfront that `tag_count == valid_indices.len() + rejected_errors.len()`. If an upstream caller truncates the tag batch or cache indices diverge, the loop terminates prematurely, silently dropping read states.

#### Invariant Verification
1. **Disjoint Index Partitioning:** Every index $k \in [0, N-1]$ in `results.into_iter().enumerate()` is strictly assigned to either `valid_indices` (when `error.is_none()`) or `rejected_errors` (when `error.is_some()`). Both collections remain strictly monotonically increasing.
2. **Borrowed Slice Invariant in `assemble_tag_values`:** On cache hits, [`CachedGroup`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L26) holds persistent `rejected_errors`, which must be borrowed across repeated ticks. On cache misses, `rejected_errors` is moved into `CachedGroup`. Therefore, `assemble_tag_values` must retain its borrowed signature `rejected_errors: &[(usize, OpcError)]`, while `partition_item_results` takes owned `results: Vec<GroupItemResult>`.

#### Technical Blueprint
```rust
// read.rs:184-210
fn partition_item_results(
    results: Vec<GroupItemResult>,
    tag_ids: &[String],
    server_id: &ServerIdentifier,
) -> (Vec<ServerItemHandle>, Vec<usize>, Vec<(usize, OpcError)>) {
    let count = results.len();
    let mut server_handles = Vec::with_capacity(count);
    let mut valid_indices = Vec::with_capacity(count);
    let mut rejected_errors = Vec::new();

    for (idx, item_result) in results.into_iter().enumerate() {
        if let Some(err) = item_result.error {
            let tag_name = tag_ids.get(idx).map_or("<unknown>", String::as_str);
            tracing::warn!(
                server = %server_id,
                tag = %tag_name,
                error = %err,
                "read_tag_values: add_items rejected tag"
            );
            rejected_errors.push((idx, err));
        } else {
            server_handles.push(item_result.server_handle);
            valid_indices.push(idx);
        }
    }

    (server_handles, valid_indices, rejected_errors)
}

// read.rs:222-241 inside assemble_tag_values:
let tag_count = tags.len();
let states = item_states.unwrap_or_default();

if tag_count != valid_indices.len() + rejected_errors.len() {
    let err = OpcError::Internal(format!(
        "Server {server_id} tag count mismatch: expected {tag_count} tags, got {} valid and {} rejected",
        valid_indices.len(),
        rejected_errors.len()
    ));
    log_opc_err!(
        &err,
        "read_tag_values:parity_mismatch",
        server = %server_id,
        tag_count = tag_count,
        valid_count = valid_indices.len(),
        rejected_count = rejected_errors.len()
    );
    return Err(err);
}

if states.len() != valid_indices.len() {
    // ... existing mismatched error handling ...
}
```

---

### Module C: Hot-Path Zero-Allocation & Worker Hygiene ([`opc-da-client/src/com/worker.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs))

#### Problem Analysis
1. In [`dispatch_pooled_request`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L552), line 568 performs `let endpoint_clone = endpoint.clone();` *before* invoking `catch_unwind`. Because [`OpcServerEndpoint`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs#L483) contains heap-allocated `Option<String>` and `ProgId(String)`, this allocates on every single read, write, browse, and ping request solely to service the cold panic path.
2. In [`dispatch_discovery_request`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L519), line 531 allocates `let host_str = host.to_string();` before `catch_unwind` solely for logging.
3. [`ComWorker::start_async`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L270) and `start_async_with_initializer` have 0 callers across the workspace.
4. [`PriorityRequestQueue::clear`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L405) silently drops queued requests without sending error replies to waiting Tokio oneshot channels (unlike `drain_and_reject`).
5. [`ComWorker::sender`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L231) is an internal channel accessor called only in unit tests (`src/com/worker/tests.rs:38`).
6. Unused import `ConnectedGroup` at line 12.

#### Technical Blueprint
```rust
// worker.rs:563-591 in dispatch_pooled_request
if reply.is_closed() {
    tracing::debug!(op, server = %endpoint, "Caller cancelled pooled request; skipping dispatch");
    return;
}
let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    pool::dispatch_with_retry(pool, connector, endpoint, retry_policy, &mut f)
}));

match result {
    Ok(res) => {
        let _ = reply.send(res);
    }
    Err(payload) => {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pool.remove(endpoint);
        }));
        let msg = extract_panic_message(&*payload);
        let err: OpcError = WorkerError::Panic(msg).into();
        log_opc_err!(
            &err,
            op,
            server = %endpoint,
        );
        let _ = reply.send(Err(err));
    }
}

// worker.rs:228-234 in ComWorker
#[cfg(test)]
#[must_use]
pub fn sender(&self) -> Option<&mpsc::Sender<ComRequest>> {
    self.sender.as_ref()
}
```
- Delete `start_async` and `start_async_with_initializer` (lines 268–302).
- Delete `PriorityRequestQueue::clear` (lines 404–408).
- Prune unit test `test_priority_request_queue_clear_drops_senders` in [`tests.rs:1017-1045`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/tests.rs#L1017).
- Remove `ConnectedGroup` from imports on line 12.

---

### Module D: Connection Pool Scoping & Alias Excision ([`opc-da-client/src/com/worker/pool.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs))

#### Problem Analysis
1. [`PooledServer::clear_active_group`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L126) is a singular alias for [`clear_active_groups`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L115). It is called in 3 places: `pool.rs:268` (in `ConnectionPool::evict`), `pool.rs:605` (unit test), and `read.rs:341` (unit test).
2. [`ConnectionPool::is_empty`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L248) has 0 callers workspace-wide.
3. [`ConnectionPool::len`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L241) is called in 12 assertion checks in tests, but 0 in production.
4. [`CachedGroup<G>`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L26) is decorated with `#[allow(dead_code)]` even though all 6 fields are actively accessed.
5. In [`insert_active_group`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L87), using `if` instead of `while` leaves active group capacity vulnerable if multiple items were ever enqueued.

#### Technical Blueprint
```rust
// pool.rs:87-95 in insert_active_group
while self.active_groups.len() >= MAX_ACTIVE_GROUPS
    && let Some(lru) = self.active_groups.pop_back()
{
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = self
            .server
            .remove_group(lru.server_handle, GroupRemovalMode::Force);
    }));
}

// Migrate pool.rs:268:
if let Some(mut pooled) = self.connections.remove(endpoint) {
    pooled.clear_active_groups();
    true
} else {
    false
}

// pool.rs:240-244:
#[cfg(test)]
#[must_use]
pub(crate) fn len(&self) -> usize {
    self.connections.len()
}
```
- Delete `PooledServer::clear_active_group` (lines 125–128).
- Delete `ConnectionPool::is_empty` (lines 247–250).
- Remove `#[allow(dead_code)]` from `CachedGroup<G>` (line 26).
- Update unit tests in `pool.rs:605` and `read.rs:341` to call `clear_active_groups()`.

---

## 5. Comprehensive Architectural Blast Radius Table

| Module / File | Change Type | Lines Modified | Impacted Invariants | Downstream Consumer Impact |
|---|---|---|---|---|
| [`opc-da-client/src/com/worker/browse.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs) | Refactor | 96-108, 130-151 | `chunk.drain(..)` reuses 6 KiB vector buffer; loop exit & cancellation checks preserved. | **Zero**. `handle_browse` signature and return values unchanged. |
| [`opc-da-client/src/com/worker/read.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs) | Refactor | 137, 184-210, 222-241 | `partition_item_results` takes owned `Vec<GroupItemResult>`; `error = %err`; upfront tag count parity assertion. | **Zero**. Crate-private read pipeline only; `handle_read` public contract preserved. |
| [`opc-da-client/src/com/worker.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs) | Excision & Optimization | 12, 228-234, 268-302, 404-408, 531, 568-588 | Zero-alloc request dispatch before `catch_unwind`; delete `start_async*` and `clear()`; scope `sender` to test. | **Zero**. `ComWorker::start` and `send_request` public facade intact. |
| [`opc-da-client/src/com/worker/pool.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs) | Excision & Hardening | 26, 87-95, 125-128, 240-251, 268 | Migrate to `clear_active_groups()`; delete singular alias and `is_empty`; test-scope `len`; while-loop LRU eviction. | **Zero**. `ConnectionPool` is crate-private. |
| [`opc-da-client/src/com/worker/tests.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/tests.rs) | Test Maintenance | 1017-1045 | Remove obsolete test `test_priority_request_queue_clear_drops_senders`. | **Zero**. `drain_and_reject` covered by existing test at L1281. |

---

## 6. Critical Things to Watch Out For

1. **Cancellation Latency Invariant in Flat Browse ($\le 256$ Items):**  
   `chunk.drain(..)` empties the buffer into `collector.push_batch`. The loop immediately evaluates `if collector.is_cancelled() || collector.is_full() { break; }`. The maximum lag between an external cancellation request and loop termination remains bounded by 1 enumerator fetch ($< 1\text{ ms}$) and $\le 256$ batch items.
2. **Index Monotonicity & Parity Invariants:**  
   In `partition_item_results`, sequential `into_iter().enumerate()` ensures `valid_indices` and `rejected_errors` are strictly disjoint, ordered subsets of $0..N$. The new upfront check in `assemble_tag_values` (`tag_count == valid_indices.len() + rejected_errors.len()`) guarantees that no index drift occurs between the batch request and COM server responses.
3. **Panic Boundary Containment:**  
   In `dispatch_pooled_request`, removing `endpoint.clone()` before `catch_unwind` relies on `endpoint: &OpcServerEndpoint` implementing `RefUnwindSafe`. Because `OpcServerEndpoint` contains only immutable data structures (`Option<String>` and `ServerIdentifier`), it satisfies `RefUnwindSafe` natively. Accessing `endpoint` in `Err(payload)` for `pool.remove(endpoint)` is 100% sound.
4. **Test Suite Integrity:**  
   Unit test `test_priority_request_queue_clear_drops_senders` ([`tests.rs:1017-1045`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/tests.rs#L1017)) must be excised in tandem with `PriorityRequestQueue::clear`. The call sites in `pool.rs:605` and `read.rs:341` must be updated to `clear_active_groups()`.

---

## 7. Next Steps & Planning Gate

📋 **Qualitative Review for Sub-Block I1 Complete.**  
The technical specifications, invariant verifications, and refactoring blueprints have been established and codified in this report:  
📄 [**`refactor/cycle2_blockI1_review.md`**](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockI1_review.md)

### Immediate Next Step
Transition to the **Think (Audit)** phase under `/plan-making` to generate the detailed implementation plan:
- Target Artifact: `refactor/cycle2_blockI1_plan.md`
- Gate Keyword: **"Plan"**
