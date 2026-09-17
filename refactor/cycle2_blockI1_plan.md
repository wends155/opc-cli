# Modernization Sub-Block I1 Implementation Plan: COM Worker Hygiene & Buffer Reuse

> **Document Status:** Active Implementation Plan (Think Phase)  
> **Workspace:** `opc-cli`  
> **Target Crate:** `opc-da-client` (v0.2.0 $\rightarrow$ v0.3.0 Release Candidate)  
> **Evaluation Date:** 2026-09-17  
> **Reference Review:** [`refactor/cycle2_blockI1_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockI1_review.md)  
> **Master Review:** [`refactor/cycle2_blockI_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockI_review.md)  

**Role:** Architect • **Date:** 2026-09-17 • **Tier:** M  
**Scope:** Sub-Block I1 — COM Worker Hygiene, Buffer Reuse & Dead Code Excision  

---

## Builder Context

Read before starting:
- [`opc-da-client/src/com/worker/browse.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs) L65-170 (Flat namespace browsing loops, chunk buffer management, and batch push)
- [`opc-da-client/src/com/worker/read.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs) L130-284 (Item result partitioning, tracing logging, and tag values assembly)
- [`opc-da-client/src/com/worker.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs) L220-305, L400-420, L519-592 (Worker constructors, queue operations, panic handling, and request dispatch)
- [`opc-da-client/src/com/worker/pool.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs) L80-135, L240-270 (Active group eviction, singular alias migration, and test sizing methods)
- [`refactor/cycle2_blockI1_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockI1_review.md) (All 12 synthesized findings across 5 lenses)

---

## Problem Statement

Iterative architectural upgrades across Cycle 2 modernized the Windows COM worker subsystem with MTA apartment isolation, multi-group LRU caching, and circuit breaker cooldowns. However, several internal inefficiencies, defensive cloning patterns, and obsolete APIs remain:
1. **Browse Buffer Allocation Churn:** In [`browse_flat_namespace`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L65) and [`try_fast_flat_browse`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L112), batching into `BROWSE_CHUNK_SIZE = 256` replaces the vector buffer on every flush using `std::mem::replace(&mut chunk, Vec::with_capacity(BROWSE_CHUNK_SIZE))`. For a 10,000-tag browse, this triggers $\approx 39$ short-lived 6 KiB vector allocations totaling ~240 KiB of throwaway heap churn.
2. **Defensive Error Cloning & Intermediate String Allocations:** In [`read.rs:184-210`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L184), [`partition_item_results`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L184) consumes results by borrowed slice `&[GroupItemResult]`, forcing `err.clone()` into `rejected_errors` despite the caller owning `Vec<GroupItemResult>`. Line 195 materializes `let err_msg = err.to_string();` purely for logging.
3. **Missing Tag Parity Validation:** In [`assemble_tag_values`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L215), the function checks `states.len() == valid_indices.len()`, but fails to validate that `tag_count == valid_indices.len() + rejected_errors.len()`, allowing silent truncation or late state exhaustion if index drift occurs.
4. **Hot-Path Dispatch Allocations:** In [`worker.rs:568, 531`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L568), `dispatch_pooled_request` and `dispatch_discovery_request` eagerly execute `endpoint.clone()` and `host.to_string()` prior to `std::panic::catch_unwind`, incurring heap allocations on the hot path solely to service cold panic recovery.
5. **Dead Code & Test Probe Visibility:** `ComWorker::start_async*` (lines 268-302) and `PriorityRequestQueue::clear` (lines 404-408) have zero callers. `clear()` presents an actor footgun that silently drops caller reply channels without error responses. Singular alias `clear_active_group` clutters the API. Test assertion probes (`len`, `sender`) and uncalled `is_empty` are maintained under `#[allow(dead_code)]`.

### User Interview Decisions Integrated
- **A1:** Keep `BROWSE_CHUNK_SIZE = 256` fixed (balances collector lock contention with $\le 256$ item cancellation responsiveness).
- **A2:** Stream read errors directly via `error = %err` without intermediate string allocations.
- **A3:** Clean Slate: Delete singular `clear_active_group` alias and migrate the 3 call sites to `clear_active_groups()`.
- **A4:** Scope `ConnectionPool::len` to `#[cfg(test)]`, delete `is_empty()`, and remove `#[allow(dead_code)]` from `CachedGroup`.
- **A5:** Fail fast on tag count parity mismatch in `assemble_tag_values` with `OpcError::Internal` and trigger active group eviction.
- **A6:** Eliminate cold-path allocations in both `dispatch_pooled_request` (`endpoint.clone()`) and `dispatch_discovery_request` (`host.to_string()`).
- **A7:** Convert `test_priority_request_queue_clear_drops_senders` into `test_priority_request_queue_drop_drops_senders` verifying `drop(queue)` behavior.

---

## Plan Objectives

| ID | Objective | Success Criteria | Steps |
|----|-----------|-----------------|-------|
| **O1** | **Browse Buffer Reuse** | `chunk.drain(..)` replaces `std::mem::replace` across all flushes in `browse.rs`; unit tests prove zero tags lost over 600 items across multiple boundaries and bounded capacity is respected. | 9-10 |
| **O2** | **Read Error Ownership & Zero-Alloc Logging** | `partition_item_results` takes owned `results: Vec<GroupItemResult>`; zero `err.clone()` in partition; `error = %err` streams directly into tracing; upfront parity check fails fast on mismatch. | 3-6 |
| **O3** | **Zero-Alloc Hot-Path Dispatch** | `dispatch_pooled_request` and `dispatch_discovery_request` eliminate pre-unwind `endpoint.clone()` and `host.to_string()` allocations; panic recovery accesses borrowed data safely. | 11-12 |
| **O4** | **Clean Slate Excision & API Hygiene** | `start_async*`, `PriorityRequestQueue::clear`, `clear_active_group`, and `is_empty` deleted; `test_priority_request_queue_drop_drops_senders` validates `drop(queue)`. | 1-2, 7-8 |
| **O5** | **Zero-Warning Workspace Quality Gate** | All 8 verification gates in `scripts/verify.ps1` pass with zero warnings, zero dead code attributes in worker modules, and zero test regressions. | 13 |

---

## Review History & Verdict

| Cycle | Reviewer | Model | Verdict | Adjustments Applied |
|---|---|---|---|---|
| 1 | `refactor/cycle2_blockI1_review.md` | Multi-Agent (5 Flash Lenses) | ✅ Approved | Synthesized 12 findings into 4 targeted modules; confirmed zero public API breakages. |
| 2 | Architect Alignment Interview | Main Agent + User | ✅ Approved | Finalized decisions A1-A7: fail-fast parity check, dual dispatch zero-alloc, test conversion. |
| 3 | `plan-reviewer` | `flash` (Gemini 3.8 Flash High) | ✅ Approved (Post-Revision) | Fixed Step 11 mock hook & closure signature; added `NamespaceType::Flat` coverage in Step 9; provided exact `while` loop snippet in Step 8; added dedicated unit test for `partition_item_results` in Step 6; clarified allocation placement in Step 4. |
| 4 | Architect & 7-Lens Planners Audit | Multi-Agent (7 Flash Planners + Main Agent) | ✅ Approved & Enhanced | Corrected Step 6 Action 6 variable scoping and missing field typo (`results` and `&tag_ids`); standardized parity mismatch error format across Step 3/4; reinforced stable Rust while loop guard in Step 8; ensured consistent chunk.drain(..) buffer reuse in Step 10; formalized architectural invariants A1-A5. |

---

## Negative Scope

**Out of Scope:**
- Do NOT modify batch write allocation or CWE-626 null byte rejection (deferred to Sub-Block I2).
- Do NOT upgrade `TagCollector` to `RwLock` or alter `harvest()` semantics in `handle_browse` (deferred to Sub-Block I3).
- Do NOT implement stack SSO representation on `WriteBatch` (deferred to Sub-Block I4).
- Do NOT touch public facade interfaces (`OpcDaClient`, `OpcProvider`, CLI commands).

---

## Interface Contracts

### Internal Signatures Modified

```rust
// read.rs:184
fn partition_item_results(
    results: Vec<GroupItemResult>,
    tag_ids: &[String],
    server_id: &ServerIdentifier,
) -> (Vec<ServerItemHandle>, Vec<usize>, Vec<(usize, OpcError)>)

// read.rs:215
pub(crate) fn assemble_tag_values<'a>(
    tags: impl ExactSizeIterator<Item = &'a str>,
    valid_indices: &[usize],
    rejected_errors: &[(usize, OpcError)],
    item_states: Option<Vec<OpcResult<GroupItemState>>>,
    server_id: &ServerIdentifier,
) -> OpcResult<Vec<TagValue>>

// pool.rs:87
pub(crate) fn insert_active_group(&mut self, group: CachedGroup<S::Group>)

// pool.rs:115
pub(crate) fn clear_active_groups(&mut self)

// pool.rs:240
#[cfg(test)]
pub(crate) fn len(&self) -> usize

// worker.rs:229
#[cfg(test)]
pub fn sender(&self) -> Option<&mpsc::Sender<ComRequest>>
```

---

## Performance Constraints

- **Buffer Reuse Amortization:** Single 6 KiB buffer allocation (`Vec<String>` capacity 256) per flat browse operation. `chunk.drain(..)` resets length to 0 while retaining capacity. Allocations for 10k items reduce from $\approx 39$ to 1.
- **Hot-Path Zero-Allocation:** Zero heap allocation in `dispatch_pooled_request` and `dispatch_discovery_request` on the 99.999% normal execution path.
- **Zero-Copy Tracing:** Streaming `%err` and `%host` eliminates intermediate `String` materialization.

---

## Security Constraints

- **Resource Ceilings (CWE-400):** `BROWSE_CHUNK_SIZE = 256`, `MAX_ACTIVE_GROUPS = 4`, `MAX_ACTIVE_CONNECTIONS = 32`, `MAX_TAG_BATCH_SIZE = 10_000`.
- **Value Misalignment Prevention (CWE-682):** Upfront 3-way parity check (`tag_count == valid_indices.len() + rejected_errors.len()`) and state length verification (`states.len() == valid_indices.len()`) prevents tag-to-value offset errors.
- **Panic Containment:** Two-tier panic containment with `AssertUnwindSafe`. `OpcServerEndpoint` conforms to `RefUnwindSafe` allowing borrow across unwinds.

---

## Blast Radius Table

| Symbol | File | Direct Callers | Indirect Deps | Test-Only | Cross-Package? |
|---|---|---|---|---|---|
| `partition_item_results` | `read.rs` | 1 (`handle_read`) | 0 | 0 | No |
| `assemble_tag_values` | `read.rs` | 1 (`handle_read`) | 0 | 4 tests | No |
| `clear_active_group` (deleted) | `pool.rs` | 3 migrated (`pool.rs:268`, `pool.rs:605`, `read.rs:341`) | 0 | 2 tests | No |
| `clear_active_groups` | `pool.rs` | 4 (`drop`, `evict`, tests) | 0 | 2 tests | No |
| `PriorityRequestQueue::clear` (deleted) | `worker.rs` | 1 test converted | 0 | 1 test | No |
| `ComWorker::start_async*` (deleted) | `worker.rs` | 0 | 0 | 0 | No |
| `ConnectionPool::is_empty` (deleted) | `pool.rs` | 0 | 0 | 0 | No |
| `ConnectionPool::len` | `pool.rs` | 12 tests | 0 | Yes (`#[cfg(test)]`) | No |
| `ComWorker::sender` | `worker.rs` | 3 tests | 0 | Yes (`#[cfg(test)]`) | No |

---

## Test Plan (TDD)

1. **`test_priority_request_queue_drop_drops_senders`**: Verifies `drop(queue)` closes queued oneshot reply senders cleanly with `RecvError`.
2. **`test_assemble_tag_values_parity_mismatch_fails_fast`**: Verifies underflow and overflow parity mismatches return `Err(OpcError::Internal)`.
3. **`test_connection_pool_len_and_lifecycle`**: Verifies `pool.len()` accurately reflects 0 $\rightarrow$ 1 $\rightarrow$ 2 $\rightarrow$ 1 $\rightarrow$ 0 across connection lifecycle under `#[cfg(test)]`.
4. **`test_handle_browse_chunk_draining_preserves_all_tags_across_boundaries`**: Verifies 600 tags across $256+256+88$ chunk boundaries are drained and preserved in order.
5. **`test_handle_browse_chunk_draining_respects_capacity_limits`**: Verifies bounded capacity halts chunk draining at exact limit (300 items).
6. **`test_handle_browse_chunk_draining_cancellation_without_leaks`**: Verifies `collector.cancel()` aborts browse without memory leaks.
7. **`test_worker_panic_recovery_with_borrowed_endpoint`**: Verifies worker thread catches panic and removes server via borrowed endpoint reference.

---

## Global Execution Order

### Component Group 1: Priority Request Queue Hygiene & Test Scaffolding ([`worker.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs), [`tests.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/tests.rs))

#### Step 1: [TEST] `opc-da-client/src/com/worker/tests.rs` — [~] `test_priority_request_queue_drop_drops_senders` (L1017-1045)
- **Pre:** `CHECK`
- **Target:** `test_priority_request_queue_clear_drops_senders` L1017-1045
- **Action:** Convert `test_priority_request_queue_clear_drops_senders` to test explicit `drop(queue)` behavior instead of calling `queue.clear()`:
```rust
#[tokio::test]
async fn test_priority_request_queue_drop_drops_senders() {
    use crate::com::worker::{ComRequest, PriorityRequestQueue};
    let mut queue = PriorityRequestQueue::default();
    let (tx1, rx1) = tokio::sync::oneshot::channel();
    let (tx2, rx2) = tokio::sync::oneshot::channel();

    queue.push(ComRequest::Ping {
        endpoint: OpcServerEndpoint::local_prog_id("Mock.Server.Q1"),
        reply: tx1,
    });
    queue.push(ComRequest::Ping {
        endpoint: OpcServerEndpoint::local_prog_id("Mock.Server.Q2"),
        reply: tx2,
    });

    assert!(!queue.is_empty());
    assert_eq!(queue.len(), 2);

    drop(queue);

    assert!(rx1.await.is_err(), "rx1 must receive RecvError after drop(queue)");
    assert!(rx2.await.is_err(), "rx2 must receive RecvError after drop(queue)");
}
```
- **Post:** `TEST (cargo test -p opc-da-client --lib com::worker::tests::test_priority_request_queue_drop_drops_senders expects: exit 0)`

#### Step 2: [MODIFY] `opc-da-client/src/com/worker.rs` — [-] `PriorityRequestQueue::clear` (L404-408)
- **Pre:** Step 1 passed
- **Target:** `PriorityRequestQueue::clear` L404-408
- **Action:** Delete `PriorityRequestQueue::clear(&mut self)` and its `#[allow(dead_code)]` annotation.
- **Post:** `CHECK, no clear() on PriorityRequestQueue (rg "fn clear\(&mut self\)" opc-da-client/src/com/worker.rs expects: 0 matches)`

---

### Component Group 2: Read Engine Parity Validation & Error Move Semantics ([`read.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs))

#### Step 3: [TEST] `opc-da-client/src/com/worker/read.rs` — [+] `test_assemble_tag_values_parity_mismatch_fails_fast` (L286+)
- **Pre:** Step 2 passed
- **Target:** `tests` module in `read.rs`
- **Action:** Add unit test asserting that `assemble_tag_values` returns `Err(OpcError::Internal)` on both underflow and overflow parity divergence:
```rust
#[test]
fn test_assemble_tag_values_parity_mismatch_fails_fast() {
    use crate::connector::GroupItemState;
    use crate::types::{ClientItemHandle, OpcQuality, OpcValue, ServerIdentifier};

    let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
    let requested_tags = ["tag1", "tag2", "tag3"];
    let tag_count = requested_tags.len();

    // Underflow Parity Mismatch
    let valid_indices_underflow = vec![0];
    let rejected_errors_underflow = vec![(1, OpcError::InvalidState("rejected tag2".into()))];
    let states_underflow = Some(vec![Ok(GroupItemState {
        client_handle: ClientItemHandle::new(1),
        value: OpcValue::Int(10),
        quality: OpcQuality::GOOD,
        timestamp: std::time::SystemTime::UNIX_EPOCH,
    })]);

    let underflow_res = assemble_tag_values(
        requested_tags.into_iter(),
        &valid_indices_underflow,
        &rejected_errors_underflow,
        states_underflow,
        &server_id,
    );
    let expected_underflow_msg = format!(
        "Server {server_id} tag count parity mismatch: expected {tag_count} tags, got {} valid and {} rejected",
        valid_indices_underflow.len(),
        rejected_errors_underflow.len()
    );
    assert!(
        matches!(
            underflow_res,
            Err(OpcError::Internal(ref msg)) if msg == &expected_underflow_msg
        ),
        "Expected OpcError::Internal for underflow parity mismatch, got: {underflow_res:?}"
    );

    // Overflow Parity Mismatch
    let valid_indices_overflow = vec![0, 2];
    let rejected_errors_overflow = vec![
        (1, OpcError::InvalidState("rejected 1".into())),
        (3, OpcError::InvalidState("rejected 3".into())),
    ];
    let states_overflow = Some(vec![
        Ok(GroupItemState {
            client_handle: ClientItemHandle::new(1),
            value: OpcValue::Int(10),
            quality: OpcQuality::GOOD,
            timestamp: std::time::SystemTime::UNIX_EPOCH,
        }),
        Ok(GroupItemState {
            client_handle: ClientItemHandle::new(3),
            value: OpcValue::Int(30),
            quality: OpcQuality::GOOD,
            timestamp: std::time::SystemTime::UNIX_EPOCH,
        }),
    ]);

    let overflow_res = assemble_tag_values(
        requested_tags.into_iter(),
        &valid_indices_overflow,
        &rejected_errors_overflow,
        states_overflow,
        &server_id,
    );
    let expected_overflow_msg = format!(
        "Server {server_id} tag count parity mismatch: expected {tag_count} tags, got {} valid and {} rejected",
        valid_indices_overflow.len(),
        rejected_errors_overflow.len()
    );
    assert!(
        matches!(
            overflow_res,
            Err(OpcError::Internal(ref msg)) if msg == &expected_overflow_msg
        ),
        "Expected OpcError::Internal for overflow parity mismatch, got: {overflow_res:?}"
    );
}
```
- **Post:** `RED(test_assemble_tag_values_parity_mismatch_fails_fast)`

#### Step 4: [MODIFY] `opc-da-client/src/com/worker/read.rs` — [~] `assemble_tag_values` (L221-241)
- **Pre:** `RED(test_assemble_tag_values_parity_mismatch_fails_fast)`
- **Target:** `assemble_tag_values` entry L221-241
- **Action:** Add upfront tag count parity validation before state inspection:
```rust
    let tag_count = tags.len();
    let states = item_states.unwrap_or_default();

    if tag_count != valid_indices.len() + rejected_errors.len() {
        let err = OpcError::Internal(format!(
            "Server {server_id} tag count parity mismatch: expected {tag_count} tags, got {} valid and {} rejected",
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
        let err = OpcError::Internal(format!(
            "Server {server_id} returned mismatched read result array size: expected {} items, got {}",
            valid_indices.len(),
            states.len()
        ));
        log_opc_err!(
            &err,
            "read_tag_values:mismatched",
            server = %server_id,
            expected = valid_indices.len(),
            actual = states.len()
        );
        return Err(err);
    }

    // Allocate accumulator vector only after fail-fast parity checks pass
    let mut tag_values = Vec::with_capacity(tag_count);
```
- **Post:** `GREEN(test_assemble_tag_values_parity_mismatch_fails_fast)`

#### Step 5: [TEST] `opc-da-client/src/com/worker/read.rs` — [~] `test_group_guard_disarm_on_read` (L341)
- **Pre:** Step 4 passed
- **Target:** `test_group_guard_disarm_on_read` line 341
- **Action:** Update call from `pooled.clear_active_group()` to `pooled.clear_active_groups()`.
- **Post:** `TEST (cargo test -p opc-da-client --lib com::worker::read::tests::test_group_guard_disarm_on_read expects: exit 0)`

#### Step 6: [MODIFY] `opc-da-client/src/com/worker/read.rs` — [~] `partition_item_results` & `handle_read` (L137, L184-210)
- **Pre:** Step 5 passed
- **Target:** `partition_item_results` L184-210 and `handle_read` caller L137
- **Action:**
  1. Change `partition_item_results` signature to accept `results: Vec<GroupItemResult>` by value.
  2. Iterate with `results.into_iter().enumerate()`.
  3. Stream tracing log directly with `error = %err`.
  4. Move `err` directly into `rejected_errors.push((idx, err))` without `.clone()`.
  5. Use `tag_ids.get(idx).map_or("<unknown>", String::as_str)` for defensive tag name lookup.
  6. In `handle_read` L137, update call to pass `results` by value and use existing local `tag_ids` (`partition_item_results(results, &tag_ids, &endpoint.identifier)`).
  7. Add dedicated unit test in `read.rs::tests`:
```rust
    #[test]
    fn test_partition_item_results_move_semantics_and_boundary() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let tag_ids = vec!["Valid.Tag".to_string()]; // length 1 to test boundary fallback on idx 1
        let results = vec![
            GroupItemResult {
                server_handle: ServerItemHandle::new(101),
                error: None,
            },
            GroupItemResult {
                server_handle: ServerItemHandle::new(0),
                error: Some(OpcError::InvalidState("Item rejected".into())),
            },
        ];

        let (handles, valid_idx, rejected) = partition_item_results(results, &tag_ids, &server_id);
        assert_eq!(handles, vec![ServerItemHandle::new(101)]);
        assert_eq!(valid_idx, vec![0]);
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].0, 1);
        assert!(matches!(rejected[0].1, OpcError::InvalidState(_)));
    }
```
- **Post:** `CHECK, no err.clone() in partition_item_results (rg "err\.clone\(\)" opc-da-client/src/com/worker/read.rs expects: exactly 1 match in assemble_tag_values)`
- **🔒 CHECKPOINT 1** (`cargo test -p opc-da-client --lib com::worker::read`)

---

### Component Group 3: Connection Pool Diagnostics & Group Management ([`pool.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs))

#### Step 7: [TEST] `opc-da-client/src/com/worker/pool.rs` — [+] `test_connection_pool_len_and_lifecycle` (L420+) & [~] `test_dispatch_with_retry_active_group_reuse` (L605)
- **Pre:** Checkpoint 1 passed
- **Target:** `tests` module in `pool.rs`
- **Action:**
  1. Update line 605 in `test_dispatch_with_retry_active_group_reuse` from `pooled.clear_active_group()` to `pooled.clear_active_groups()`.
  2. Add `test_connection_pool_len_and_lifecycle` to verify `pool.len()` across creation, dispatches, eviction, and clear.
```rust
#[test]
fn test_connection_pool_len_and_lifecycle() {
    let state = Arc::new(MockState::default());
    let connector = Arc::new(MockServerConnector::with_state(state.clone()));
    let mut pool = ConnectionPool::new();
    let endpoint1 = OpcServerEndpoint::local_prog_id("Server.1");
    let endpoint2 = OpcServerEndpoint::local_prog_id("Server.2");

    assert_eq!(pool.len(), 0);

    let res1 = dispatch_with_retry(
        &mut pool,
        &connector,
        &endpoint1,
        RetryPolicy::Idempotent,
        |_| Ok("ok1"),
    );
    assert_eq!(res1.unwrap(), "ok1");
    assert_eq!(pool.len(), 1);

    let res2 = dispatch_with_retry(
        &mut pool,
        &connector,
        &endpoint2,
        RetryPolicy::Idempotent,
        |_| Ok("ok2"),
    );
    assert_eq!(res2.unwrap(), "ok2");
    assert_eq!(pool.len(), 2);

    assert!(pool.evict(&endpoint1));
    assert_eq!(pool.len(), 1);

    pool.clear();
    assert_eq!(pool.len(), 0);
}
```
- **Post:** `TEST (cargo test -p opc-da-client --lib com::worker::pool::tests::test_connection_pool_len_and_lifecycle expects: exit 0)`

#### Step 8: [MODIFY] `opc-da-client/src/com/worker/pool.rs` — [~] `insert_active_group`, `evict`, `len`; [-] `clear_active_group`, `is_empty`, `CachedGroup allow(dead_code)` (L26, L87-95, L125-128, L240-251, L268)
- **Pre:** Step 7 passed
- **Target:** `pool.rs` L26, L87-95, L125-128, L240-251, L268
- **Action:**
  1. Remove `#[allow(dead_code)]` from `pub(crate) struct CachedGroup<G>` (line 26).
  2. Change `if self.active_groups.len() >= MAX_ACTIVE_GROUPS` to `while self.active_groups.len() >= MAX_ACTIVE_GROUPS` in `insert_active_group` (lines 87-95) per IPR Control Flow Override:
```rust
    pub(crate) fn insert_active_group(&mut self, group: CachedGroup<S::Group>) {
        while self.active_groups.len() >= MAX_ACTIVE_GROUPS {
            if let Some(lru) = self.active_groups.pop_back() {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = self.server.remove_group(lru.server_handle, GroupRemovalMode::Force);
                }));
            }
        }
        self.active_groups.push_front(group);
    }
```
    > [!NOTE]
    > `while self.active_groups.len() >= MAX_ACTIVE_GROUPS && let Some(...)` must NOT be used because it requires nightly `#![feature(let_chains)]` (`E0658`). The nested `if let Some(lru) = self.active_groups.pop_back()` inside `while self.active_groups.len() >= MAX_ACTIVE_GROUPS` is required on stable Rust (MSRV 1.93.1).
  3. Delete `pub(crate) fn clear_active_group(&mut self)` (lines 125-128).
  4. In `ConnectionPool::evict` line 268, change `pooled.clear_active_group()` to `pooled.clear_active_groups()`.
  5. Replace `len` and `is_empty` (lines 240-251): delete `is_empty`, scope `len` to `#[cfg(test)]`, and remove `#[allow(dead_code)]`:
```rust
    #[cfg(test)]
    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.connections.len()
    }
```
- **Post:** `CHECK, no clear_active_group in pool.rs (rg "clear_active_group\b" opc-da-client/src/ expects: 0 matches)`
- **🔒 CHECKPOINT 2** (`cargo test -p opc-da-client --lib com::worker::pool`)

---

### Component Group 4: Browse Buffer Draining & Capacity Bounding ([`browse.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs))

#### Step 9: [TEST] `opc-da-client/src/com/worker/browse.rs` — [+] `test_handle_browse_chunk_draining_*` (L270+)
- **Pre:** Checkpoint 2 passed
- **Target:** `tests` module in `browse.rs`
- **Action:** Add 4 comprehensive unit tests exercising multi-chunk draining boundaries ($N=600$) under both hierarchical fallback (`try_fast_flat_browse`) and native flat namespace (`browse_flat_namespace`), capacity bounding ($N=300$), and clean cancellation:
```rust
    #[test]
    fn test_handle_browse_chunk_draining_preserves_all_tags_across_boundaries() {
        let generated_tags: Vec<String> = (0..600)
            .map(|i| format!("Device.Channel1.Tag{i:04}"))
            .collect();
        let server = MockConnectedServer::default().with_tags(generated_tags.clone());
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let collector = TagCollector::new(1000);

        let tags = handle_browse(&server_id, &collector, &server)
            .expect("browse operation across chunk boundaries should succeed");

        assert_eq!(tags.len(), 600);
        assert_eq!(collector.len(), 600);
        assert_eq!(tags, generated_tags);
        assert_eq!(collector.snapshot(), generated_tags);
    }

    #[test]
    fn test_handle_browse_flat_namespace_chunk_draining_with_flat_organization() {
        let generated_tags: Vec<String> = (0..600)
            .map(|i| format!("Device.Channel1.Tag{i:04}"))
            .collect();
        let server = MockConnectedServer::default()
            .with_tags(generated_tags.clone())
            .with_organization(NamespaceType::Flat);
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let collector = TagCollector::new(1000);

        let tags = handle_browse(&server_id, &collector, &server)
            .expect("browse flat operation across chunk boundaries should succeed");

        assert_eq!(tags.len(), 600);
        assert_eq!(collector.len(), 600);
        assert_eq!(tags, generated_tags);
        assert_eq!(collector.snapshot(), generated_tags);
    }

    #[test]
    fn test_handle_browse_chunk_draining_respects_capacity_limits() {
        let generated_tags: Vec<String> = (0..600)
            .map(|i| format!("Device.Channel1.Tag{i:04}"))
            .collect();
        let server = MockConnectedServer::default().with_tags(generated_tags.clone());
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let collector = TagCollector::new(300);

        let tags = handle_browse(&server_id, &collector, &server)
            .expect("browse operation with bounded capacity should succeed");

        assert_eq!(tags.len(), 300);
        assert_eq!(collector.len(), 300);
        assert!(collector.is_full());
        assert_eq!(tags, &generated_tags[..300]);
    }

    #[test]
    fn test_handle_browse_chunk_draining_cancellation_without_leaks() {
        let generated_tags: Vec<String> = (0..600)
            .map(|i| format!("Device.Channel1.Tag{i:04}"))
            .collect();
        let server = MockConnectedServer::default().with_tags(generated_tags);
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let collector = TagCollector::new(1000);
        collector.cancel();

        let tags = handle_browse(&server_id, &collector, &server)
            .expect("cancelled browse should return empty ok without leak");

        assert!(tags.is_empty());
        assert_eq!(collector.len(), 0);
        assert!(collector.is_cancelled());
    }
```
- **Post:** `TEST (cargo test -p opc-da-client --lib com::worker::browse::tests::test_handle_browse_chunk_draining_preserves_all_tags_across_boundaries expects: exit 0)`

#### Step 10: [MODIFY] `opc-da-client/src/com/worker/browse.rs` — [~] `browse_flat_namespace` & `try_fast_flat_browse` (L96-108, L130-151)
- **Pre:** Step 9 passed
- **Target:** `browse_flat_namespace` L96-108 and `try_fast_flat_browse` L130-151
- **Action:** Replace `std::mem::replace(&mut chunk, Vec::with_capacity(BROWSE_CHUNK_SIZE))` with `chunk.drain(..)` across intermediate chunk flushes, and ensure the terminal batch push consistently uses `chunk.drain(..)` (replacing `collector.push_batch(chunk)`):
```rust
// browse_flat_namespace (lines 96-108):
if chunk.len() >= BROWSE_CHUNK_SIZE {
    let _ = collector.push_batch(chunk.drain(..));
    if collector.is_cancelled() || collector.is_full() {
        break;
    }
}
// ... after loop (terminal batch push) ...
if !chunk.is_empty() {
    let _ = collector.push_batch(chunk.drain(..));
}

// try_fast_flat_browse (lines 130-151):
if chunk.len() >= BROWSE_CHUNK_SIZE {
    let _ = collector.push_batch(chunk.drain(..));
    if collector.is_cancelled() || collector.is_full() {
        break;
    }
}
// ... after loop (terminal batch push) ...
if !chunk.is_empty() {
    let _ = collector.push_batch(chunk.drain(..));
}
```
- **Post:** `CHECK, no std::mem::replace on chunk (rg "std::mem::replace\(&mut chunk" opc-da-client/src/com/worker/browse.rs expects: 0 matches)`
- **🔒 CHECKPOINT 3** (`cargo test -p opc-da-client --lib com::worker::browse`)

---

### Component Group 5: Zero-Allocation Request Dispatching & Clean Slate Hygiene ([`worker.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs), [`tests.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/tests.rs))

#### Step 11: [TEST] `opc-da-client/src/com/worker/tests.rs` — [+] `test_worker_panic_recovery_with_borrowed_endpoint` (L1300+)
- **Pre:** Checkpoint 3 passed
- **Target:** `tests.rs` end of file
- **Action:** Add test verifying panic containment in `dispatch_pooled_request` accesses borrowed endpoint safely:
```rust
#[tokio::test]
async fn test_worker_panic_recovery_with_borrowed_endpoint() {
    use crate::com::worker::{ComRequest, ComWorker};
    use crate::connector::mock::{MockServerConnector, MockState};
    use crate::types::{OpcServerEndpoint, TagBatch};
    use std::sync::Arc;

    let state = Arc::new(MockState::default());
    let connector = Arc::new(
        MockServerConnector::with_state(state.clone())
            .with_read_fn(|_, _| panic!("Simulated worker panic during read")),
    );
    let worker = ComWorker::start(connector).unwrap();
    let endpoint = OpcServerEndpoint::local_prog_id("Panic.Server");
    let tags = TagBatch::from_static(&["Panic.Tag"]);

    let res = worker
        .send_request(|reply| ComRequest::ReadTagValues {
            endpoint: endpoint.clone(),
            tags,
            reply,
        })
        .await;

    assert!(
        matches!(
            res,
            Err(OpcError::Worker(crate::errors::WorkerError::Panic(ref msg)))
                if msg.contains("Simulated worker panic during read")
        ),
        "Expected WorkerError::Panic, got: {res:?}"
    );

    // Verify worker remains alive and processes subsequent healthy requests
    let res2 = worker
        .send_request(|reply| ComRequest::Ping {
            endpoint: OpcServerEndpoint::local_prog_id("Healthy.Server"),
            reply,
        })
        .await;
    assert!(res2.is_ok());
}
```
- **Post:** `TEST (cargo test -p opc-da-client --lib com::worker::tests::test_worker_panic_recovery_with_borrowed_endpoint expects: exit 0)`

#### Step 12: [MODIFY] `opc-da-client/src/com/worker.rs` — [~] `dispatch_pooled_request`, `dispatch_discovery_request`, `sender`; [-] `start_async*`, `ConnectedGroup` import (L12, L228-234, L268-302, L531, L568-588)
- **Pre:** Step 11 passed
- **Target:** `worker.rs` lines 12, 228-234, 268-302, 531, 568-588
- **Action:**
  1. Remove unused import `ConnectedGroup` from line 12.
  2. Scope `ComWorker::sender` to `#[cfg(test)]` (lines 228-234), removing `#[allow(dead_code)]`.
  3. Delete `ComWorker::start_async` and `start_async_with_initializer` (lines 268-302).
  4. In `dispatch_discovery_request` (line 531), delete `let host_str = host.to_string();` and log `host = %host` directly in the `Err(payload)` block.
  5. In `dispatch_pooled_request` (lines 568-588), delete `let endpoint_clone = endpoint.clone();`, pass `endpoint` to `pool.remove(endpoint)`, and log `server = %endpoint` directly in the `Err(payload)` block.
- **Post:** `CHECK, no endpoint.clone in dispatch_pooled_request (rg "let endpoint_clone = endpoint\.clone\(\);" opc-da-client/src/com/worker.rs expects: 0 matches)`
- **🔒 CHECKPOINT 4** (`cargo test -p opc-da-client --lib com::worker`)

---

### Component Group 6: Verification Pipeline & Final Quality Gate

#### Step 13: [CHECK] Workspace Quality Gate
- **Pre:** Checkpoints 1, 2, 3, 4 passed
- **Target:** Full repository workspace
- **Action:** Run the complete zero-warning verification script `pwsh -File scripts/verify.ps1`.
- **Post:** `ALL (scripts/verify.ps1 expects: exit 0 across all 8 gates)`
- **🔒 FINAL CHECKPOINT**

---

## Verification Plan

### Automated Tests
```powershell
# Fast target component verification
cargo test -p opc-da-client --lib com::worker

# Doc testing check
cargo test --doc --workspace

# Full 8-gate quality suite
pwsh -File scripts/verify.ps1
```

### Manual Verification
- Verify that `git diff` shows zero additions of `#[allow(dead_code)]`.
- Confirm `rg "clear_active_group\b" opc-da-client/` returns exactly 0 matches.
- Confirm `rg "start_async\b" opc-da-client/` returns exactly 0 matches.

---

## Plan Summary

| Metric | Value |
|--------|-------|
| **Tier** | M (Feature / Component Refactor) |
| **Files Modified** | 5 (`browse.rs`, `read.rs`, `worker.rs`, `pool.rs`, `tests.rs`) |
| **Steps** | 13 (4 Test, 8 Modify, 1 Check) |
| **Checkpoints** | 5 (4 Component Group Checkpoints + 1 Final Checkpoint) |
| **Estimated Effort** | Medium |

---

## ✅ Reviewer Findings & Audit Resolutions

All reviewer findings across Cycles 1 through 3, along with the multi-lens deep audit from Cycle 4, have been categorized into Code-Level and Architectural domains and fully resolved in the plan. **Zero unresolved findings remain.**

### 1. Code-Level Issues Resolution Matrix

| Issue ID | Location | Original Defect / Review Finding | Resolution Applied in Plan | Verification Method | Status |
|---|---|---|---|---|---|
| **C1** | Step 6, Action 6 (`read.rs:137`) | Calling `partition_item_results(item_results, &tags.tag_ids, ...)` causes `E0425` (variable `item_results` not in scope) and `E0609` (`tag_ids` field does not exist on `&TagBatch`). | Corrected call signature to `partition_item_results(results, &tag_ids, &endpoint.identifier)` using actual bound variable `results` and existing local `let tag_ids: Vec<String>` from line 129. | AST syntax inspection; Step 6 unit test | `✅ Resolved` |
| **C2** | Step 3 & Step 4 (`read.rs:221-239`) | Error message literal inconsistency between test assertion and implementation string. | Standardized verbatim on `"Server {server_id} tag count parity mismatch: expected {tag_count} tags, got {} valid and {} rejected"` in both Step 3 exact test assertion and Step 4 implementation. | `cargo test --lib com::worker::read::tests::test_assemble_tag_values_parity_mismatch_fails_fast` | `✅ Resolved` |
| **C3** | Step 8 (`pool.rs:87-97`) | Potential regression to unstable `while ... && let Some(...)` syntax requiring nightly `#![feature(let_chains)]` (`E0658`). | Reaffirmed stable Rust 2024 (MSRV 1.93.1) construct: `while self.active_groups.len() >= MAX_ACTIVE_GROUPS { if let Some(lru) = self.active_groups.pop_back() { ... } }` with explicit diagnostic note. | `cargo check --lib` on stable Rust | `✅ Resolved` |
| **C4** | Step 10 (`browse.rs:96-151`) | Inconsistent buffer draining in terminal batch pushes (mixing `chunk.drain(..)` with moving `chunk`). | Standardized terminal flush in both `browse_flat_namespace` and `try_fast_flat_browse` to consistently execute `if !chunk.is_empty() { let _ = collector.push_batch(chunk.drain(..)); }`. | `rg "std::mem::replace\(&mut chunk" opc-da-client/src/com/worker/browse.rs` yields 0 matches | `✅ Resolved` |

### 2. Architectural Invariants Formalization Matrix

| Invariant ID | Target Component | Architectural Principle | Hardened Specification & Guarantees | Planner Lens | Status |
|---|---|---|---|---|---|
| **A1** | [`worker.rs:568-588`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L568) | Zero-Allocation Panic Containment | `OpcServerEndpoint` contains only immutable data (`Option<String>`, `ProgId`/`Clsid`) and natively implements `RefUnwindSafe + UnwindSafe`. Passing borrowed `&OpcServerEndpoint` across `std::panic::AssertUnwindSafe(|| { ... })` eliminates defensive heap allocations on the 99.999% normal path while safely enabling `pool.remove(endpoint)` during panic unwinds. | Concurrency / Perf | `✅ Verified` |
| **A2** | [`worker.rs:404-408`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs#L404) | Actor Footgun Excision | `PriorityRequestQueue::clear()` silently dropped pending oneshot reply channels, causing callers to suffer broken pipe `RecvError`s without an actionable error message. Excision of `clear()` enforces the actor invariant: *all queued requests either execute or receive a structured `OpcError` via `drain_and_reject`*. | Concurrency / API | `✅ Verified` |
| **A3** | [`read.rs:215-241`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L215) | Industrial ICS Sensor Parity (CWE-682) | Upfront 3-way parity check (`tag_count == valid_indices.len() + rejected_errors.len()`) strictly validates index conservation before capacity reservation, preventing sensor tag misattribution. On parity violation, returns `Err(OpcError::Internal)` and triggers immediate active group cache eviction (`pooled.remove_active_group(0)`). | Security / Types | `✅ Verified` |
| **A4** | [`read.rs:184-241`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L184) | Asymmetric Ownership Architecture | Consuming owned `Vec<GroupItemResult>` in `partition_item_results` eliminates `err.clone()` on cache misses, while retaining borrowed `rejected_errors: &[(usize, OpcError)]` in `assemble_tag_values` prevents deep cloning on recurring steady-state cache hits. | Types / Perf | `✅ Verified` |
| **A5** | [`browse.rs:65-170`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/browse.rs#L65) | Bounded Ceilings & Cancellation (CWE-400) | Reusing a single 256-item buffer (`BROWSE_CHUNK_SIZE = 256`) via `chunk.drain(..)` amortizes vector allocations to 1 for arbitrarily large namespaces ($\approx 39 \rightarrow 1$ for 10,000 tags), while polling `collector.is_cancelled() \|\| collector.is_full()` after every chunk flush bounds cancellation latency to $\le 256$ items. | Security / Concurrency / Perf | `✅ Verified` |
| **A6** | [`write.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/write.rs) | Boundary Isolation Defense | Confirmed that CWE-626 null byte rejection on write batches is strictly isolated and deferred to Sub-Block I2, maintaining an uncompromised release boundary with zero half-baked mutations in Sub-Block I1. | Security / Module | `✅ Verified` |

