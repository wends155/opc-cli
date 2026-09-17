# Task Checklist: Modernization Sub-Block I1 (COM Worker Hygiene & Buffer Reuse)

> Tracking implementation progress for `refactor/cycle2_blockI1_plan.md`.

---

## Plan Objectives

| ID | Objective | Success Criteria |
|---|---|---|
| **O1** | **Browse Buffer Reuse** | `chunk.drain(..)` replaces `std::mem::replace` across all flushes in `browse.rs`; unit tests prove zero tags lost over 600 items across multiple boundaries and bounded capacity is respected. |
| **O2** | **Read Error Ownership & Zero-Alloc Logging** | `partition_item_results` takes owned `results: Vec<GroupItemResult>`; zero `err.clone()` in partition; `error = %err` streams directly into tracing; upfront parity check fails fast on mismatch. |
| **O3** | **Zero-Alloc Hot-Path Dispatch** | `dispatch_pooled_request` and `dispatch_discovery_request` eliminate pre-unwind `endpoint.clone()` and `host.to_string()` allocations; panic recovery accesses borrowed data safely. |
| **O4** | **Clean Slate Excision & API Hygiene** | `start_async*`, `PriorityRequestQueue::clear`, `clear_active_group`, and `is_empty` deleted; `test_priority_request_queue_drop_drops_senders` validates `drop(queue)`. |
| **O5** | **Zero-Warning Workspace Quality Gate** | All 8 verification gates in `scripts/verify.ps1` pass with zero warnings, zero dead code attributes in worker modules, and zero test regressions. |

---

## Global Execution Order

### Component Group 1: Priority Request Queue Hygiene & Test Scaffolding
- [ ] **Step 1: [TEST]** `opc-da-client/src/com/worker/tests.rs` — [~] `test_priority_request_queue_drop_drops_senders` (L1017-1045)
- [ ] **Step 2: [MODIFY]** `opc-da-client/src/com/worker.rs` — [-] `PriorityRequestQueue::clear` (L404-408)

### Component Group 2: Read Engine Parity Validation & Error Move Semantics
- [ ] **Step 3: [TEST]** `opc-da-client/src/com/worker/read.rs` — [+] `test_assemble_tag_values_parity_mismatch_fails_fast` (L286+)
- [ ] **Step 4: [MODIFY]** `opc-da-client/src/com/worker/read.rs` — [~] `assemble_tag_values` (L221-241)
- [ ] **Step 5: [TEST]** `opc-da-client/src/com/worker/read.rs` — [~] `test_group_guard_disarm_on_read` (L341)
- [ ] **Step 6: [MODIFY]** `opc-da-client/src/com/worker/read.rs` — [~] `partition_item_results` & `handle_read` (L137, L184-210)
- [ ] **🔒 CHECKPOINT 1**: Verify read module tests pass (`cargo test -p opc-da-client --lib com::worker::read`)

### Component Group 3: Connection Pool Diagnostics & Group Management
- [ ] **Step 7: [TEST]** `opc-da-client/src/com/worker/pool.rs` — [+] `test_connection_pool_len_and_lifecycle` (L420+) & [~] `test_dispatch_with_retry_active_group_reuse` (L605)
- [ ] **Step 8: [MODIFY]** `opc-da-client/src/com/worker/pool.rs` — [~] `insert_active_group`, `evict`, `len`; [-] `clear_active_group`, `is_empty`, `CachedGroup allow(dead_code)` (L26, L87-95, L125-128, L240-251, L268)
- [ ] **🔒 CHECKPOINT 2**: Verify pool module tests pass (`cargo test -p opc-da-client --lib com::worker::pool`)

### Component Group 4: Browse Buffer Draining & Capacity Bounding
- [ ] **Step 9: [TEST]** `opc-da-client/src/com/worker/browse.rs` — [+] `test_handle_browse_chunk_draining_*` (L270+)
- [ ] **Step 10: [MODIFY]** `opc-da-client/src/com/worker/browse.rs` — [~] `browse_flat_namespace` & `try_fast_flat_browse` (L96-108, L130-151)
- [ ] **🔒 CHECKPOINT 3**: Verify browse module tests pass (`cargo test -p opc-da-client --lib com::worker::browse`)

### Component Group 5: Zero-Allocation Request Dispatching & Clean Slate Hygiene
- [ ] **Step 11: [TEST]** `opc-da-client/src/com/worker/tests.rs` — [+] `test_worker_panic_recovery_with_borrowed_endpoint` (L1300+)
- [ ] **Step 12: [MODIFY]** `opc-da-client/src/com/worker.rs` — [~] `dispatch_pooled_request`, `dispatch_discovery_request`, `sender`; [-] `start_async*`, `ConnectedGroup` import (L12, L228-234, L268-302, L531, L568-588)
- [ ] **🔒 CHECKPOINT 4**: Verify all worker tests pass (`cargo test -p opc-da-client --lib com::worker`)

### Component Group 6: Verification Pipeline & Final Quality Gate
- [ ] **Step 13: [CHECK]** Workspace Quality Gate — Full repository verification (`pwsh -File scripts/verify.ps1`)
- [ ] **🔒 FINAL CHECKPOINT**: Full workspace zero-exit gate
