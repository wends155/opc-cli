# Implementation Plan: Sub-Block I2 — Batch Write Allocation & Defensive Hardening

| Field | Detail |
|:---|:---|
| **Role** | Architect |
| **Date** | 2026-09-18 |
| **Scope** | `opc-da-client/src/com/worker/write.rs`, `opc-da-client/src/types/write_batch.rs`, `opc-da-client/tests/batch_write_test.rs` |
| **Tier** | S-Tier (3 files: types/write_batch.rs, com/worker/write.rs, tests/batch_write_test.rs) |

## Review History & Verdict

| Cycle | Reviewer Verdict | Key Findings / Action Items | Status |
|:---|:---|:---|:---|
| 1 | `⚠️ Revisions Recommended` | Missing full test bodies in GEO, missing IPR header & objectives table, missing null-byte quarantine tracing, missing `# Examples` on `is_connection_error`, missing buffer size guards. | Resolved |
| 2 | `⚠️ Revisions Recommended` | Observability regression in `handle_write_batch` (stripped tracing/timer), prose test lists in Steps 9 & 10, tier discrepancy (M vs S), unchecked slice indexing in `partition_item_registration_results`, missing `# Errors` doc comments. | Resolved |
| 3 | `✅ Approved` | All Cycle 2 findings resolved with exact verified code snippets. Plan approved for implementation. | Approved |

## Plan Objectives

| ID | Objective | Success Criteria | Steps |
|:---|:---|:---|:---|
| `O1` | **Eliminate Dead Store Allocations** | Happy-path 10k batch reduces heap allocations from 30,005 to 10,004 (-66.7%) and allocator ops from 50,005 to 10,004 (-80.0%) via lazy `Vec<Option<WriteResult>>` slot buffer. | Steps 3–8 |
| `O2` | **Granular CWE-626 Null-Byte Defense** | Interior null bytes in tag IDs are isolated into `WriteResult::failure` during pre-registration partitioning with `tracing::warn!` telemetry; valid tags proceed to COM registration (prevents CWE-400 batch poisoning DoS). | Steps 3, 4, 9a, 9b, 10 |
| `O3` | **Two-Stage Defensive Index Mapping** | Strict mathematical 1:1 positional correspondence between input writes and output results enforced via `valid_orig_indices` (Stage 1) and `valid_write_orig_indices` (Stage 2) using safe `.get()` slice indexing and `.zip()` pairing. | Steps 5–8 |
| `O4` | **Decompose Monolithic Write Engine** | Refactor 112-line `handle_write_batch` into 3 cohesive helper functions (`partition_write_inputs`, `partition_item_registration_results`, `assemble_write_results`), removing `#[allow(clippy::too_many_lines)]`. | Steps 3–9b |
| `O5` | **Ergonomic Domain Extensions** | Implement `Display` and `is_connection_error(&self) -> bool` on `WriteResult` with runnable `# Examples` doctests and complete rustdoc specifications. | Steps 1, 2 |
| `O6` | **Comprehensive Integration Testing** | Expand `tests/batch_write_test.rs` with multi-tag partial registration failures, partial write failures, null-byte isolation, all-null COM short-circuiting, and verification via `pwsh scripts/verify.ps1`. | Step 10 |

---

## 1. Executive Summary & Blast Radius
- **Domain Scope:** Modernization of the batch write execution engine in `opc-da-client/src/com/worker/write.rs`, ergonomic enhancements to `opc-da-client/src/types/write_batch.rs`, and comprehensive integration testing in `opc-da-client/tests/batch_write_test.rs`.
- **Target Files:**
  1. `[MODIFY]` `opc-da-client/src/types/write_batch.rs` (Display impl, `is_connection_error(&self) -> bool`)
  2. `[MODIFY]` `opc-da-client/src/com/worker/write.rs` (decomposition into 3 pipeline helpers, lazy slot mapping, granular CWE-626 quarantine, removal of `#[allow(clippy::too_many_lines)]`, internal unit tests)
  3. `[MODIFY]` `opc-da-client/tests/batch_write_test.rs` (expansion with multi-tag partial failures, interior null byte isolation, all-null short-circuits)
- **Problem Statement:**
  1. Monolithic function: `handle_write_batch` spans 112 lines with `#[allow(clippy::too_many_lines)]` mixing 5 concerns.
  2. Dead store allocations: Pre-populating `write_results` with dummy `WriteResult::failure` strings allocates and frees 20,000 throwaway heap strings per 10k batch on the happy path.
  3. Redundant intermediate vectors: Allocates both `items: Vec<(&str, &OpcValue)>` and `tag_names: Vec<&str>` (160 KB heap churn for 10k items).
  4. Cascading batch abort on null bytes (CWE-626): If a single tag contains `\0`, `register_item_group` fails the entire batch via `?`, aborting up to 9,999 legitimate PLC setpoint actuations (industrial batch poisoning DoS, CWE-400).
  5. Missing ergonomics: `WriteResult` lacks `Display` and `is_connection_error`, unlike `TagValue`.
  6. Integration testing gaps: `tests/batch_write_test.rs` only tests simple 2-tag happy path, missing partial registration failure, partial write failure, null byte isolation, and short-circuiting.
- **Blast Radius:** Narrow and strictly contained. Zero breaking changes to public facades (`OpcDaClient::write_tag`, `write_tags`, `write_tag_value`, `write_tag_batch`). `WriteResult` changes are strictly additive (`Display` and `is_connection_error`).

---

## 2. Architecture Blueprint & Pipeline Flow
The batch write engine is decomposed into a clean 5-stage pipeline with 3 dedicated single-responsibility helpers:
1. **Gate 0: Resource Limits & Empty Short-Circuit**
   - `writes.is_empty()` -> `Ok(Vec::new())` (zero allocation).
   - `writes.len() > MAX_TAG_BATCH_SIZE (10,000)` -> `Err(OpcError::InvalidState(...))` (zero allocation).
2. **Gate 1: Input Partitioning (`partition_write_inputs`)**
   - Single linear scan over `writes.iter()`.
   - Tags with interior `\0` quarantined immediately into `write_results[orig_idx] = Some(WriteResult::failure(tag_id, InvalidState("Tag identifier contains illegal interior null byte")))` with `tracing::warn!` telemetry.
   - Clean tags collected into `valid_tags: Vec<&'a str>` with Stage 1 index mapping `valid_orig_indices: Vec<usize>`.
   - Lazy slot buffer initialized: `write_results: Vec<Option<WriteResult>> = vec![None; items.len()]` (0 string allocations on happy path).
   - Short-circuit: If `valid_tags.is_empty()`, directly invoke `assemble_write_results` (0 COM ephemeral groups created).
3. **Stage 2: Ephemeral Group Registration & Item Addition**
   - Call `crate::com::worker::register_item_group(opc_server, server_id, "opc-write", &valid_tags)` protected by RAII `GroupGuard`.
4. **Stage 3: Registration Result Partitioning (`partition_item_registration_results`)**
   - Enforce buffer size precondition: `write_results.len() == items.len()`.
   - Enforce registration array parity: `results.len() == valid_orig_indices.len()`.
   - Pair results via `results.into_iter().zip(valid_orig_indices)`.
   - Items with `item_res.error` recorded directly into `write_results[orig_idx] = Some(WriteResult::failure(tag_id, err))`.
   - Valid items paired with server handle and cloned value into `valid_writes: Vec<ItemWrite>` with Stage 2 index mapping `valid_write_orig_indices: Vec<usize>`.
5. **Stage 4: Synchronous COM Write Dispatch**
   - If `!valid_writes.is_empty()`, invoke `reg.group.write(&valid_writes)?`.
   - If `valid_writes.is_empty()` (all items rejected in registration), bypass `group.write()` completely (`server_write_results = None`).
6. **Stage 5: Result Assembly & Final Resolution (`assemble_write_results`)**
   - Enforce buffer size precondition: `write_results.len() == items.len()`.
   - If `server_write_results` present, verify parity: `server_res.len() == valid_write_orig_indices.len()`.
   - Map write outcomes via `valid_write_orig_indices[write_idx] -> orig_idx`:
     - `Ok(())` -> `Some(WriteResult::success(tag_id))`
     - `Err(e)` -> `Some(WriteResult::failure(tag_id, e))`
   - Fallback unassigned slot resolution: `opt.unwrap_or_else(|| WriteResult::failure(tag_id, OpcError::Internal(format!("Write result slot for tag '{tag_id}' was not populated by server {server_id}"))))`.
   - Return `Ok(Vec<WriteResult>)` with exact 1:1 caller ordering and length.

---

## 3. Interface Contracts & Types

### 3.1 Public Additions to `WriteResult` (`src/types/write_batch.rs`)
```rust
impl WriteResult {
    /// Returns `true` if the write operation failed with a transport or connection-level error.
    ///
    /// # Details
    ///
    /// Checks whether the contained error (if any) represents a connection-related failure,
    /// such as server disconnection, RPC failure, or dead connection handle. Returns `false`
    /// if the write succeeded or failed due to an item-level, configuration, or data validation error.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcError, WriteResult};
    ///
    /// let conn_err = WriteResult::failure("Tag1", OpcError::Connection("Lost".into()));
    /// assert!(conn_err.is_connection_error());
    ///
    /// let state_err = WriteResult::failure("Tag2", OpcError::InvalidState("Bad tag".into()));
    /// assert!(!state_err.is_connection_error());
    ///
    /// let ok_res = WriteResult::success("Tag3");
    /// assert!(!ok_res.is_connection_error());
    /// ```
    ///
    /// # Panics
    ///
    /// This function does not panic.
    #[must_use]
    pub fn is_connection_error(&self) -> bool {
        self.error().is_some_and(OpcError::is_connection_error)
    }
}

impl std::fmt::Display for WriteResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.status {
            Ok(()) => write!(f, "Write '{}': succeeded", self.tag_id),
            Err(e) => write!(f, "Write '{}': failed ({e})", self.tag_id),
        }
    }
}
```

### 3.2 Internal Pipeline Helper Signatures (`src/com/worker/write.rs`)
```rust
pub(crate) fn partition_write_inputs<'a>(
    items: &[(&'a str, &OpcValue)],
    server_id: &ServerIdentifier,
) -> (Vec<&'a str>, Vec<usize>, Vec<Option<WriteResult>>)

pub(crate) fn partition_item_registration_results(
    results: Vec<crate::connector::GroupItemResult>,
    valid_orig_indices: &[usize],
    items: &[(&str, &OpcValue)],
    write_results: &mut [Option<WriteResult>],
    server_id: &ServerIdentifier,
) -> OpcResult<(Vec<crate::connector::ItemWrite>, Vec<usize>)>

pub(crate) fn assemble_write_results(
    items: &[(&str, &OpcValue)],
    mut write_results: Vec<Option<WriteResult>>,
    valid_write_orig_indices: &[usize],
    server_write_results: Option<Vec<OpcResult<()>>>,
    server_id: &ServerIdentifier,
) -> OpcResult<Vec<WriteResult>>
```

### 3.3 Public Worker Functions (`src/com/worker/write.rs`)
```rust
pub fn handle_write_batch<S: ConnectedServer>(
    server_id: &ServerIdentifier,
    writes: &WriteBatch,
    opc_server: &S,
) -> OpcResult<Vec<WriteResult>>

pub fn handle_write<S: ConnectedServer>(
    server_id: &ServerIdentifier,
    tag_id: &str,
    value: &OpcValue,
    opc_server: &S,
) -> OpcResult<WriteResult>
```

---

## 4. Security & Safety Controls
- **CWE-770 (Uncontrolled Resource Consumption):** Bounded batch ceiling `MAX_TAG_BATCH_SIZE = 10,000` enforced at Gate 0 before any heap allocations or COM calls.
- **CWE-626 (Interior Null Byte Injection):** Inverted validation from unmanaged COM FFI to boundary entry. Tags with `\0` are quarantined as `WriteResult::failure` and never cross into `PWSTR`/`BSTR`.
- **CWE-400 (Denial of Service via Batch Poisoning):** Granular defensive isolation ensures contaminated tags do not cascade-fail legitimate physical setpoint writes.
- **Zero COM Ephemeral Allocation on All-Invalid Batch:** When 100% of tags contain null bytes, COM group creation is short-circuited.
- **CWE-125 / CWE-787 (Out-of-Bounds Indexing & IDOR Attribution):** Banned unchecked indexing; accesses use `.get()` and `.zip()`. Strict two-stage index mapping (`valid_orig_indices`, `valid_write_orig_indices`) guarantees exact 1:1 caller ordering.
- **Server Result Array Parity:** Verifies server returns exactly the expected number of results before `.zip()`, failing fast with `OpcError::Internal("server returned mismatched write result array size: expected ..., got ...")` to protect invariant assertion in `src/com/worker/tests.rs:1011`.
- **Fail-Safe Slot Fallback:** Any unassigned slot resolves via `unwrap_or_else` to `WriteResult::failure(tag_id, OpcError::Internal(...))`.

---

## 5. Performance & Allocation Bounds
- **Quantitative Allocation Scaling ($N = 10{,}000$ items, happy path):**
  - Old: 30,005 allocations, 20,000 deallocations (50,005 allocator ops).
  - New: 10,004 allocations, 0 dead deallocations (10,004 allocator ops).
  - Net Reduction: -66.7% allocations, -80.0% total allocator operations.
- **Memory Footprint:** Intermediate `tag_names: Vec<&str>` vector eliminated (-160 KB buffer churn).
- **ExactSizeIterator Guarantee:** Note: `WriteBatchIter` implements `ExactSizeIterator`, ensuring `writes.iter().collect()` pre-allocates exact capacity via `Vec::with_capacity(writes.len())` without amortized reallocation.
- **Zero-Copy Borrowing:** `items: Vec<(&'a str, &'a OpcValue)>` borrows slices and values; only surviving items clone values for COM dispatch.
- **Algorithmic Complexity:** $O(N)$ linear time complexity throughout all 5 pipeline stages.

---

## 6. Global Execution Order (GEO)
Strictly sequenced in TDD order (Tests precede modifications), with pre/post checks and Git checkpoints:

- **Step 1: [TEST] Unit tests for `WriteResult` `Display` and `is_connection_error`**
  - File: `opc-da-client/src/types/write_batch.rs`
  - Pre-check: `cargo check -p opc-da-client` passes.
  - Action: Add `test_write_result_display_formatting` and `test_write_result_is_connection_error` in `tests` module:
    ```rust
    #[test]
    fn test_write_result_display_formatting() {
        let ok = WriteResult::success("Channel.Device.Tag1");
        assert_eq!(format!("{ok}"), "Write 'Channel.Device.Tag1': succeeded");

        let err = WriteResult::failure(
            "Channel.Device.Tag2",
            OpcError::InvalidState("Tag not found".into()),
        );
        assert_eq!(
            format!("{err}"),
            "Write 'Channel.Device.Tag2': failed (Invalid state: Tag not found)"
        );
    }

    #[test]
    fn test_write_result_is_connection_error() {
        let conn_err = WriteResult::failure(
            "Channel.Device.Tag1",
            OpcError::Connection("Server unreachable".into()),
        );
        assert!(conn_err.is_connection_error());

        let state_err = WriteResult::failure(
            "Channel.Device.Tag2",
            OpcError::InvalidState("Access denied".into()),
        );
        assert!(!state_err.is_connection_error());

        let ok_res = WriteResult::success("Channel.Device.Tag3");
        assert!(!ok_res.is_connection_error());
    }
    ```
  - Post-check: `cargo test -p opc-da-client --lib types::write_batch::tests` fails (RED).

- **Step 2: [MODIFY] Implement `Display` and `is_connection_error` on `WriteResult`**
  - File: `opc-da-client/src/types/write_batch.rs`
  - Pre-check: Step 1 fails with missing method / trait error.
  - Action: Add implementation with full doctests:
    ```rust
    impl WriteResult {
        /// Returns `true` if the write operation failed with a transport or connection-level error.
        ///
        /// # Details
        ///
        /// Checks whether the contained error (if any) represents a connection-related failure,
        /// such as server disconnection, RPC failure, or dead connection handle. Returns `false`
        /// if the write succeeded or failed due to an item-level, configuration, or data validation error.
        ///
        /// # Examples
        ///
        /// ```
        /// use opc_da_client::{OpcError, WriteResult};
        ///
        /// let conn_err = WriteResult::failure("Tag1", OpcError::Connection("Lost".into()));
        /// assert!(conn_err.is_connection_error());
        ///
        /// let state_err = WriteResult::failure("Tag2", OpcError::InvalidState("Bad tag".into()));
        /// assert!(!state_err.is_connection_error());
        ///
        /// let ok_res = WriteResult::success("Tag3");
        /// assert!(!ok_res.is_connection_error());
        /// ```
        ///
        /// # Panics
        ///
        /// This function does not panic.
        #[must_use]
        pub fn is_connection_error(&self) -> bool {
            self.error().is_some_and(OpcError::is_connection_error)
        }
    }

    impl std::fmt::Display for WriteResult {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match &self.status {
                Ok(()) => write!(f, "Write '{}': succeeded", self.tag_id),
                Err(e) => write!(f, "Write '{}': failed ({e})", self.tag_id),
            }
        }
    }
    ```
  - Post-check: `cargo test -p opc-da-client --lib types::write_batch::tests` passes (GREEN).
  - Checkpoint: `git commit -m "feat(types): implement Display and is_connection_error for WriteResult"`

- **Step 3: [TEST] Unit tests for `partition_write_inputs`**
  - File: `opc-da-client/src/com/worker/write.rs`
  - Pre-check: `cargo check -p opc-da-client` passes.
  - Action: Add unit tests:
    ```rust
    #[test]
    fn test_partition_write_inputs_clean_tags() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val1 = OpcValue::Int(10);
        let val2 = OpcValue::Int(20);
        let items = [("Tag1", &val1), ("Tag2", &val2)];

        let (valid_tags, valid_orig_indices, write_results) =
            partition_write_inputs(&items, &server_id);

        assert_eq!(valid_tags, vec!["Tag1", "Tag2"]);
        assert_eq!(valid_orig_indices, vec![0, 1]);
        assert_eq!(write_results.len(), 2);
        assert!(write_results[0].is_none());
        assert!(write_results[1].is_none());
    }

    #[test]
    fn test_partition_write_inputs_contaminated_tags() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val1 = OpcValue::Int(10);
        let val2 = OpcValue::Int(20);
        let val3 = OpcValue::Int(30);
        let items = [
            ("Clean.Tag1", &val1),
            ("Dirty\0.Tag2", &val2),
            ("Clean.Tag3", &val3),
        ];

        let (valid_tags, valid_orig_indices, write_results) =
            partition_write_inputs(&items, &server_id);

        assert_eq!(valid_tags, vec!["Clean.Tag1", "Clean.Tag3"]);
        assert_eq!(valid_orig_indices, vec![0, 2]);
        assert_eq!(write_results.len(), 3);
        assert!(write_results[0].is_none());
        assert!(write_results[2].is_none());

        let quarantined = write_results[1]
            .as_ref()
            .expect("quarantined result must be present");
        assert_eq!(quarantined.tag_id, "Dirty\0.Tag2");
        assert!(quarantined.is_error());
        assert!(
            quarantined
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
    }

    #[test]
    fn test_partition_write_inputs_all_null_tags() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val1 = OpcValue::Int(10);
        let val2 = OpcValue::Int(20);
        let items = [("Bad\0One", &val1), ("Bad\0Two", &val2)];

        let (valid_tags, valid_orig_indices, write_results) =
            partition_write_inputs(&items, &server_id);

        assert!(valid_tags.is_empty());
        assert!(valid_orig_indices.is_empty());
        assert_eq!(write_results.len(), 2);
        let res0 = write_results[0].as_ref().unwrap();
        assert_eq!(res0.tag_id, "Bad\0One");
        assert!(res0.is_error());
        assert!(
            res0.error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
        let res1 = write_results[1].as_ref().unwrap();
        assert_eq!(res1.tag_id, "Bad\0Two");
        assert!(res1.is_error());
        assert!(
            res1.error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
    }
    ```
  - Post-check: `cargo test -p opc-da-client --lib com::worker::write::tests::test_partition_write_inputs` fails (RED).

- **Step 4: [MODIFY] Implement `partition_write_inputs`**
  - File: `opc-da-client/src/com/worker/write.rs`
  - Pre-check: Step 3 fails.
  - Action: Implement helper with structured `tracing::warn!` logging:
    ```rust
    /// Partitions write batch inputs into valid tags and quarantines tags containing interior null bytes.
    ///
    /// # Errors
    ///
    /// This function does not return an [`Err`] result; validation rejections are captured directly
    /// into the returned [`Option<WriteResult>`] vector.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    pub(crate) fn partition_write_inputs<'a>(
        items: &[(&'a str, &OpcValue)],
        server_id: &ServerIdentifier,
    ) -> (Vec<&'a str>, Vec<usize>, Vec<Option<WriteResult>>) {
        let mut valid_tags = Vec::with_capacity(items.len());
        let mut valid_orig_indices = Vec::with_capacity(items.len());
        let mut write_results = vec![None; items.len()];

        for (orig_idx, &(tag_id, _)) in items.iter().enumerate() {
            if tag_id.contains('\0') {
                tracing::warn!(
                    server = %server_id,
                    tag = %tag_id.escape_debug(),
                    "write_tag_values: tag contains illegal interior null byte; quarantined"
                );
                if let Some(slot) = write_results.get_mut(orig_idx) {
                    *slot = Some(WriteResult::failure(
                        tag_id,
                        OpcError::InvalidState("Tag identifier contains illegal interior null byte".into()),
                    ));
                } else {
                    tracing::error!(server = %server_id, orig_idx, "Invalid write results slot index");
                }
            } else {
                valid_tags.push(tag_id);
                valid_orig_indices.push(orig_idx);
            }
        }

        (valid_tags, valid_orig_indices, write_results)
    }
    ```
  - Post-check: `cargo test -p opc-da-client --lib com::worker::write::tests::test_partition_write_inputs` passes (GREEN).

- **Step 5: [TEST] Unit tests for `partition_item_registration_results`**
  - File: `opc-da-client/src/com/worker/write.rs`
  - Pre-check: Step 4 passes.
  - Action: Add test:
    ```rust
    #[test]
    fn test_partition_item_registration_results_partial() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val0 = OpcValue::Int(100);
        let val1 = OpcValue::Int(200);
        let items = [("Tag.A", &val0), ("Tag.B", &val1)];
        let valid_orig_indices = vec![0, 1];
        let mut write_results = vec![None, None];

        let registration_results = vec![
            GroupItemResult {
                server_handle: ServerItemHandle::new(42),
                canonical_type: VarType::I4,
                error: None,
            },
            GroupItemResult {
                server_handle: ServerItemHandle::new(0),
                canonical_type: VarType::EMPTY,
                error: Some(OpcError::InvalidState("Tag.B not found".into())),
            },
        ];

        let (valid_writes, valid_write_orig_indices) = partition_item_registration_results(
            registration_results,
            &valid_orig_indices,
            &items,
            &mut write_results,
            &server_id,
        )
        .expect("partitioning must succeed");

        assert_eq!(valid_writes.len(), 1);
        assert_eq!(valid_writes[0].handle.as_raw(), 42);
        assert_eq!(valid_writes[0].value, OpcValue::Int(100));
        assert_eq!(valid_write_orig_indices, vec![0]);

        assert!(write_results[0].is_none());
        let failed = write_results[1]
            .as_ref()
            .expect("Tag.B must have failed result");
        assert_eq!(failed.tag_id, "Tag.B");
        assert!(failed.is_error());
        assert!(failed.error().unwrap().to_string().contains("Tag.B not found"));
    }

    #[test]
    fn test_partition_item_registration_results_count_mismatch() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val = OpcValue::Int(10);
        let items = [("Tag1", &val), ("Tag2", &val)];
        let valid_orig_indices = vec![0, 1];
        let mut write_results = vec![None, None];
        let registration_results = vec![GroupItemResult {
            server_handle: ServerItemHandle::new(1),
            canonical_type: VarType::I4,
            error: None,
        }]; // 1 result for 2 indices — mismatch!

        let err = partition_item_registration_results(
            registration_results,
            &valid_orig_indices,
            &items,
            &mut write_results,
            &server_id,
        )
        .expect_err("mismatched registration count must return Internal error");

        assert!(matches!(err, OpcError::Internal(ref msg) if msg.contains("mismatched")));
    }
    ```
  - Post-check: `cargo test -p opc-da-client --lib com::worker::write::tests::test_partition_item_registration_results` fails (RED).

- **Step 6: [MODIFY] Implement `partition_item_registration_results`**
  - File: `opc-da-client/src/com/worker/write.rs`
  - Pre-check: Step 5 fails.
  - Action: Update imports at the top of `src/com/worker/write.rs` (line 4) to include `GroupItemResult`:
    ```rust
    use crate::connector::{ConnectedGroup, ConnectedServer, GroupItemResult, ItemWrite};
    ```
    Implement helper with size guards, doc comments, `.zip()` pairing, and safe `.get_mut()` slot assignment:
    ```rust
    /// Partitions COM item registration results into valid write requests and records registration failures.
    ///
    /// # Errors
    ///
    /// - [`OpcError::Internal`]: Returned if the lengths of `write_results` and `items` mismatch,
    ///   if `results.len()` does not equal `valid_orig_indices.len()`, or if an `orig_idx` is out of bounds for `items`.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    pub(crate) fn partition_item_registration_results(
        results: Vec<GroupItemResult>,
        valid_orig_indices: &[usize],
        items: &[(&str, &OpcValue)],
        write_results: &mut [Option<WriteResult>],
        server_id: &ServerIdentifier,
    ) -> OpcResult<(Vec<ItemWrite>, Vec<usize>)> {
        if write_results.len() != items.len() {
            return Err(OpcError::Internal(format!(
                "Write results buffer size mismatch: expected {}, got {}",
                items.len(),
                write_results.len()
            )));
        }

        if results.len() != valid_orig_indices.len() {
            return Err(OpcError::Internal(format!(
                "Server {server_id} returned mismatched item registration count: expected {}, got {}",
                valid_orig_indices.len(),
                results.len()
            )));
        }

        let mut valid_writes = Vec::with_capacity(results.len());
        let mut valid_write_orig_indices = Vec::with_capacity(results.len());

        for (item_res, &orig_idx) in results.into_iter().zip(valid_orig_indices) {
            let (tag_id, val) = items.get(orig_idx).copied().ok_or_else(|| {
                OpcError::Internal(format!("Invalid original item index {orig_idx}"))
            })?;

            if let Some(e) = item_res.error {
                log_opc_err!(
                    &e,
                    "write_tag_values:items_rejected",
                    server = %server_id,
                    tag = %tag_id
                );
                let slot = write_results.get_mut(orig_idx).ok_or_else(|| {
                    OpcError::Internal(format!("Invalid write results slot index {orig_idx}"))
                })?;
                *slot = Some(WriteResult::failure(tag_id, e));
            } else {
                valid_writes.push(ItemWrite::new(item_res.server_handle, (*val).clone()));
                valid_write_orig_indices.push(orig_idx);
            }
        }

        Ok((valid_writes, valid_write_orig_indices))
    }
    ```
  - Post-check: `cargo test -p opc-da-client --lib com::worker::write::tests::test_partition_item_registration_results` passes (GREEN).

- **Step 7: [TEST] Unit tests for `assemble_write_results`**
  - File: `opc-da-client/src/com/worker/write.rs`
  - Pre-check: Step 6 passes.
  - Action: Add tests:
    ```rust
    #[test]
    fn test_assemble_write_results_outcomes() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val0 = OpcValue::Int(10);
        let val1 = OpcValue::Int(20);
        let items = [("Tag1", &val0), ("Tag2", &val1)];
        let write_results = vec![None, None];
        let valid_write_orig_indices = vec![0, 1];
        let server_write_results = Some(vec![
            Ok(()),
            Err(OpcError::InvalidState("Write rejected by PLC".into())),
        ]);

        let final_results = assemble_write_results(
            &items,
            write_results,
            &valid_write_orig_indices,
            server_write_results,
            &server_id,
        )
        .expect("assembly must succeed");

        assert_eq!(final_results.len(), 2);
        assert_eq!(final_results[0].tag_id, "Tag1");
        assert!(final_results[0].is_success());

        assert_eq!(final_results[1].tag_id, "Tag2");
        assert!(final_results[1].is_error());
        assert!(
            final_results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("Write rejected by PLC")
        );
    }

    #[test]
    fn test_assemble_write_results_parity_mismatch() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val0 = OpcValue::Int(10);
        let val1 = OpcValue::Int(20);
        let items = [("Tag1", &val0), ("Tag2", &val1)];
        let write_results = vec![None, None];
        let valid_write_orig_indices = vec![0, 1];
        let server_write_results = Some(vec![Ok(())]);

        let err = assemble_write_results(
            &items,
            write_results,
            &valid_write_orig_indices,
            server_write_results,
            &server_id,
        )
        .expect_err("mismatched results array size must fail");

        assert!(
            err.to_string().contains("mismatched write result array size"),
            "Expected 'mismatched write result array size' error, got: {err}"
        );
    }

    #[test]
    fn test_assemble_write_results_unassigned_slots_fallback() {
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let val0 = OpcValue::Int(10);
        let val1 = OpcValue::Int(20);
        let items = [("Tag1", &val0), ("Tag2", &val1)];
        let write_results = vec![Some(WriteResult::success("Tag1")), None];
        let valid_write_orig_indices = vec![];
        let server_write_results = None;

        let final_results = assemble_write_results(
            &items,
            write_results,
            &valid_write_orig_indices,
            server_write_results,
            &server_id,
        )
        .expect("assembly must succeed even with unassigned slots");

        assert_eq!(final_results.len(), 2);
        assert_eq!(final_results[0].tag_id, "Tag1");
        assert!(final_results[0].is_success());

        assert_eq!(final_results[1].tag_id, "Tag2");
        assert!(final_results[1].is_error());
        assert!(
            final_results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("was not populated by server")
        );
    }
    ```
  - Post-check: `cargo test -p opc-da-client --lib com::worker::write::tests::test_assemble_write_results` fails (RED).

- **Step 8: [MODIFY] Implement `assemble_write_results`**
  - File: `opc-da-client/src/com/worker/write.rs`
  - Pre-check: Step 7 fails.
  - Action: Implement helper with size guards, doc comments, and safe `.get_mut()` slot assignment:
    ```rust
    /// Assembles the final [`WriteResult`] list preserving exact index parity with caller input.
    ///
    /// # Errors
    ///
    /// - [`OpcError::Internal`]: Returned if `write_results.len()` does not equal `items.len()`,
    ///   if `server_write_results.len()` does not equal `valid_write_orig_indices.len()`, or if an `orig_idx`
    ///   is out of bounds for `items`.
    ///
    /// # Panics
    ///
    /// This function does not panic.
    pub(crate) fn assemble_write_results(
        items: &[(&str, &OpcValue)],
        mut write_results: Vec<Option<WriteResult>>,
        valid_write_orig_indices: &[usize],
        server_write_results: Option<Vec<OpcResult<()>>>,
        server_id: &ServerIdentifier,
    ) -> OpcResult<Vec<WriteResult>> {
        if write_results.len() != items.len() {
            return Err(OpcError::Internal(format!(
                "Write results buffer size mismatch: expected {}, got {}",
                items.len(),
                write_results.len()
            )));
        }

        if let Some(server_results) = server_write_results {
            if server_results.len() != valid_write_orig_indices.len() {
                let err = OpcError::Internal(format!(
                    "server returned mismatched write result array size: expected {}, got {}",
                    valid_write_orig_indices.len(),
                    server_results.len()
                ));
                log_opc_err!(
                    &err,
                    "write_tag_values:mismatched",
                    server = %server_id,
                    expected = valid_write_orig_indices.len(),
                    actual = server_results.len()
                );
                return Err(err);
            }

            for (res, &orig_idx) in server_results.into_iter().zip(valid_write_orig_indices) {
                let (tag_id, _) = items.get(orig_idx).copied().ok_or_else(|| {
                    OpcError::Internal(format!(
                        "Invalid original item index {orig_idx} during write result assembly"
                    ))
                })?;

                let slot = write_results.get_mut(orig_idx).ok_or_else(|| {
                    OpcError::Internal(format!("Invalid write results slot index {orig_idx}"))
                })?;
                *slot = Some(match res {
                    Ok(()) => WriteResult::success(tag_id),
                    Err(e) => {
                        log_opc_err!(
                            &e,
                            "write_tag_values:server_rejected",
                            server = %server_id,
                            tag = %tag_id
                        );
                        WriteResult::failure(tag_id, e)
                    }
                });
            }
        }

        let final_results: Vec<WriteResult> = write_results
            .into_iter()
            .enumerate()
            .map(|(orig_idx, opt)| {
                opt.unwrap_or_else(|| {
                    let tag_id = items
                        .get(orig_idx)
                        .map_or("<unknown>", |(tag, _)| *tag);
                    WriteResult::failure(
                        tag_id,
                        OpcError::Internal(format!(
                            "Write result slot for tag '{tag_id}' was not populated by server {server_id}"
                        )),
                    )
                })
            })
            .collect();

        Ok(final_results)
    }
    ```
  - Post-check: `cargo test -p opc-da-client --lib com::worker::write::tests::test_assemble_write_results` passes (GREEN).
  - Checkpoint: `git commit -m "feat(worker): implement decomposed pipeline helpers for batch write"`

- **Step 9a: [TEST] Unit Tests for Refactored Write Pipeline**
  - File: `opc-da-client/src/com/worker/write.rs`
  - Pre-check: All helper unit tests pass (GREEN).
  - Note: These tests verify the new decomposed behavior. They will fail (RED) against the current monolithic implementation because: (1) null-byte tags currently abort the entire batch via `?`, (2) all-null batches still allocate COM groups, (3) `handle_write` currently returns `Err` not `Ok(WriteResult::failure)` for null bytes.
  - Action: Add the 4 unit tests for `handle_write_batch` and `handle_write` in `src/com/worker/write.rs`:
    ```rust
    #[test]
    fn test_handle_write_batch_granular_null_byte_isolation() {
        let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
        let server = crate::connector::mock::MockConnectedServer {
            state: state.clone(),
            ..Default::default()
        };
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let writes = vec![
            ("Clean.Tag1".to_string(), OpcValue::Int(1)),
            ("Dirty\0.Tag2".to_string(), OpcValue::Int(2)),
            ("Clean.Tag3".to_string(), OpcValue::Int(3)),
        ];

        let results = handle_write_batch(&server_id, &writes.into_write_batch(), &server)
            .expect("batch write should succeed with granular isolation");

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].tag_id, "Clean.Tag1");
        assert!(results[0].is_success());

        assert_eq!(results[1].tag_id, "Dirty\0.Tag2");
        assert!(results[1].is_error());
        assert!(
            results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );

        assert_eq!(results[2].tag_id, "Clean.Tag3");
        assert!(results[2].is_success());

        assert_eq!(
            state
                .add_group_count
                .load(std::sync::atomic::Ordering::Relaxed),
            1,
            "Exactly 1 COM group should be registered for valid tags"
        );
    }

    #[test]
    fn test_handle_write_batch_all_null_short_circuits() {
        let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
        let server = crate::connector::mock::MockConnectedServer {
            state: state.clone(),
            ..Default::default()
        };
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let writes = vec![
            ("Bad\0Tag1".to_string(), OpcValue::Int(1)),
            ("Bad\0Tag2".to_string(), OpcValue::Int(2)),
        ];

        let results = handle_write_batch(&server_id, &writes.into_write_batch(), &server)
            .expect("all-null batch should short-circuit and return results");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tag_id, "Bad\0Tag1");
        assert!(results[0].is_error());
        assert!(
            results[0]
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
        assert_eq!(results[1].tag_id, "Bad\0Tag2");
        assert!(results[1].is_error());
        assert!(
            results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
        assert_eq!(
            state
                .add_group_count
                .load(std::sync::atomic::Ordering::Relaxed),
            0,
            "No COM groups should be allocated when all tags contain null bytes"
        );
    }

    #[test]
    fn test_handle_write_batch_all_rejected_registration_skips_write() {
        let state = std::sync::Arc::new(crate::connector::mock::MockState::default());
        let write_invoked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let write_invoked_clone = write_invoked.clone();

        let group = crate::connector::mock::MockConnectedGroup {
            state: state.clone(),
            ..Default::default()
        }
        .with_add_items_fn(|items| {
            Ok(items
                .iter()
                .map(|_| crate::connector::GroupItemResult {
                    server_handle: ServerItemHandle::new(0),
                    canonical_type: VarType::EMPTY,
                    error: Some(OpcError::InvalidState("Rejected in registration".into())),
                })
                .collect())
        })
        .with_write_fn(move |items| {
            write_invoked_clone.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(items.iter().map(|_| Ok(())).collect())
        });

        let server = crate::connector::mock::MockConnectedServer {
            group: std::sync::Arc::new(group),
            state,
            ..Default::default()
        };
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let writes = vec![
            ("Tag1".to_string(), OpcValue::Int(10)),
            ("Tag2".to_string(), OpcValue::Int(20)),
        ];

        let results = handle_write_batch(&server_id, &writes.into_write_batch(), &server)
            .expect("all-rejected registration must return results without error");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tag_id, "Tag1");
        assert!(results[0].is_error());
        assert!(
            results[0]
                .error()
                .unwrap()
                .to_string()
                .contains("Rejected in registration")
        );
        assert_eq!(results[1].tag_id, "Tag2");
        assert!(results[1].is_error());
        assert!(
            results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("Rejected in registration")
        );
        assert!(
            !write_invoked.load(std::sync::atomic::Ordering::Relaxed),
            "group.write() must not be called when all items are rejected during registration"
        );
    }

    #[test]
    fn test_handle_write_scalar_null_byte_returns_failure_result() {
        let server = crate::connector::mock::MockConnectedServer::default();
        let server_id = ServerIdentifier::try_from("Test.Server").unwrap();
        let value = OpcValue::Int(42);

        let result = handle_write(&server_id, "Bad\0Tag", &value, &server)
            .expect("scalar write must return Ok(WriteResult) even on item validation error");

        assert!(result.is_error());
        assert_eq!(result.tag_id, "Bad\0Tag");
        assert!(
            result
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
    }
    ```
  - Post-check: `cargo test -p opc-da-client --lib com::worker::write::tests` fails (RED).

- **Step 9b: [MODIFY] Refactor Write Pipeline with Decomposed Helpers**
  - File: `opc-da-client/src/com/worker/write.rs`
  - Pre-check: Step 9a tests fail (RED).
  - Action: Replace `handle_write_batch` and `handle_write`, remove `#[allow(clippy::too_many_lines)]`, use structural destructuring of `RegisteredItemGroup`, call `partition_write_inputs(&items, server_id)`, and preserve full telemetry and documentation:
    ```rust
    /// Executes synchronous batch writing across multiple tags in a single atomic COM group, returning
    /// a list of structured [`WriteResult`]s preserving the original index ordering.
    ///
    /// # Parameters
    ///
    /// - `server_id`: Identifier of the connected OPC DA server for contextual tracing and logging.
    /// - `writes`: Batch of tag IDs and corresponding [`OpcValue`] payloads to write.
    /// - `opc_server`: Connected OPC server instance implementing [`ConnectedServer`].
    ///
    /// # Errors
    ///
    /// - [`OpcError::InvalidState`]: Returned if the batch size exceeds [`MAX_TAG_BATCH_SIZE`].
    /// - [`OpcError::Internal`]: Returned if internal buffer sizes, item registration counts, or
    ///   result array lengths exhibit invariant mismatches during input partitioning or result assembly.
    /// - Transport / COM errors ([`OpcError::Com`]): Returned if COM group creation via
    ///   [`register_item_group`](super::register_item_group) or COM group write execution via
    ///   [`ConnectedGroup::write`] fails due to communication or server interface failure.
    #[tracing::instrument(
        name = "opc.write_tag_values",
        level = "info",
        skip(writes, opc_server),
        fields(write_count = writes.len()),
        err
    )]
    pub fn handle_write_batch<S: ConnectedServer>(
        server_id: &ServerIdentifier,
        writes: &WriteBatch,
        opc_server: &S,
    ) -> OpcResult<Vec<WriteResult>> {
        if writes.is_empty() {
            return Ok(Vec::new());
        }

        if writes.len() > MAX_TAG_BATCH_SIZE {
            return Err(OpcError::InvalidState(format!(
                "Write batch size {} exceeds maximum allowed limit of {MAX_TAG_BATCH_SIZE}",
                writes.len(),
            )));
        }

        #[cfg(feature = "dev-diagnostics")]
        tracing::trace!(
            server = %server_id,
            write_count = writes.len(),
            sample_writes = ?writes.iter().take(5).collect::<Vec<_>>(),
            "write_tag_values: starting batch write"
        );
        let start = std::time::Instant::now();

        let items: Vec<(&str, &OpcValue)> = writes.iter().collect();

        // Stage 1 & 2: Partition inputs and isolate interior null byte tags (CWE-626 defense)
        let (valid_tags, valid_orig_indices, mut write_results) =
            partition_write_inputs(&items, server_id);

        // Short-circuit if all tags were quarantined: zero COM group allocations
        if valid_tags.is_empty() {
            let final_results = assemble_write_results(&items, write_results, &[], None, server_id)?;
            tracing::info!(
                count = final_results.len(),
                elapsed_ms = super::elapsed_ms(start),
                "write_tag_values batch completed"
            );
            return Ok(final_results);
        }

        // Stage 3: Register COM item group for valid tags
        let crate::com::worker::RegisteredItemGroup {
            group,
            group_guard: _group_guard,
            item_results,
        } = crate::com::worker::register_item_group(opc_server, server_id, "opc-write", &valid_tags)?;

        // Stage 4: Partition item registration results
        let (valid_writes, valid_write_orig_indices) = partition_item_registration_results(
            item_results,
            &valid_orig_indices,
            &items,
            &mut write_results,
            server_id,
        )?;

        // Stage 5: Execute COM write (if any items registered successfully) and assemble final results
        let server_write_results = if !valid_writes.is_empty() {
            let results = group.write(&valid_writes).inspect_err(|e| {
                log_opc_err!(
                    e,
                    "write_tag_values:sync",
                    server = %server_id,
                    handle_count = valid_writes.len()
                );
            })?;
            Some(results)
        } else {
            None
        };

        let final_results = assemble_write_results(
            &items,
            write_results,
            &valid_write_orig_indices,
            server_write_results,
            server_id,
        )?;

        tracing::info!(
            count = final_results.len(),
            elapsed_ms = super::elapsed_ms(start),
            "write_tag_values batch completed"
        );
        Ok(final_results)
    }

    /// Executes synchronous single-tag writing, delegating to [`handle_write_batch`].
    ///
    /// # Parameters
    ///
    /// - `server_id`: Identifier of the connected OPC DA server for contextual tracing.
    /// - `tag_id`: String identifier of the target OPC tag.
    /// - `value`: [`OpcValue`] payload to write.
    /// - `opc_server`: Connected OPC server instance implementing [`ConnectedServer`].
    ///
    /// # Errors
    ///
    /// - [`OpcError::Internal`]: Returned if internal result assembly fails to yield a result slot.
    /// - Transport / COM errors ([`OpcError::Com`], [`OpcError::Connection`]): Returned if COM communication fails.
    #[tracing::instrument(
        name = "opc.write_tag_value",
        level = "info",
        skip(value, opc_server),
        fields(tag = %tag_id),
        err
    )]
    pub fn handle_write<S: ConnectedServer>(
        server_id: &ServerIdentifier,
        tag_id: &str,
        value: &OpcValue,
        opc_server: &S,
    ) -> OpcResult<WriteResult> {
        #[cfg(feature = "dev-diagnostics")]
        tracing::trace!(
            server = %server_id,
            tag = %tag_id,
            value = ?value,
            "write_tag_value: starting single write"
        );
        let mut results = handle_write_batch(
            server_id,
            &WriteBatch::Single(tag_id.to_string(), value.clone()),
            opc_server,
        )?;
        results
            .pop()
            .ok_or_else(|| OpcError::Internal("No write result returned".into()))
    }
    ```
  - Post-check: `cargo test -p opc-da-client --lib com::worker::write::tests` passes (GREEN).
  - Checkpoint: `git commit -m "feat(worker): refactor handle_write_batch and handle_write with decomposed helpers"`

- **Step 10: [TEST+VERIFY] Expand `tests/batch_write_test.rs` & Run Full Quality Gate**
  - File: `opc-da-client/tests/batch_write_test.rs`
  - Pre-check: All unit tests pass.
  - Action: Update file imports and add full integration tests:
    ```rust
    use opc_da_client::connector::mock::{MockServerConnector, MockState};
    use opc_da_client::connector::GroupItemResult;
    use opc_da_client::types::{ServerItemHandle, VarType, WriteResult};
    use opc_da_client::{OpcDaClient, OpcError, OpcProvider, OpcServerEndpoint, OpcValue};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_client_batch_write_partial_rejection_preserves_order() {
        let state = Arc::new(MockState::default());
        let connector = MockServerConnector::with_state(state.clone())
            .with_servers(vec!["Mock.Server.Order".to_string()])
            .with_add_items_fn(|items| {
                Ok(items
                    .iter()
                    .enumerate()
                    .map(|(idx, item)| {
                        if item.item_id == "Tag.AddFail" {
                            GroupItemResult {
                                server_handle: ServerItemHandle::new(0),
                                canonical_type: VarType::EMPTY,
                                error: Some(OpcError::InvalidState("Tag.AddFail not configured".into())),
                            }
                        } else {
                            GroupItemResult {
                                #[allow(clippy::cast_possible_truncation)]
                                server_handle: ServerItemHandle::new((idx + 1) as u32),
                                canonical_type: VarType::I4,
                                error: None,
                            }
                        }
                    })
                    .collect())
            })
            .with_write_fn(|items| {
                Ok(items
                    .iter()
                    .map(|item| {
                        if item.value == OpcValue::Int(999) {
                            Err(OpcError::InvalidState("Tag.WriteFail read-only".into()))
                        } else {
                            Ok(())
                        }
                    })
                    .collect())
            });

        let client = OpcDaClient::builder()
            .server("Mock.Server.Order")
            .with_connector(connector)
            .build_bound()
            .expect("build bound client");

        let writes = vec![
            ("Tag.Success1".to_string(), OpcValue::Int(10)),
            ("Tag.AddFail".to_string(), OpcValue::Int(20)),
            ("Tag.WriteFail".to_string(), OpcValue::Int(999)),
            ("Tag.Success2".to_string(), OpcValue::Int(30)),
        ];

        let results = client
            .write_tags(writes)
            .await
            .expect("write_tags must succeed with mixed results");

        assert_eq!(results.len(), 4);

        assert_eq!(results[0].tag_id, "Tag.Success1");
        assert!(results[0].is_success());

        assert_eq!(results[1].tag_id, "Tag.AddFail");
        assert!(results[1].is_error());
        assert!(
            results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("Tag.AddFail not configured")
        );

        assert_eq!(results[2].tag_id, "Tag.WriteFail");
        assert!(results[2].is_error());
        assert!(
            results[2]
                .error()
                .unwrap()
                .to_string()
                .contains("Tag.WriteFail read-only")
        );

        assert_eq!(results[3].tag_id, "Tag.Success2");
        assert!(results[3].is_success());
    }

    #[tokio::test]
    async fn test_client_batch_write_interior_null_byte_isolated() {
        let state = Arc::new(MockState::default());
        let connector = MockServerConnector::with_state(state.clone());
        let client = OpcDaClient::builder()
            .server("Mock.Server.NullIso")
            .with_connector(connector)
            .build_bound()
            .expect("build bound client");

        let writes = vec![
            ("Clean.Tag1".to_string(), OpcValue::Int(1)),
            ("Dirty\0.Tag2".to_string(), OpcValue::Int(2)),
            ("Clean.Tag3".to_string(), OpcValue::Int(3)),
        ];

        let results = client
            .write_tags(writes)
            .await
            .expect("write_tags must return results");

        assert_eq!(results.len(), 3);

        assert_eq!(results[0].tag_id, "Clean.Tag1");
        assert!(results[0].is_success());

        assert_eq!(results[1].tag_id, "Dirty\0.Tag2");
        assert!(results[1].is_error());
        assert!(
            results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );

        assert_eq!(results[2].tag_id, "Clean.Tag3");
        assert!(results[2].is_success());

        assert_eq!(
            state.add_group_count.load(Ordering::Relaxed),
            1,
            "Exactly 1 COM group allocated for clean tags"
        );
    }

    #[tokio::test]
    async fn test_client_batch_write_empty_short_circuits() {
        let state = Arc::new(MockState::default());
        let connector = MockServerConnector::with_state(state.clone());
        let client = OpcDaClient::builder()
            .server("Mock.Server.Empty")
            .with_connector(connector)
            .build_bound()
            .expect("build bound client");

        let empty_writes: Vec<(String, OpcValue)> = Vec::new();
        let results = client
            .write_tags(empty_writes)
            .await
            .expect("empty write batch must succeed");

        assert!(results.is_empty());
        assert_eq!(
            state.add_group_count.load(Ordering::Relaxed),
            0,
            "No COM groups must be allocated for empty batch"
        );
    }

    #[tokio::test]
    async fn test_client_batch_write_all_rejected_skips_write() {
        let state = Arc::new(MockState::default());
        let write_called = Arc::new(AtomicBool::new(false));
        let write_called_clone = write_called.clone();

        let connector = MockServerConnector::with_state(state.clone())
            .with_servers(vec!["Mock.Server.Reject".to_string()])
            .with_add_items_fn(|items| {
                Ok(items
                    .iter()
                    .map(|_| GroupItemResult {
                        server_handle: ServerItemHandle::new(0),
                        canonical_type: VarType::EMPTY,
                        error: Some(OpcError::InvalidState("Item rejected in add_items".into())),
                    })
                    .collect())
            })
            .with_write_fn(move |items| {
                write_called_clone.store(true, Ordering::Relaxed);
                Ok(items.iter().map(|_| Ok(())).collect())
            });

        let client = OpcDaClient::builder()
            .server("Mock.Server.Reject")
            .with_connector(connector)
            .build_bound()
            .expect("build bound client");

        let writes = vec![
            ("Tag1".to_string(), OpcValue::Int(10)),
            ("Tag2".to_string(), OpcValue::Int(20)),
        ];

        let results = client
            .write_tags(writes)
            .await
            .expect("write_tags must return results vector");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tag_id, "Tag1");
        assert!(results[0].is_error());
        assert!(
            results[0]
                .error()
                .unwrap()
                .to_string()
                .contains("Item rejected in add_items")
        );
        assert_eq!(results[1].tag_id, "Tag2");
        assert!(results[1].is_error());
        assert!(
            results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("Item rejected in add_items")
        );
        assert!(
            !write_called.load(Ordering::Relaxed),
            "group.write() must not be invoked when all items are rejected during registration"
        );
    }

    #[tokio::test]
    async fn test_client_batch_write_all_null_short_circuits() {
        let state = Arc::new(MockState::default());
        let connector = MockServerConnector::with_state(state.clone());
        let client = OpcDaClient::builder()
            .server("Mock.Server.AllNull")
            .with_connector(connector)
            .build_bound()
            .expect("build bound client");

        let writes = vec![
            ("Bad\0Tag1".to_string(), OpcValue::Int(1)),
            ("Bad\0Tag2".to_string(), OpcValue::Int(2)),
        ];

        let results = client
            .write_tags(writes)
            .await
            .expect("all-null batch must return results without aborting");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tag_id, "Bad\0Tag1");
        assert!(results[0].is_error());
        assert!(
            results[0]
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
        assert_eq!(results[1].tag_id, "Bad\0Tag2");
        assert!(results[1].is_error());
        assert!(
            results[1]
                .error()
                .unwrap()
                .to_string()
                .contains("illegal interior null byte")
        );
        assert_eq!(
            state.add_group_count.load(Ordering::Relaxed),
            0,
            "No COM groups must be allocated when all tags contain null bytes"
        );
    }

    #[test]
    fn test_write_result_display_and_connection_error() {
        let ok = WriteResult::success("Plant.Pump1");
        assert!(!ok.is_connection_error());
        assert_eq!(format!("{ok}"), "Write 'Plant.Pump1': succeeded");

        let conn_err = WriteResult::failure(
            "Plant.Pump1",
            OpcError::Connection("Lost connection to PLC".into()),
        );
        assert!(conn_err.is_connection_error());
        assert_eq!(
            format!("{conn_err}"),
            "Write 'Plant.Pump1': failed (Connection failed: Lost connection to PLC)"
        );

        let config_err = WriteResult::failure(
            "Plant.Pump2",
            OpcError::InvalidState("Unknown tag name".into()),
        );
        assert!(!config_err.is_connection_error());
        assert_eq!(
            format!("{config_err}"),
            "Write 'Plant.Pump2': failed (Invalid state: Unknown tag name)"
        );
    }
    ```
  - Post-check:
    - `cargo test -p opc-da-client --test batch_write_test --features="test-support"` passes.
    - `cargo test --workspace` passes.
    - `pwsh scripts/verify.ps1` passes all 8 quality gates with 0 warnings under `-D warnings`.
  - Checkpoint: `git commit -m "test(com): expand batch write integration coverage with defensive hardening"`

---

## 7. Verification & Test Strategy
- **Commands:**
  1. `cargo test -p opc-da-client --lib types::write_batch`
  2. `cargo test --doc -p opc-da-client is_connection_error`
  3. `cargo test -p opc-da-client --lib com::worker::write`
  4. `cargo test -p opc-da-client --test batch_write_test --features="test-support"`
  5. `cargo clippy -p opc-da-client --all-targets -- -D warnings`
  6. `pwsh scripts/verify.ps1`
- **Quality Gates:**
  - Zero compiler/clippy warnings.
  - Zero unchecked indexing (`rg "items\[" opc-da-client/src/com/worker/write.rs` returns 0).
  - All 8 verification gates exit 0.

---

## Appendix: Plan Review — Final Validation (Cycle 4)

**Reviewer:** Plan Reviewer (Gemini 3.8 Flash High)
**Date:** 2026-09-18
**Verdict:** ✅ APPROVED

### Correction Verification (14/14 ✅)

| ID | Description | Status |
|:---|:---|:---:|
| C-1 | Step 10 complete import block | ✅ |
| C-2 | All `[]` indexing → `.get_mut()` | ✅ |
| C-3 | `server_id` parameter position standardized | ✅ |
| C-4 | Fluent mock builder pattern | ✅ |
| C-5 | All-null integration test added | ✅ |
| U-1 | Step 9 → 9a [TEST] RED + 9b [MODIFY] GREEN | ✅ |
| U-2 | Assertions strengthened | ✅ |
| U-3 | Parity mismatch test added | ✅ |
| U-6 | `handle_write` `# Errors` fixed | ✅ |
| U-7 | Helper doc comments added | ✅ |
| U-8 | `# Panics` on `is_connection_error` | ✅ |
| U-9 | `escape_debug()` in tracing | ✅ |
| U-10 | `ExactSizeIterator` documented | ✅ |
| U-11 | Structural destructuring | ✅ |

### IPR Compliance (8/8 ✅)
- Complete test bodies, code snippets, header table, objectives, GEO, defensive indexing, observability, feature gating — all verified.

### Post-Approval Notes (for Builder)
- **FV-1**: Add `GroupItemResult` to `write.rs` line 4 imports: `use crate::connector::{ConnectedGroup, ConnectedServer, GroupItemResult, ItemWrite};`
- **FV-2**: Step 10 imports trimmed to exclude unused `MockConnectedGroup`, `MockConnectedServer`, `Mutex`.
