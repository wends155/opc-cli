# Task Checklist: Modernization Sub-Block I2 (Batch Write Allocation & Defensive Hardening)

> Tracking implementation progress for [`refactor/cycle2_blockI2_plan.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockI2_plan.md).

---

## Plan Objectives

| ID | Objective | Success Criteria |
|---|---|---|
| **O1** | **Eliminate Dead Store Allocations** | Happy-path 10k batch reduces heap allocations from 30,005 to 10,004 (-66.7%) and allocator ops from 50,005 to 10,004 (-80.0%) via lazy `Vec<Option<WriteResult>>` slot buffer. |
| **O2** | **Granular CWE-626 Null-Byte Defense** | Interior null bytes in tag IDs are isolated into `WriteResult::failure` during pre-registration partitioning with `tracing::warn!` telemetry; valid tags proceed to COM registration (prevents CWE-400 batch poisoning DoS). |
| **O3** | **Two-Stage Defensive Index Mapping** | Strict mathematical 1:1 positional correspondence between input writes and output results enforced via `valid_orig_indices` (Stage 1) and `valid_write_orig_indices` (Stage 2) using safe `.get()` slice indexing and `.zip()` pairing. |
| **O4** | **Decompose Monolithic Write Engine** | Refactor 112-line `handle_write_batch` into 3 cohesive helper functions (`partition_write_inputs`, `partition_item_registration_results`, `assemble_write_results`), removing `#[allow(clippy::too_many_lines)]`. |
| **O5** | **Ergonomic Domain Extensions** | Implement `Display` and `is_connection_error(&self) -> bool` on `WriteResult` with runnable `# Examples` doctests and complete rustdoc specifications. |
| **O6** | **Comprehensive Integration Testing** | Expand `tests/batch_write_test.rs` with multi-tag partial registration failures, partial write failures, null-byte isolation, all-null COM short-circuiting, and verification via `pwsh scripts/verify.ps1`. |

---

## Global Execution Order

### Phase 1: `WriteResult` Domain Ergonomics
- [x] **Step 1: [TEST]** `opc-da-client/src/types/write_batch.rs` — [+] `test_write_result_display_formatting`, `test_write_result_is_connection_error`
- [x] **Step 2: [MODIFY]** `opc-da-client/src/types/write_batch.rs` — [+] `<WriteResult as Display>::fmt`, `WriteResult::is_connection_error`
- [x] **🔒 CHECKPOINT 1**: Verify `WriteResult` unit tests (`cargo test -p opc-da-client --lib types::write_batch::tests`)

### Phase 2: Pipeline Helper Decomposition & Defense
- [x] **Step 3: [TEST]** `opc-da-client/src/com/worker/write.rs` — [+] `test_partition_write_inputs_clean_tags`, `test_partition_write_inputs_contaminated_tags`, `test_partition_write_inputs_all_null_tags`
- [x] **Step 4: [MODIFY]** `opc-da-client/src/com/worker/write.rs` — [+] `partition_write_inputs` with CWE-626 null-byte isolation and `tracing::warn!`
- [x] **Step 5: [TEST]** `opc-da-client/src/com/worker/write.rs` — [+] `test_partition_item_registration_results_partial`, `test_partition_item_registration_results_count_mismatch`
- [x] **Step 6: [MODIFY]** `opc-da-client/src/com/worker/write.rs` — [+] `partition_item_registration_results` with buffer size check and `.zip()` pairing
- [x] **Step 7: [TEST]** `opc-da-client/src/com/worker/write.rs` — [+] `test_assemble_write_results_outcomes`, `test_assemble_write_results_parity_mismatch`, `test_assemble_write_results_unassigned_slots_fallback`
- [x] **Step 8: [MODIFY]** `opc-da-client/src/com/worker/write.rs` — [+] `assemble_write_results` with server parity check and fail-safe slot fallback
- [x] **🔒 CHECKPOINT 2**: Verify all write helper unit tests (`cargo test -p opc-da-client --lib com::worker::write::tests`)

### Phase 3: Monolithic Orchestration Refactoring
- [x] **Step 9a: [TEST]** `opc-da-client/src/com/worker/write.rs` — [+] Unit tests for refactored write pipeline (`test_handle_write_batch_granular_null_byte_isolation`, `test_handle_write_batch_all_null_short_circuits`, `test_handle_write_batch_all_rejected_registration_skips_write`, `test_handle_write_scalar_null_byte_returns_failure_result`)
- [x] **Step 9b: [MODIFY]** `opc-da-client/src/com/worker/write.rs` — [~] Refactor `handle_write_batch` to 5-stage pipeline, remove `#[allow(clippy::too_many_lines)]`, delegate `handle_write`
- [x] **🔒 CHECKPOINT 3**: Verify worker write unit suite (`cargo test -p opc-da-client --lib com::worker::write::tests`)

### Phase 4: Integration Coverage & Quality Gate Verification
- [ ] **Step 10: [TEST+VERIFY]** `opc-da-client/tests/batch_write_test.rs` — [+] Add 6 integration tests (`test_client_batch_write_partial_rejection_preserves_order`, `test_client_batch_write_interior_null_byte_isolated`, `test_client_batch_write_empty_short_circuits`, `test_client_batch_write_all_rejected_skips_write`, `test_client_batch_write_all_null_short_circuits`, `test_write_result_display_and_connection_error`) and verify full workspace gate (`pwsh scripts/verify.ps1`)
- [ ] **🔒 FINAL CHECKPOINT**: Full workspace zero-exit gate

## Builder Notes
