# Sub-Block H3b: Worker Active Group Caching & Batch Defense

**Role:** Architect • **Date:** 2026-09-17 • **Tier:** M
**Scope:** Case-insensitive active group cache matching, deterministic response tag casing, and batch write resource bounding in `opc-da-client/src/com/worker/`

### Builder Context
Read before starting:
- `opc-da-client/src/com/worker.rs` L1-30 (module structure, request dispatch, `register_item_group` helper)
- `opc-da-client/src/com/worker/pool.rs` L25-68 (`CachedGroup` struct, `find_active_group_idx`, `active_groups` VecDeque)
- `opc-da-client/src/com/worker/read.rs` L15-40 (`MAX_TAG_BATCH_SIZE` constant, `handle_read` entrypoint); L80-165 (cache hit/miss branching); L217-285 (`assemble_tag_values` current implementation)
- `opc-da-client/src/com/worker/write.rs` L18-35 (`handle_write_batch` entrypoint, empty check)
- `opc-da-client/src/types/batch.rs` L213 (`TagBatch::iter_str` zero-alloc iterator); L327 (`ExactSizeIterator` impl)
- `opc-da-client/src/types/collection.rs` L422 (`TagValues::get` case-insensitive precedent)
- `opc-da-client/src/connector/mock/server.rs` L19 (`MockConnectedServer::group` is `Arc<MockConnectedGroup>`)
- `architecture.md` § Error Handling, § Toolchain
- `.agents/rules/coding-standard.md` (governance rules)

### Problem Statement

Three critical defects were identified in the Sub-Block H3b review ([cycle2_blockh3b_review.md](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockh3b_review.md)) within the `ComWorker` background apartment's active group caching layer:

1. **Spurious Cache Misses & LRU Thrashing (Finding #1 — 🔴 Critical):** [`PooledServer::find_active_group_idx`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L64) (pool.rs:64) evaluates byte-exact equality (`a == b`) on tag identifiers. Because OPC DA ItemIDs are case-insensitive per the OPC DA 2.05 specification, casing variations across polling cycles trigger false cache misses. Over remote DCOM, every miss incurs 2–3 synchronous Win32 COM round-trips (`AddGroup`, `AddItems`, `RemoveGroup`), introducing 50–150ms of latency (25x–75x penalty), churning the 4-slot LRU cache, and risking server handle exhaustion (CWE-400).

2. **Non-Deterministic Response Tag Casing (Finding #2 — 🟠 Major):** On a cache hit, [`handle_read`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/read.rs#L88) (read.rs:88) passes `&cached.tags` (the casing from initial group registration) to `assemble_tag_values`. The returned `TagValue.tag_id` strings mutate depending on cache state, breaking caller `HashMap<String, TagValue>` lookups and causing TUI UI flicker. The existing precedent in [`TagValues::get`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collection.rs#L422) (collection.rs:422) already uses case-insensitive matching, but the assembly path does not preserve caller casing.

3. **Unbounded Batch Writes (Finding #5 — 🟠 Major):** [`handle_write_batch`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/write.rs#L18) (write.rs:18) lacks an upper-bound check. Unlike `handle_read` which enforces `MAX_TAG_BATCH_SIZE = 10,000` tags (read.rs:15), unbounded write batches can trigger Win32 RPC packet size violations, allocate massive buffers, or crash remote PLCs (CWE-400 / CWE-770).

**Constraints:**
- All changes are `pub(crate)` internal — zero public API breakage, zero semver impact.
- TDD-first: failing tests written before implementation changes.
- 3 commits: one per finding.
- `MAX_TAG_BATCH_SIZE` promoted from `read.rs` to `worker.rs` as single canonical constant.
- Code-level fixes (eq_ignore_ascii_case, assemble_tag_values sig change, batch guard) delegated to subagent Builder; architectural decisions (constant promotion, cache-miss path redesign) handled by main agent.

**Dependencies:**
- Existing mock infrastructure: `MockConnectedServer`, `MockConnectedGroup` in `opc-da-client/src/connector/mock/`.
- `MockConnectedServer::group` field is `Arc<MockConnectedGroup>` — all test fixtures use `server.group.clone()`.
- Existing `TagBatch::iter_str()` returns `TagBatchIter` implementing `ExactSizeIterator` (batch.rs:327).
- Existing `TagBatch::from_static()` constructor for test inputs.

### Plan Objectives
| ID | Objective | Success Criteria | Steps |
|----|-----------|-----------------|-------|
| O1 | Case-Insensitive Active Group Cache Hit | `cargo test -p opc-da-client test_active_group_cache_hit_case_insensitive` passes; `rg "eq_ignore_ascii_case" opc-da-client/src/com/worker/pool.rs` returns ≥1 match | 1-5 |
| O2 | Deterministic Response Tag Casing | `cargo test -p opc-da-client test_handle_read_cache_hit_preserves_caller_casing` passes; `handle_read` passes `tags.iter_str()` on both hit and miss paths | 6-10 |
| O3 | Canonical Batch Size Constant | `rg "pub\(crate\) const MAX_TAG_BATCH_SIZE" opc-da-client/src/com/worker.rs` returns 1 match; `rg "const MAX_TAG_BATCH_SIZE" opc-da-client/src/com/worker/read.rs` returns 0 matches | 6-7 |
| O4 | Defensive Resource Bounding on Batch Writes | `cargo test -p opc-da-client test_handle_write_batch_exceeds_max_limit` passes; `rg "writes.len\(\) > MAX_TAG_BATCH_SIZE" opc-da-client/src/com/worker/write.rs` returns 1 match | 11-14 |
| O5 | Full Test Suite Green | `cargo test --workspace` exits 0 with all 442+ tests passing | 5, 10, 14 |

### Review History & Verdict
| Cycle | Reviewer | Model | Verdict | Adjustments Applied |
|---|---|---|---|---|
| 1 | `plan-reviewer` | `flash` | `⚠️ Revisions Recommended` | Initial Draft — 5 findings raised |
| 2 | `Architect (inline)` | `opus-4.6-thinking` | `✅ Approved` | Applied all 5 findings: (1) Fixed `Arc<MockConnectedGroup>` in test fixtures, (2) Added `use super::MAX_TAG_BATCH_SIZE;` import in `read.rs`/`write.rs`, (3) Restored `test_handle_read_cache_hit_preserves_caller_casing` as valid RED test, (4) Adopted `ExactSizeIterator` to eliminate redundant `tag_count`, (5) Aligned format string syntax |

### Negative Scope
**Out of Scope:**
- Do NOT modify public API signatures, traits, or error types.
- Do NOT modify `src/com/worker/browse.rs` or `src/com/worker/tests.rs`.
- Do NOT modify `src/types/batch.rs`, `src/types/collection.rs`, or any types crate files.
- Do NOT add new dependencies to `Cargo.toml`.
- Do NOT refactor `PooledServer` struct fields, `CachedGroup` struct definition, or LRU eviction logic.
- Do NOT modify `dispatch_with_retry` or `RetryPolicy` (non-idempotent write replay guard is preserved as-is).

### Interface Contracts

#### `assemble_tag_values` (signature change)
```rust
// BEFORE:
pub(crate) fn assemble_tag_values(
    tags: &[String],
    valid_indices: &[usize],
    rejected_errors: &[(usize, OpcError)],
    item_states: Option<Vec<OpcResult<GroupItemState>>>,
    server_id: &ServerIdentifier,
) -> OpcResult<Vec<TagValue>>

// AFTER:
pub(crate) fn assemble_tag_values(
    tags: impl ExactSizeIterator<Item = &str>,
    valid_indices: &[usize],
    rejected_errors: &[(usize, OpcError)],
    item_states: Option<Vec<OpcResult<GroupItemState>>>,
    server_id: &ServerIdentifier,
) -> OpcResult<Vec<TagValue>>
```

> [!IMPORTANT]
> `TagBatchIter` implements `ExactSizeIterator` (batch.rs:327), so `tags.len()` is available
> for `Vec::with_capacity()` without a separate `tag_count` parameter. This eliminates parameter
> desynchronization risk. Array `[&str; N]::into_iter()` also implements `ExactSizeIterator`,
> so test callers work unchanged.

**Invariants:**
- Array length parity check: `states.len() == valid_indices.len()` (fail-fast `OpcError::Internal`).
- Iterator exhaustion check: returns `OpcError::Internal` if state iterator depletes before tag iterator.
- Per-item error logging preserved via `log_opc_err!`.
- `TagValue.tag_id` always matches caller-provided casing via `tag_str.to_string()`.

#### `find_active_group_idx` (behavior change)
```rust
// BEFORE: g.tags.iter().zip(tags.iter_str()).all(|(a, b)| a == b)
// AFTER:  g.tags.iter().zip(tags.iter_str()).all(|(a, b)| a.eq_ignore_ascii_case(b))
```
**Invariants:**
- O(1) length pre-check (`g.tags.len() == tags.len()`) before string comparison.
- Strict positional O(N) correspondence via `zip` — NOT set-based matching.
- Zero heap allocation (borrows `&str` from TagBatch SSO buffer).

### Blast Radius Table
| Symbol | File | Direct Callers | Indirect Deps | Test-Only | Cross-Package? |
|--------|------|:---:|:---:|:---:|:---:|
| `MAX_TAG_BATCH_SIZE` | `src/com/worker.rs` | 4 (`read.rs:37`, `write.rs`, `read.rs:352` test, `write.rs` tests) | 0 | No | No |
| `PooledServer::find_active_group_idx` | `src/com/worker/pool.rs` | 7 (`read.rs:54`, pool.rs tests x6) | 1 (`execute_request`) | No | No |
| `assemble_tag_values` | `src/com/worker/read.rs` | 2 (`handle_read` hit L88, miss L157) | 1 (`execute_request`) | No | No |
| `handle_write_batch` | `src/com/worker/write.rs` | 3 (`worker.rs:692`, `handle_write` L138, `write.rs:224` test) | 1 (`OpcDaClient::write_batch`) | No | No |
| `handle_read` | `src/com/worker/read.rs` | 6 (`worker.rs:659`, read.rs tests x5) | 1 (`OpcDaClient::read_tags`) | No | No |
| `CachedGroup<G>` | `src/com/worker/pool.rs` | 3 (`pool.rs:47`, `read.rs:57` hit, `read.rs:168` miss) | 0 | No | No |

### Performance Constraints

| Operation | Algorithm | Time Complexity | Space Complexity | Trade-off |
|---|---|---|---|---|
| `find_active_group_idx` | Bounded 4-slot linear scan with O(1) length gate + positional ASCII case-folding | O(M) worst-case (M ≤ 10,000 tags; N ≤ 4 slots) | O(1) auxiliary | ~5-15 ns/tag CPU cost eliminates 50-150ms DCOM RPC misses |
| `assemble_tag_values` | Single-pass pre-allocated vector + synchronized peekable rejection | O(M) single traversal | O(M) output (single Vec allocation) | Borrows `&str` from TagBatch SSO; zero intermediate buffers |
| `handle_write_batch` guard | O(1) length comparison | O(1) | O(1) | Rejects before any COM allocation |

**Allocation Budget:**
- Cache hit path: Zero heap allocation for tag name streaming (borrows from TagBatch's 31-byte `InlineSingle` SSO).
- `assemble_tag_values`: Single `Vec::with_capacity(tags.len())` allocation + M `tag_str.to_string()` (unavoidable — `TagValue` owns `tag_id: String`).
- Write guard rejection: Zero allocation.

### Security Constraints

**Trust Boundary:** Client requests (`TagBatch`, `WriteBatch`) cross from the async API layer into the COM worker thread via `mpsc` channel. Input validation (batch size) occurs BEFORE any COM resource allocation.

| CWE | Threat | Mitigation |
|---|---|---|
| CWE-400/770 | Unbounded write batches exhaust RPC buffers / PLC memory | `writes.len() > MAX_TAG_BATCH_SIZE` guard at entrypoint |
| CWE-400 | Spurious cache misses churn COM group handles | Case-insensitive matching eliminates false misses |
| CWE-178 | Case-sensitive identifier comparison in case-insensitive protocol | `eq_ignore_ascii_case` aligns with OPC DA spec |
| CWE-294/841 | Write replay on connection drop | `RetryPolicy::NonIdempotent` preserved (out of scope, unchanged) |

### Edge Cases & Risks

1. **Positional O(N) Correspondence:** Tag matching via `zip` preserves 1:1 alignment between COM item handles and requested tags. Set-based or sorted matching would corrupt telemetry (Sensor A's values returned for Sensor B).
2. **Array Length Parity Guard:** `assemble_tag_values` returns `Err(OpcError::Internal)` if `states.len() != valid_indices.len()`, triggering `should_clear = true` in `handle_read` to evict corrupted groups.
3. **Iterator Exhaustion:** If state iterator depletes before tag iterator, `assemble_tag_values` returns `Err(OpcError::Internal)` with index context.
4. **Empty TagBatch:** `find_active_group_idx` with empty batch returns `None` (length mismatch with non-empty cached groups).
5. **Existing Test Compatibility:** Existing 6 tests in pool.rs that call `find_active_group_idx` use exact-case tags and will continue to pass because `eq_ignore_ascii_case` is a superset of exact equality.

### Test Plan (TDD)

**Test Coverage Matrix (11 test cases):**

| Target Symbol | Test Name | Test Type | Assertion |
|---|---|---|---|
| `find_active_group_idx` | `test_active_group_cache_hit_case_insensitive` | Unit | `["temp1"]` matches cached `["TEMP1"]` → `Some(0)` |
| `find_active_group_idx` | `test_active_group_cache_order_sensitive` | Unit | `["B", "A"]` does NOT match `["A", "B"]` → `None` |
| `find_active_group_idx` | `test_active_group_cache_length_mismatch` | Unit | `["A"]` does NOT match `["A", "B"]` → `None` |
| `find_active_group_idx` | `test_active_group_cache_empty_batch` | Unit | Empty batch does NOT match non-empty group → `None` |
| `assemble_tag_values` | `test_assemble_tag_values_preserves_caller_casing` | Unit | `["tag1"]` iterator produces `TagValue.tag_id == "tag1"` |
| `assemble_tag_values` | `test_assemble_tag_values_array_length_parity_mismatch` | Unit | Mismatched states/indices → `Err(OpcError::Internal)` |
| `assemble_tag_values` | `test_assemble_tag_values_mixed_valid_and_rejected` | Unit | Interleaved valid + rejected tags assembled correctly |
| `handle_read` | `test_handle_read_cache_hit_preserves_caller_casing` | Integration | Cache miss then case-different hit → caller casing preserved |
| `handle_write_batch` | `test_handle_write_batch_empty` | Unit | Empty batch → `Ok([])` with 0 COM calls |
| `handle_write_batch` | `test_handle_write_batch_exceeds_max_limit` | Unit | 10,001 writes → `Err(OpcError::InvalidState)` |
| `handle_write_batch` | `test_handle_write_batch_at_max_limit` | Unit | 10,000 writes proceeds to execution |

**Mock & Stub Registry:**
| Dependency | Mock | Trait | Location |
|---|---|---|---|
| OPC Server | `MockConnectedServer` | `ConnectedServer` | `src/connector/mock/server.rs` |
| OPC Group | `MockConnectedGroup` (via `Arc<MockConnectedGroup>`) | `ConnectedGroup` | `src/connector/mock/group.rs` |
| Tag Input | `TagBatch::from_static` | N/A | `src/types/batch.rs` |
| Write Input | `WriteBatch::empty`, `into_write_batch` | N/A | `src/types/write_batch.rs` |

### Global Execution Order

---
**COMMIT 1: Finding #1 — Case-Insensitive Active Group Cache Matching**

Step 1: [TEST] `opc-da-client/src/com/worker/pool.rs` — [+] `test_active_group_cache_hit_case_insensitive` (L413+)
- Pre: ALL
- Target: `pool::tests::test_active_group_cache_hit_case_insensitive`
- Action: Add unit test that creates a `PooledServer` with a `CachedGroup` containing tags `["TEMP1", "DEVICE.SPEED"]`, then searches with `TagBatch::from_static(&["temp1", "device.speed"])` and asserts `Some(0)`.
```rust
#[test]
fn test_active_group_cache_hit_case_insensitive() {
    use crate::types::handles::{ServerGroupHandle, ServerItemHandle};
    use crate::types::TagBatch;

    let server = MockConnectedServer::default();
    let mut pooled = PooledServer::new(server);

    pooled.insert_active_group(CachedGroup {
        tags: vec!["TEMP1".to_string(), "DEVICE.SPEED".to_string()],
        group: pooled.server.group.clone(),
        server_handle: ServerGroupHandle::new(1),
        server_item_handles: vec![ServerItemHandle::new(1), ServerItemHandle::new(2)],
        valid_indices: vec![0, 1],
        rejected_errors: Vec::new(),
    });

    let search_batch = TagBatch::from_static(&["temp1", "device.speed"]);
    assert_eq!(
        pooled.find_active_group_idx(&search_batch),
        Some(0),
        "find_active_group_idx must match cached group when tag names differ only in ASCII casing"
    );
}
```
- Post: RED(test_active_group_cache_hit_case_insensitive)

Step 2: [TEST] `opc-da-client/src/com/worker/pool.rs` — [+] `test_active_group_cache_order_sensitive` (L413+)
- Pre: CHECK
- Target: `pool::tests::test_active_group_cache_order_sensitive`
- Action: Add unit test verifying permuted tags `["TAG_B", "TAG_A"]` do NOT match cached `["TAG_A", "TAG_B"]` → returns `None`.
```rust
#[test]
fn test_active_group_cache_order_sensitive() {
    use crate::types::handles::{ServerGroupHandle, ServerItemHandle};
    use crate::types::TagBatch;

    let server = MockConnectedServer::default();
    let mut pooled = PooledServer::new(server);

    pooled.insert_active_group(CachedGroup {
        tags: vec!["TAG_A".to_string(), "TAG_B".to_string()],
        group: pooled.server.group.clone(),
        server_handle: ServerGroupHandle::new(1),
        server_item_handles: vec![ServerItemHandle::new(1), ServerItemHandle::new(2)],
        valid_indices: vec![0, 1],
        rejected_errors: Vec::new(),
    });

    let permuted_batch = TagBatch::from_static(&["TAG_B", "TAG_A"]);
    assert_eq!(
        pooled.find_active_group_idx(&permuted_batch),
        None,
        "Permuted tag batch must not match cached active group (matching must be strictly positional)"
    );
}
```
- Post: GREEN(test_active_group_cache_order_sensitive)

Step 3: [TEST] `opc-da-client/src/com/worker/pool.rs` — [+] `test_active_group_cache_length_mismatch` (L413+)
- Pre: CHECK
- Target: `pool::tests::test_active_group_cache_length_mismatch`
- Action: Add unit test verifying shorter/longer batches return `None`.
```rust
#[test]
fn test_active_group_cache_length_mismatch() {
    use crate::types::handles::{ServerGroupHandle, ServerItemHandle};
    use crate::types::TagBatch;

    let server = MockConnectedServer::default();
    let mut pooled = PooledServer::new(server);

    pooled.insert_active_group(CachedGroup {
        tags: vec!["TAG_A".to_string(), "TAG_B".to_string()],
        group: pooled.server.group.clone(),
        server_handle: ServerGroupHandle::new(1),
        server_item_handles: vec![ServerItemHandle::new(1), ServerItemHandle::new(2)],
        valid_indices: vec![0, 1],
        rejected_errors: Vec::new(),
    });

    let shorter = TagBatch::from_static(&["TAG_A"]);
    assert_eq!(pooled.find_active_group_idx(&shorter), None,
        "Shorter batch must not match longer cached group");
    let longer = TagBatch::from_static(&["TAG_A", "TAG_B", "TAG_C"]);
    assert_eq!(pooled.find_active_group_idx(&longer), None,
        "Longer batch must not match shorter cached group");
}
```
- Post: GREEN(test_active_group_cache_length_mismatch)

Step 4: [TEST] `opc-da-client/src/com/worker/pool.rs` — [+] `test_active_group_cache_empty_batch` (L413+)
- Pre: CHECK
- Target: `pool::tests::test_active_group_cache_empty_batch`
- Action: Add unit test verifying empty `TagBatch` returns `None`.
```rust
#[test]
fn test_active_group_cache_empty_batch() {
    use crate::types::handles::{ServerGroupHandle, ServerItemHandle};
    use crate::types::TagBatch;

    let server = MockConnectedServer::default();
    let mut pooled = PooledServer::new(server);

    pooled.insert_active_group(CachedGroup {
        tags: vec!["TAG_A".to_string()],
        group: pooled.server.group.clone(),
        server_handle: ServerGroupHandle::new(1),
        server_item_handles: vec![ServerItemHandle::new(1)],
        valid_indices: vec![0],
        rejected_errors: Vec::new(),
    });

    let empty = TagBatch::from_static(&[]);
    assert_eq!(pooled.find_active_group_idx(&empty), None,
        "Empty batch must not match non-empty cached active group");
}
```
- Post: GREEN(test_active_group_cache_empty_batch)

Step 5: [MODIFY] `opc-da-client/src/com/worker/pool.rs` — [~] `PooledServer::find_active_group_idx` (L64-68)
- Pre: RED(test_active_group_cache_hit_case_insensitive)
- Target: `find_active_group_idx`
- Action: Replace `a == b` with `a.eq_ignore_ascii_case(b)` in the positional tag comparison.
```rust
    pub(crate) fn find_active_group_idx(&self, tags: &TagBatch) -> Option<usize> {
        self.active_groups.iter().position(|g| {
            g.tags.len() == tags.len()
                && g.tags
                    .iter()
                    .zip(tags.iter_str())
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
        })
    }
```
- Post: GREEN(test_active_group_cache_hit_case_insensitive), ALL
- 🔒 CHECKPOINT: `git add -A && git commit -m "fix(worker): case-insensitive active group cache matching (Finding #1)"`

---
**COMMIT 2: Finding #2 — Deterministic Response Tag Casing + Constant Promotion**

Step 6: [MODIFY] `opc-da-client/src/com/worker.rs` — [+] `MAX_TAG_BATCH_SIZE` (L29+)
- Pre: ALL
- Target: Module-level constant in `worker.rs`
- Action: Add `pub(crate) const MAX_TAG_BATCH_SIZE: usize = 10_000;` to `worker.rs` after the `elapsed_ms` function.
```rust
/// Maximum number of tags or write items permitted in a single COM worker batch operation.
///
/// Batches exceeding this limit are rejected immediately with `OpcError::InvalidState`
/// before any COM resource allocation occurs.
pub(crate) const MAX_TAG_BATCH_SIZE: usize = 10_000;
```
- Post: CHECK

Step 7: [MODIFY] `opc-da-client/src/com/worker/read.rs` — [-] `MAX_TAG_BATCH_SIZE` (L15); [+] `use super::MAX_TAG_BATCH_SIZE;`
- Pre: CHECK
- Target: `read.rs` constant definition and all references
- Action:
  1. Remove `pub(crate) const MAX_TAG_BATCH_SIZE: usize = 10_000;` from read.rs:15.
  2. Add `use super::MAX_TAG_BATCH_SIZE;` at the top of the `use` block.
  3. All references (`MAX_TAG_BATCH_SIZE` at L37, L39 in `handle_read`, and L352 in tests via `use super::*;`) remain unqualified.
- Post: CHECK

Step 8: [TEST] `opc-da-client/src/com/worker/read.rs` — [+] `test_handle_read_cache_hit_preserves_caller_casing` (L288+)
- Pre: CHECK
- Target: `read::tests::test_handle_read_cache_hit_preserves_caller_casing`
- Action: Add integration test that performs a cache miss with `"TAG_A"` then a cache hit with `"tag_a"` (different casing), asserting the returned `TagValue.tag_id == "tag_a"` (caller casing preserved). This test compiles against the current `assemble_tag_values(&[String])` signature and FAILS at runtime because the cache hit path passes `&cached.tags` which returns `"TAG_A"`.

> [!NOTE]
> This is the valid TDD RED test for Finding #2. It compiles but fails at runtime on the
> assertion, satisfying the TDD protocol (RED = compilable runtime failure, not compile error).

```rust
#[test]
fn test_handle_read_cache_hit_preserves_caller_casing() {
    let server = MockConnectedServer::default();
    let mut pooled = PooledServer::new(server);
    let endpoint = OpcServerEndpoint::local_prog_id("Test.Server");

    // Miss: First read caches uppercase tag "TAG_A"
    let tags_upper = TagBatch::from_static(&["TAG_A"]);
    let res1 = handle_read(&endpoint, &tags_upper, &mut pooled).unwrap();
    assert_eq!(res1.get("TAG_A").unwrap().tag_id, "TAG_A");

    // Hit: Second read requests lowercase tag "tag_a" against cached "TAG_A"
    let tags_lower = TagBatch::from_static(&["tag_a"]);
    let res2 = handle_read(&endpoint, &tags_lower, &mut pooled).unwrap();
    let tag_val = res2
        .get("tag_a")
        .expect("TagValues must index using caller casing 'tag_a'");
    assert_eq!(
        tag_val.tag_id, "tag_a",
        "TagValue must preserve caller casing 'tag_a' rather than cached 'TAG_A'"
    );
}
```
- Post: RED(test_handle_read_cache_hit_preserves_caller_casing) — compiles but fails on `tag_val.tag_id == "tag_a"` assertion

Step 9: [MODIFY] `opc-da-client/src/com/worker/read.rs` — [~] `assemble_tag_values` (L217-285); [~] `handle_read` cache hit (L88) and cache miss (L157) call sites
- Pre: RED(test_handle_read_cache_hit_preserves_caller_casing)
- Target: `assemble_tag_values` signature + `handle_read` call sites
- Action:
  1. Change `assemble_tag_values` signature from `tags: &[String]` to `tags: impl ExactSizeIterator<Item = &str>`.
  2. Add `let tag_count = tags.len();` at function entry (before consuming the iterator).
  3. Replace `Vec::with_capacity(tags.len())` with `Vec::with_capacity(tag_count)`.
  4. Replace `for (idx, tag_id) in tags.iter().enumerate()` with `for (idx, tag_str) in tags.enumerate()`.
  5. Replace `tag_id.clone()` with `tag_str.to_string()` in TagValue construction.
  6. Update cache hit call site (L88): change `&cached.tags` to `tags.iter_str()`.
  7. Update cache miss call site (L157): change `&tag_ids` to `tags.iter_str()`.
  8. Preserve the array length parity guard, per-item error logging, and iterator exhaustion check.

```rust
pub(crate) fn assemble_tag_values(
    tags: impl ExactSizeIterator<Item = &str>,
    valid_indices: &[usize],
    rejected_errors: &[(usize, OpcError)],
    item_states: Option<Vec<OpcResult<GroupItemState>>>,
    server_id: &ServerIdentifier,
) -> OpcResult<Vec<TagValue>> {
    let tag_count = tags.len();
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

Cache hit call site update (L88):
```rust
match assemble_tag_values(
    tags.iter_str(),
    &cached.valid_indices,
    &cached.rejected_errors,
    states,
    &endpoint.identifier,
)
```

Cache miss call site update (L157):
```rust
let tag_values = assemble_tag_values(
    tags.iter_str(),
    &valid_indices,
    &rejected_errors,
    item_states,
    &endpoint.identifier,
)?;
```
- Post: GREEN(test_handle_read_cache_hit_preserves_caller_casing), CHECK

Step 10: [TEST] `opc-da-client/src/com/worker/read.rs` — [+] unit tests for `assemble_tag_values` invariants (L288+)
- Pre: CHECK
- Target: `read::tests::test_assemble_tag_values_preserves_caller_casing`, `test_assemble_tag_values_array_length_parity_mismatch`, `test_assemble_tag_values_mixed_valid_and_rejected`
- Action: Add 3 unit tests for caller casing preservation, array length parity mismatch error, and mixed valid/rejected tag assembly.
```rust
#[test]
fn test_assemble_tag_values_preserves_caller_casing() {
    use crate::connector::GroupItemState;
    use crate::types::{ClientItemHandle, OpcQuality, OpcValue, ServerIdentifier};

    let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
    let requested_tags = ["tag1", "mixedCase_Tag2"];
    let valid_indices = vec![0, 1];
    let rejected_errors = vec![];
    let states = Some(vec![
        Ok(GroupItemState {
            client_handle: ClientItemHandle::new(1),
            value: OpcValue::Int(42),
            quality: OpcQuality::GOOD,
            timestamp: std::time::SystemTime::UNIX_EPOCH,
        }),
        Ok(GroupItemState {
            client_handle: ClientItemHandle::new(2),
            value: OpcValue::String("val2".into()),
            quality: OpcQuality::GOOD,
            timestamp: std::time::SystemTime::UNIX_EPOCH,
        }),
    ]);

    let results = assemble_tag_values(
        requested_tags.into_iter(),
        &valid_indices,
        &rejected_errors,
        states,
        &server_id,
    )
    .expect("assemble_tag_values must succeed");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].tag_id, "tag1");
    assert_eq!(results[1].tag_id, "mixedCase_Tag2");
}

#[test]
fn test_assemble_tag_values_array_length_parity_mismatch() {
    use crate::connector::GroupItemState;
    use crate::types::{ClientItemHandle, OpcQuality, OpcValue, ServerIdentifier};

    let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
    let requested_tags = ["tag1", "tag2"];
    let valid_indices = vec![0, 1];
    let rejected_errors = vec![];
    let states = Some(vec![Ok(GroupItemState {
        client_handle: ClientItemHandle::new(1),
        value: OpcValue::Int(42),
        quality: OpcQuality::GOOD,
        timestamp: std::time::SystemTime::UNIX_EPOCH,
    })]);

    let err = assemble_tag_values(
        requested_tags.into_iter(),
        &valid_indices,
        &rejected_errors,
        states,
        &server_id,
    )
    .expect_err("array length parity mismatch must return error");

    assert!(
        matches!(err, OpcError::Internal(ref msg) if msg.contains("mismatched read result array size")),
        "Expected OpcError::Internal with mismatch details, got: {err:?}"
    );
}

#[test]
fn test_assemble_tag_values_mixed_valid_and_rejected() {
    use crate::connector::GroupItemState;
    use crate::types::{ClientItemHandle, OpcQuality, OpcValue, ServerIdentifier};

    let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
    let requested_tags = ["tag0", "tag1", "tag2"];
    let valid_indices = vec![0, 2];
    let rejected_errors = vec![(1, OpcError::InvalidState("Item rejected".into()))];
    let states = Some(vec![
        Ok(GroupItemState {
            client_handle: ClientItemHandle::new(0),
            value: OpcValue::Int(10),
            quality: OpcQuality::GOOD,
            timestamp: std::time::SystemTime::UNIX_EPOCH,
        }),
        Ok(GroupItemState {
            client_handle: ClientItemHandle::new(2),
            value: OpcValue::Int(20),
            quality: OpcQuality::GOOD,
            timestamp: std::time::SystemTime::UNIX_EPOCH,
        }),
    ]);

    let results = assemble_tag_values(
        requested_tags.into_iter(),
        &valid_indices,
        &rejected_errors,
        states,
        &server_id,
    )
    .expect("assembly of mixed valid and rejected tags must succeed");

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].tag_id, "tag0");
    assert!(results[0].outcome.is_ok());
    assert_eq!(results[1].tag_id, "tag1");
    assert!(results[1].outcome.is_err());
    assert_eq!(results[1].quality, OpcQuality::BAD_CONFIG_ERROR);
    assert_eq!(results[2].tag_id, "tag2");
    assert!(results[2].outcome.is_ok());
}
```
- Post: GREEN(test_assemble_tag_values_preserves_caller_casing), GREEN(test_assemble_tag_values_array_length_parity_mismatch), GREEN(test_assemble_tag_values_mixed_valid_and_rejected), ALL
- 🔒 CHECKPOINT: `git add -A && git commit -m "fix(worker): deterministic response tag casing + promote MAX_TAG_BATCH_SIZE (Finding #2)"`

---
**COMMIT 3: Finding #5 — Batch Write Resource Bounding**

Step 11: [MODIFY] `opc-da-client/src/com/worker/write.rs` — [+] `use super::MAX_TAG_BATCH_SIZE;`
- Pre: ALL
- Target: Module-level import in `write.rs`
- Action: Add `use super::MAX_TAG_BATCH_SIZE;` to the `use` block at the top of `write.rs`.
- Post: CHECK

Step 12: [TEST] `opc-da-client/src/com/worker/write.rs` — [+] `test_handle_write_batch_exceeds_max_limit` (L148+)
- Pre: CHECK
- Target: `write::tests::test_handle_write_batch_exceeds_max_limit`
- Action: Add unit test creating a `WriteBatch` of 10,001 items and asserting `Err(OpcError::InvalidState)` with zero COM calls.
```rust
#[test]
fn test_handle_write_batch_exceeds_max_limit() {
    let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
    let server = crate::connector::mock::MockConnectedServer {
        state: state.clone(),
        ..Default::default()
    };
    let server_id = ServerIdentifier::try_from("Test.Server").unwrap();

    let writes_vec: Vec<(String, OpcValue)> = (0..=MAX_TAG_BATCH_SIZE)
        .map(|i| {
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            (format!("Tag.{i}"), OpcValue::Int(i as i32))
        })
        .collect();
    let writes = writes_vec.into_write_batch();

    let err = handle_write_batch(&server_id, &writes, &server)
        .expect_err("batch exceeding limit must be rejected");
    assert!(
        matches!(err, OpcError::InvalidState(ref msg) if msg.contains("exceeds maximum allowed limit")),
        "Expected InvalidState error, got: {err:?}"
    );
    assert_eq!(
        state.add_group_count.load(std::sync::atomic::Ordering::Relaxed),
        0, "No COM group allocation should occur"
    );
}
```
- Post: RED(test_handle_write_batch_exceeds_max_limit)

Step 13: [MODIFY] `opc-da-client/src/com/worker/write.rs` — [~] `handle_write_batch` (L18-35)
- Pre: RED(test_handle_write_batch_exceeds_max_limit)
- Target: `handle_write_batch` entrypoint
- Action: Add batch size upper-bound validation after the empty check, using inline `{MAX_TAG_BATCH_SIZE}` format syntax for consistency with `read.rs`.
```rust
    if writes.len() > MAX_TAG_BATCH_SIZE {
        return Err(OpcError::InvalidState(format!(
            "Write batch size {} exceeds maximum allowed limit of {MAX_TAG_BATCH_SIZE}",
            writes.len(),
        )));
    }
```
- Post: GREEN(test_handle_write_batch_exceeds_max_limit), CHECK

Step 14: [TEST] `opc-da-client/src/com/worker/write.rs` — [+] `test_handle_write_batch_empty`, `test_handle_write_batch_at_max_limit` (L148+)
- Pre: CHECK
- Target: `write::tests::test_handle_write_batch_empty`, `write::tests::test_handle_write_batch_at_max_limit`
- Action: Add 2 unit tests for empty batch and boundary maximum (N = 10,000).
```rust
#[test]
fn test_handle_write_batch_empty() {
    let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
    let server = crate::connector::mock::MockConnectedServer {
        state: state.clone(),
        ..Default::default()
    };
    let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
    let writes = WriteBatch::empty();

    let results = handle_write_batch(&server_id, &writes, &server)
        .expect("empty write batch must succeed");
    assert!(results.is_empty());
    assert_eq!(
        state.add_group_count.load(std::sync::atomic::Ordering::Relaxed),
        0, "No COM groups must be allocated for empty batch"
    );
}

#[test]
fn test_handle_write_batch_at_max_limit() {
    let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
    let server = crate::connector::mock::MockConnectedServer {
        state: state.clone(),
        ..Default::default()
    };
    let server_id = ServerIdentifier::try_from("Test.Server").unwrap();

    let writes_vec: Vec<(String, OpcValue)> = (0..MAX_TAG_BATCH_SIZE)
        .map(|i| {
            #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
            (format!("Tag.{i}"), OpcValue::Int(i as i32))
        })
        .collect();
    let writes = writes_vec.into_write_batch();

    let results = handle_write_batch(&server_id, &writes, &server)
        .expect("batch at maximum limit must proceed");
    assert_eq!(results.len(), MAX_TAG_BATCH_SIZE);
    assert_eq!(
        state.add_group_count.load(std::sync::atomic::Ordering::Relaxed),
        1, "Exactly 1 COM group should be registered"
    );
}
```
- Post: GREEN(test_handle_write_batch_empty), GREEN(test_handle_write_batch_at_max_limit), ALL
- 🔒 CHECKPOINT: `git add -A && git commit -m "fix(worker): batch write resource bounding MAX_TAG_BATCH_SIZE (Finding #5)"`

### Verification Plan
| Type | Command |
|------|---------|
| Format | `cargo fmt --all -- --check` |
| Lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| Tests | `cargo test --workspace` |
| Doc Tests | `cargo test --doc --workspace` |
| Forbidden Patterns | `rg --color=never -n -g "*.rs" "\b(println!\|dbg!\|todo!)" opc-da-client/src/` |
| Constant Consolidation | `rg "pub\(crate\) const MAX_TAG_BATCH_SIZE" opc-da-client/src/com/worker.rs` (expects: 1 match) |
| Constant Removal | `rg "const MAX_TAG_BATCH_SIZE" opc-da-client/src/com/worker/read.rs` (expects: 0 matches) |
| Case-Insensitive | `rg "eq_ignore_ascii_case" opc-da-client/src/com/worker/pool.rs` (expects: ≥1 match) |
| Write Guard | `rg "writes.len\(\) > MAX_TAG_BATCH_SIZE" opc-da-client/src/com/worker/write.rs` (expects: 1 match) |

### Plan Summary
| Metric | Value |
|--------|-------|
| Tier | M |
| Files | 4 (`worker.rs`, `pool.rs`, `read.rs`, `write.rs`) |
| Steps | 14 |
| Checkpoints | 3 (one per commit/finding) |
| Tests Added | 11 |
| Estimated effort | Medium |
