# Cycle 2 Qualitative Architecture & Code Quality Review: Sub-Block H3b

> **Document Status:** Active Engineering Review Report & Planning Foundation  
> **Workspace:** `opc-cli`  
> **Target Crate:** `opc-da-client` (v0.2.0 $\rightarrow$ v0.3.0 Release Candidate)  
> **Evaluation Date:** 2026-09-17  
> **Sub-Block Focus:** Sub-Block H3b: Worker Active Group Caching & Batch Defense  
> **Reference Documents:** [`refactor/cycle2_blockH3_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockH3_review.md), [`refactor/cycle2_blockH_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockH_review.md)  
> **Review Scope:** Worker thread active group LRU cache, response casing assembly, and batch write execution safety (`src/com/worker/pool.rs`, `src/com/worker/read.rs`, `src/com/worker/write.rs`)  
> **Review Pipeline:** Decomposed Multi-Lens Audit across Logic, Design, Performance, Security, and API lenses  
> **User Interview Alignment & Consensuses:**
> 1. **Decomposition Mapping:** Approved Sub-Block H3b isolating Worker Active Group Caching & Batch Defense (Findings #1, #2, #5).
> 2. **Execution Ordering:** Strictly sequential — Sub-Block H3b executes immediately after Sub-Block H3a and precedes H3c.
> 3. **Active Group Matching Strategy:** Positional case-insensitive ASCII comparison (`a.eq_ignore_ascii_case(b)`) preserving strict $O(N)$ order-sensitive lookup.
> 4. **Response Casing Strategy:** Strictly preserve the caller's requested tag casing across both active group cache hits and cache misses via `tags.iter_str()` in `assemble_tag_values` to prevent non-deterministic casing and broken `HashMap` lookups.
> 5. **Resource Defense & Constant Consolidation:** Promote `MAX_TAG_BATCH_SIZE = 10_000` to `src/com/worker.rs` as a single canonical constant, enforcing symmetric diagnostic upper-bound error checks across both read and batch write pipelines.
> 6. **TDD Matrix:** Dedicated Red-Green test matrix included to guide implementation planning.

---

## 1. Executive Summary

Sub-Block H3b focuses on **eliminating COM IPC latency penalties, ensuring deterministic telemetry casing, and hardening worker thread execution loops** against Denial of Service.

Within the `ComWorker` background apartment, `PooledServer` manages a 4-slot LRU cache of active COM groups (`MAX_ACTIVE_GROUPS = 4`) to reuse registered item handles across polling cycles. Three critical defects were uncovered in this layer:
1. **Spurious Cache Misses & LRU Thrashing (Finding #1 - 🔴 Critical):** Active group cache matching evaluated byte-exact equality (`a == b`) on tag identifiers. Because OPC DA ItemIDs are case-insensitive, casing variations across polling cycles triggered false cache misses. Over remote DCOM, every miss incurred 2–3 synchronous Win32 COM round-trips (`AddGroup`, `AddItems`, `RemoveGroup`), introducing **$50\text{--}150\,\text{ms}$ of latency (25x–75x penalty)**, churning LRU slots, and risking server handle exhaustion (CWE-400).
2. **Non-Deterministic Response Tag Casing (Finding #2 - 🟠 Major):** On a cache hit, `handle_read` passed `&cached.tags` (the casing from the initial request that created the group) to `assemble_tag_values`. Returned `TagValue.tag_id` strings mutated depending on cache state, breaking caller `HashMap<String, TagValue>` lookups and causing TUI UI flicker.
3. **Unbounded Batch Writes (Finding #5 - 🟠 Major):** `handle_write_batch` lacked an upper-bound check (unlike `read` which bounds at 10,000 tags), allowing unbounded write batches to trigger Win32 RPC packet size violations, allocate massive buffers, or crash remote PLCs (CWE-400 / CWE-770).

Sub-Block H3b resolves these issues completely within `src/com/worker/` with zero public API breakage.

---

## 2. Scoped Files & Target Symbols

| File | Target Symbols | Role in Sub-Block H3b |
|:---|:---|:---|
| [`opc-da-client/src/com/worker.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker.rs) | `MAX_TAG_BATCH_SIZE` | Host canonical batch size limit constant (`10_000`) shared by read and write worker routines. |
| [`opc-da-client/src/com/worker/pool.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs) | `PooledServer::find_active_group_idx` | Upgrade tag matching from byte equality `a == b` to positional case-insensitive `a.eq_ignore_ascii_case(b)`. |
| [`opc-da-client/src/com/worker/read.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs) | `handle_read`<br>`assemble_tag_values` | Pass caller's requested tag string iterator (`tags.iter_str()`) on cache hits and misses, guaranteeing deterministic response tag casing. Reference `super::MAX_TAG_BATCH_SIZE`. |
| [`opc-da-client/src/com/worker/write.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/write.rs) | `handle_write_batch` | Enforce `super::MAX_TAG_BATCH_SIZE = 10_000` upper-bound check at batch write entrypoint. |

---

## 3. High-Level Objectives & Goals

| ID | Objective | Measurable Success Criteria |
|:---:|:---|:---|
| **O1** | Case-Insensitive Active Group Cache Hit | `PooledServer::find_active_group_idx` returns `Some(idx)` when queried with tags differing only in casing (e.g. `["TAG1"]` matches `["tag1"]`). |
| **O2** | Positional Handle Alignment & Integrity | Tag matching maintains strict $O(N)$ positional correspondence (`zip`), guaranteeing 1:1 alignment between registered COM handles and requested tags. |
| **O3** | Deterministic Response Tag Casing | On both cache hits and misses, every returned `TagValue.tag_id` exactly matches the casing of the current caller's `TagBatch` with zero extra allocations. |
| **O4** | Elimination of Redundant COM IPC Round-Trips | Polling with alternating tag casing produces 100% cache hits, eliminating 50–150ms DCOM latency penalties and COM group churn. |
| **O5** | Defensive Resource Bounding on Batch Writes | `handle_write_batch` rejects batches with $> 10_000$ items immediately with `OpcError::InvalidState`, preventing RPC buffer overflow and server DoS. |

---

## 4. Multi-Lens Qualitative Assessment Matrix

| Lens | Severity | Finding Anchor | Summary & Lens-Specific Impact |
|:---|:---:|:---|:---|
| **Performance** | 🔴 Critical | [`src/com/worker/pool.rs:64`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L64) | **25x–75x Latency Spike:** Byte-exact equality on OPC tags caused false cache misses in 4-slot LRU cache. Over remote DCOM, each miss required 2–3 blocking RPC round-trips (`AddGroup`, `AddItems`, `RemoveGroup`), inflating read times from 2ms to 50–150ms. |
| **Logic** | 🟠 Major | [`src/com/worker/read.rs:88`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L88) | **Non-Deterministic Response Tag Casing:** `assemble_tag_values` cloned tag names from `cached.tags` instead of current caller's `tags`. Tag casing flipped depending on cache state, breaking caller `map.get()` lookups and TUI display consistency. |
| **Security** | 🟠 Major | [`src/com/worker/write.rs:18`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/write.rs#L18)<br>[`src/com/worker/pool.rs:64`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L64) | **Server DoS (CWE-400 / CWE-770):** Unbounded batch writes allowed multi-megabyte RPC payloads that could exhaust client buffers or crash PLCs. Spurious cache misses caused rapid COM group churn, risking server-side handle exhaustion. |
| **Design** | 🟡 Minor | [`src/com/worker/read.rs:88`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L88) | **Iterator Decoupling:** Refactoring `assemble_tag_values` to accept `tags: impl Iterator<Item = &str>` decouples response construction from concrete storage (`Vec<String>` vs `TagBatch`), optimizing both allocation and encapsulation. |
| **API** | ⚪ Nitpick | [`src/com/worker/write.rs:18`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/write.rs#L18) | **Error Consistency:** Write batch size limit error message matches read batch format (`"Write batch size {len} exceeds maximum allowed limit of {MAX_TAG_BATCH_SIZE}"`), ensuring symmetric error diagnostics. |

---

## 5. Technical Deliverables & Implementation Contracts

### 5.1 Case-Insensitive Positional Cache Lookup ([`src/com/worker/pool.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs))
In `PooledServer<S>`, cached groups are stored as `self.active_groups: VecDeque<CachedGroup<S::Group>>`. We update `find_active_group_idx` to perform case-insensitive ASCII comparison across all tags positionally:
```rust
impl<S: ConnectedServer> PooledServer<S> {
    /// Searches for a cached active group (`CachedGroup<S::Group>`) whose tag list matches `tags`.
    pub(crate) fn find_active_group_idx(&self, tags: &TagBatch) -> Option<usize> {
        self.active_groups.iter().position(|g| {
            g.tags.len() == tags.len()
                && g.tags
                    .iter()
                    .zip(tags.iter_str())
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
        })
    }
}
```

### 5.2 Deterministic Response Assembly ([`src/com/worker/read.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs))
In `handle_read`, pass `tags.iter_str()` directly on BOTH cache hits (L88) and cache misses (L157), preserving caller casing uniformly across all execution branches:

**Cache Hit Call Site (L88):**
```rust
let (tag_values_res, should_clear) = if let Some(cached) = pooled.active_groups.front() {
    match assemble_tag_values(
        tags.iter_str(),
        tags.len(),
        &cached.valid_indices,
        &cached.rejected_errors,
        states,
        &endpoint.identifier,
    ) {
        Ok(vals) => (Ok(vals), false),
        Err(err) => (Err(err), true),
    }
} else {
    (
        Err(OpcError::Internal(
            "Active group unexpectedly missing".into(),
        )),
        false,
    )
};
```

**Cache Miss Call Site (L157):**
```rust
let tag_values = assemble_tag_values(
    tags.iter_str(),
    tags.len(),
    &valid_indices,
    &rejected_errors,
    item_states,
    &endpoint.identifier,
)?;
```

And update `assemble_tag_values` signature and implementation, preserving the critical array length parity check (`states.len() != valid_indices.len()`), per-item error logging, and direct `TagValue` field assignment:
```rust
pub(crate) fn assemble_tag_values<'a>(
    tags: impl Iterator<Item = &'a str>,
    tag_count: usize,
    valid_indices: &[usize],
    rejected_errors: &[(usize, OpcError)],
    item_states: Option<Vec<OpcResult<GroupItemState>>>,
    server_id: &ServerIdentifier,
) -> OpcResult<Vec<TagValue>> {
    let mut tag_values = Vec::with_capacity(tag_count);
    let states = item_states.unwrap_or_default();

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

    let mut state_iter = states.into_iter();
    let mut reject_iter = rejected_errors.iter().peekable();

    for (idx, tag_str) in tags.enumerate() {
        if let Some((rej_idx, err)) = reject_iter.peek()
            && *rej_idx == idx
        {
            let err = err.clone();
            reject_iter.next();
            tag_values.push(TagValue {
                tag_id: tag_str.to_string(),
                outcome: Err(err),
                quality: OpcQuality::BAD_CONFIG_ERROR,
                timestamp: None,
            });
        } else if let Some(state_res) = state_iter.next() {
            let (outcome, quality, timestamp) = match state_res {
                Ok(state) => (Ok(state.value), state.quality, Some(state.timestamp)),
                Err(e) => {
                    log_opc_err!(
                        &e,
                        "read_tag_values:per_item",
                        server = %server_id,
                        tag = %tag_str
                    );
                    (Err(e), OpcQuality::BAD_COMM_FAILURE, None)
                }
            };
            tag_values.push(TagValue {
                tag_id: tag_str.to_string(),
                outcome,
                quality,
                timestamp,
            });
        } else {
            return Err(OpcError::Internal(format!(
                "Unexpected state exhaustion at index {idx} on server {server_id}"
            )));
        }
    }

    Ok(tag_values)
}
```

### 5.3 Batch Write Resource Bounding ([`src/com/worker/write.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/write.rs))
```rust
pub fn handle_write_batch<S: ConnectedServer>(
    server_id: &ServerIdentifier,
    writes: &WriteBatch,
    opc_server: &S,
) -> OpcResult<Vec<WriteResult>> {
    if writes.is_empty() {
        return Ok(Vec::new());
    }

    if writes.len() > super::MAX_TAG_BATCH_SIZE {
        return Err(OpcError::InvalidState(format!(
            "Write batch size {} exceeds maximum allowed limit of {}",
            writes.len(),
            super::MAX_TAG_BATCH_SIZE
        )));
    }
    ...
}
```

---

## 6. Blast Radius Table

| Symbol | File | Direct Callers | Indirect Callers | Cross-Crate Impact | Risk Level |
|:---|:---|:---:|:---:|:---:|:---:|
| `MAX_TAG_BATCH_SIZE` | `src/com/worker.rs` | 3 (`read.rs`, `write.rs`, `read.rs` test) | 0 | None (internal `pub(crate)`) | 🟢 Low |
| `PooledServer::find_active_group_idx` | `src/com/worker/pool.rs` | 1 (`handle_read`) | 0 | None (internal `pub(crate)`) | 🟢 Low |
| `handle_read` | `src/com/worker/read.rs` | 1 (`execute_request`) | 0 | None (internal `pub(crate)`) | 🟢 Low |
| `assemble_tag_values` | `src/com/worker/read.rs` | 2 (`handle_read` hit + miss) | 0 | None (internal `pub(crate)`) | 🟢 Low |
| `handle_write_batch` | `src/com/worker/write.rs` | 1 (`execute_request`) | 0 | None (internal `pub(crate)`) | 🟢 Low |

---

## 7. Things to Watch Out For (Defensive Invariants)

1. **Strict Positional $O(N)$ Correspondence:**
   Do **NOT** implement set-based or order-independent tag matching. COM server item handles (`server_item_handles: Vec<ServerItemHandle>`) are returned by `IOPCItemMgt::AddItems` in the exact sequential order of the input registration slice. `valid_indices` maps handles positionally. Scrambling or sorting tags would corrupt telemetry mapping, returning Sensor A's values for Sensor B.
2. **Zero Allocation on Cache Hit Response Assembly:**
   By passing `tags.iter_str()` to `assemble_tag_values`, we borrow string slices directly from the incoming `TagBatch` (including 31-byte stack SSO `InlineSingle`). No heap vectors are allocated to stream tag names.
3. **Write Operation Non-Idempotency Preservation:**
   Batch writes remain strictly `RetryPolicy::NonIdempotent` in `pool.rs`. Connection drops during a write operation are never automatically retried to prevent state replay attacks on physical machinery.

---

## 8. TDD Red-Green Verification Matrix

| Test Case | Scope | Red Baseline | Green Success Criteria |
|:---|:---|:---|:---|
| `test_active_group_cache_hit_case_insensitive` | `src/com/worker/pool.rs` | Returns `None` when casing differs | `find_active_group_idx(&["temp1"])` matches cached `["TEMP1"]` and returns `Some(0)` |
| `test_active_group_cache_order_sensitive` | `src/com/worker/pool.rs` | N/A | Permuted tags `["B", "A"]` does NOT match cached `["A", "B"]`, returning `None` |
| `test_active_group_cache_length_mismatch` | `src/com/worker/pool.rs` | N/A | `["A"]` does NOT match `["A", "B"]`, returning `None` immediately without string scan |
| `test_assemble_tag_values_preserves_caller_casing` | `src/com/worker/read.rs` | Returns `TagValue.tag_id == "TAG1"` (cached) | Requesting `["tag1"]` against cached `["TAG1"]` returns `TagValue.tag_id == "tag1"` |
| `test_handle_write_batch_empty` | `src/com/worker/write.rs` | Passes | Returns `Ok([])` with zero COM calls |
| `test_handle_write_batch_exceeds_max_limit` | `src/com/worker/write.rs` | Attempts COM write allocation | Batch of 10,001 writes returns `Err(OpcError::InvalidState)` immediately |
| `test_handle_write_batch_at_max_limit` | `src/com/worker/write.rs` | Passes | Batch of 10,000 writes proceeds to server write execution |

---

📄 **Sub-Block Report:** [`refactor/cycle2_blockh3b_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockh3b_review.md)
