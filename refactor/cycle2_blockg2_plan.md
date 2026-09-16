# Implementation Plan: Block G2: Ergonomic Symmetry & Comprehensive Public Documentation

**Role:** Architect • **Date:** 2026-09-16 • **Tier:** M  
**Scope:** Block G2: Ergonomic Symmetry & Comprehensive Public Documentation in `opc-da-client`

### Builder Context
Read before starting:
- `opc-da-client/src/client/gateway.rs` L14-62 (`OpcDaClient<C, Unbound>` inherent methods)
- `opc-da-client/src/client/session.rs` L12-117 (`OpcDaClient<C, Bound>` inherent methods)
- `opc-da-client/src/client/typestate.rs` L80-96 (`connect_eager` inherent method)
- `opc-da-client/src/client/mod.rs` L80-131 (`bind_new`, `bind_new_remote`, `new`)
- `opc-da-client/src/client/builder.rs` L12-190 (`OpcDaClientBuilder` constructors, `new`, `build`, `build_bound`)
- `opc-da-client/src/types/write_batch.rs` L340-450 (`IntoWriteBatch` blanket impls and `WriteBatch::from`)
- `opc-da-client/src/types/collection.rs` L448-468 (`TagValues::get_value_checked`)
- `opc-da-client/src/client/tests.rs` (client facade unit tests)
- `refactor/cycle2_review.md` (Findings #5, #11, #15)
- `.agents/rules/coding-standard.md` (governance core rules, §4.5 doc standards)

### Phase Context
- **Phase:** Block G2 of 2 (Cycle 2 Modernization: Block G1 Clean Slate Excision -> Block G2 Ergonomics & Documentation)
- **Prior phase:** Block G1 excised deprecated public APIs (`connect`, `connect_remote`, `write_tag_values`), consolidated duplicate struct declarations for `OpcDaClient` and `OpcDaClientBuilder`, introduced `NoopServerBackend`, and achieved zero `#[allow(deprecated)]` across `opc-da-client`.
- **Stubs for this phase:** None. All domain models (`IntoWriteBatch`, `Into<OpcValue>`, `IntoTags`, `TagValues`, `OpcValue`) and pure-Rust mock backends (`MockServerConnector`, `MockState`) are fully operational.
- **Following phase:** Block H (Domain Invariants, Security Hardening & Case Normalization — Findings #2, #7, #8, #9, #12, #18, #21).

---

### Problem Statement
In `refactor/cycle2_review.md`, three findings compromise the public API ergonomics, developer experience, and documentation completeness of `opc-da-client`:
1. **Finding #11 (Minor - API Design / Ergonomic Asymmetry)**: Inherent gateway methods on `OpcDaClient<C, Unbound>` accept concrete types (`writes: WriteBatch`, `value: OpcValue`), forcing callers to explicitly instantiate enum wrappers even for single primitives or tuple arrays. In contrast, `OpcDaClient<C, Bound>` provides generic conversions (`impl IntoWriteBatch`, `impl Into<OpcValue>`). Furthermore, `Unbound` lacks intuitive shorthand aliases (`read_tags`, `read_tag`, `write_tags`, `write_tag`, `browse`) taking `(server, ...)`, which creates cognitive dissonance between `Bound` and `Unbound` workflows.
2. **Finding #5 (Major - API Documentation)**: Complete absence of rustdoc comments (`///`) across all 13 inherent session methods in `session.rs` and all 7 inherent gateway methods in `gateway.rs`. This violates `coding-standard.md §2` ("100% of public APIs documented") and `§4.5` ("Every public item must have a doc comment that includes: Summary, Details, # Errors, # Panics, # Examples").
3. **Finding #15 (Minor - API Documentation)**: Incomplete documentation sections (`# Examples`, `# Errors`) on public constructors and utilities, including `OpcDaClient::connect_eager` in `typestate.rs`, `OpcDaClient::bind_new_remote` and `OpcDaClient::new` in `client/mod.rs`, `TagValues::get_value_checked` in `types/collection.rs`, and `OpcDaClientBuilder` constructors (`new`, `new_with_connector`, `build`, `build_bound`).

---

### Plan Objectives
| ID | Objective | Success Criteria | Steps |
|----|-----------|-----------------|-------|
| O1 | Implement generic `From` conversions for `WriteBatch` (Finding #11) | `WriteBatch::from` implemented for `(S, V)`, `[(S, V); N]`, `&[(S, V); N]`, `Vec<(S, V)>`, `&[(S, V)]`, and `FromIterator<(S, V)>` where `S: Into<String>` and `V: Into<OpcValue>`; unit tests verify zero explicit enum wrapping | 1, 2 |
| O2 | Modernize `Unbound::write_tag_batch` and `write_tag_value` signatures with generic ergonomics (Finding #11) | `write_tag_batch` accepts `impl IntoWriteBatch`; `write_tag_value` accepts `impl Into<OpcValue>`; `#[tracing::instrument]` adjusted; unit tests verify primitives and tuple arrays pass cleanly | 3, 4, 5, 6 |
| O3 | Add ergonomic shorthand aliases on `OpcDaClient<C, Unbound>` (Finding #11) | `read_tags`, `read_tag`, `write_tags`, `write_tag`, `browse` aliases added to `Unbound`; unit tests verify delegation and mock assertions | 7, 8 |
| O4 | Add runnable doctest & error documentation for `TagValues::get_value_checked` (Finding #15) | `get_value_checked` in `src/types/collection.rs` includes `#[must_use]`, complete `# Errors`, `# Panics`, and runnable doctest using `OpcValue::Float` passing in Gate 3 (`cargo test --doc`) | 9 |
| O5 | Add comprehensive rustdoc comments across all 13 inherent `Bound` session methods (Finding #5) | All 13 methods in `src/client/session.rs` have Summary, Details, `# Errors`, `# Panics`, and ````rust,no_run` examples | 10 |
| O6 | Add comprehensive rustdoc comments across all 12 inherent `Unbound` gateway methods & aliases (Finding #5) | All 7 original + 5 alias methods in `src/client/gateway.rs` have Summary, Details, `# Errors`, `# Panics`, and ````rust,no_run` examples | 11 |
| O7 | Add complete rustdoc & examples on client constructors, typestate, and builder methods (Finding #15) | `connect_eager`, `bind_new_remote`, `OpcDaClient::new`, and `OpcDaClientBuilder` methods have `# Errors`, `# Panics`, and `# Examples` passing doc checks | 12, 13 |
| O8 | Full workspace 9-gate quality verification green | `pwsh -File scripts/verify.ps1` exits 0 with zero warnings under `-D warnings` | 14 |

---

### Review History & Verdict
| Cycle | Reviewer | Model | Verdict | Adjustments Applied |
|---|---|---|---|---|
| 1 | `plan-reviewer` | `flash` | ⚠️ Revisions Recommended | (1) Removed `fields(write_count = writes.len())` from `#[tracing::instrument]` on generic `write_tag_batch`. (2) Added complete, compilable Rust code bodies to all `[TEST]` steps. (3) Added `From<&[(S, V); N]>` and `FromIterator<(S, V)>` for `V: Into<OpcValue>`. (4) Added `# Errors` for `build` and `build_with_connector`, and documented `OpcDaClient::new`. (5) Consolidated `get_value_checked` into a single step with `#[must_use]`, and inserted component checkpoints. |
| 2 | `plan-reviewer` | `flash` | ⚠️ Revisions Recommended | (1) Added `setup_mock_client` test fixture definition in `src/client/tests.rs` (Step 3). (2) Corrected doctest variant from `OpcValue::Double` to `OpcValue::Float` in `TagValues::get_value_checked` (Step 9). (3) Changed unused variable `state` to `_state` in Step 5. (4) Used idiomatic `to_owned()` in `From<&[(S, V)]>` slice mapping (Step 2). |
| 3 | `plan-reviewer` | `flash` | ✅ Approved | All 4 findings resolved; trait object safety strictly preserved on SPI role traits (`TagWriter`, `TagReader`, `TagBrowser`); Gate 3 doc tests verified. Approved for execution. |
| 4 | Architect (codebase-validated) | `flash` recon × 3 | ✅ Approved (Hardened) | (1) Step 2: added explicit deletion manifest for 7 concrete `From` impls + 1 `FromIterator` to prevent E0119 coherence errors; preserved `From<Arc<...>>`. (2) Step 2: corrected target line range from L340-410 to L332-396. (3) Step 4: added `IntoWriteBatch` to `gateway.rs` import block (was missing). (4) Step 9: clarified existing Summary + `# Errors` preserved; only `#[must_use]`, `# Panics`, `# Examples` are new. (5) Step 12: annotated per-method existing vs new doc sections. (6) Step 13: clarified all builder methods already have summary docs; only `# Errors`, `# Panics`, `# Examples` enhancements. (7) Plan Summary: corrected file count from 6 to 7. |

---

### Negative Scope
**Out of Scope:**
- Do NOT modify SPI role traits in `src/provider.rs` (`TagWriter`, `TagReader`, `TagBrowser`) — traits strictly retain concrete parameter types (`WriteBatch`, `TagBatch`) to preserve trait object safety (`dyn TagWriter`, `dyn OpcProvider`).
- Do NOT modify CLI crate commands in `opc-cli` — per user interview decision, Block G2 is strictly isolated to `opc-da-client`.
- Do NOT touch Block H items (ProgID validation, CWE-626 null-byte checks in COM FFI, unread COM interfaces, case-insensitivity in cache/validation).
- Do NOT touch Block I items (worker write batch allocations, `MAX_WRITE_BATCH_SIZE`, `TagCollector` `RwLock`).
- Do NOT modify external dependencies in `Cargo.toml`.

---

### Interface Contracts

#### 1. Generic Conversions for `WriteBatch` (`src/types/write_batch.rs`)
```rust
impl<S: Into<String>, V: Into<OpcValue>> From<(S, V)> for WriteBatch {
    #[inline]
    fn from((tag, val): (S, V)) -> Self {
        Self::Single(tag.into(), val.into())
    }
}

impl<S: Into<String>, V: Into<OpcValue>, const N: usize> From<[(S, V); N]> for WriteBatch {
    #[inline]
    fn from(arr: [(S, V); N]) -> Self {
        Self::Owned(arr.into_iter().map(|(t, v)| (t.into(), v.into())).collect())
    }
}

impl<S: AsRef<str>, V: Clone + Into<OpcValue>, const N: usize> From<&[(S, V); N]> for WriteBatch {
    #[inline]
    fn from(arr: &[(S, V); N]) -> Self {
        Self::from(&arr[..])
    }
}

impl<S: Into<String>, V: Into<OpcValue>> From<Vec<(S, V)>> for WriteBatch {
    #[inline]
    fn from(vec: Vec<(S, V)>) -> Self {
        Self::Owned(vec.into_iter().map(|(t, v)| (t.into(), v.into())).collect())
    }
}

impl<S: AsRef<str>, V: Clone + Into<OpcValue>> From<&[(S, V)]> for WriteBatch {
    #[inline]
    fn from(slice: &[(S, V)]) -> Self {
        Self::Owned(
            slice
                .iter()
                .map(|(t, v)| (t.as_ref().to_owned(), v.clone().into()))
                .collect(),
        )
    }
}

impl<S: Into<String>, V: Into<OpcValue>> FromIterator<(S, V)> for WriteBatch {
    #[inline]
    fn from_iter<T: IntoIterator<Item = (S, V)>>(iter: T) -> Self {
        Self::Owned(
            iter.into_iter()
                .map(|(tag, val)| (tag.into(), val.into()))
                .collect(),
        )
    }
}
```

#### 2. Modernized Inherent Gateway Signatures & Shorthand Aliases (`src/client/gateway.rs`)
```rust
impl<C: ServerBackend + 'static> OpcDaClient<C, Unbound> {
    #[tracing::instrument(level = "info", skip(self, value), err)]
    pub async fn write_tag_value(
        &self,
        server: &str,
        tag_id: &str,
        value: impl Into<OpcValue>,
    ) -> OpcResult<WriteResult> {
        TagWriter::write_tag_value(self, server, tag_id, value.into()).await
    }

    #[tracing::instrument(level = "info", skip(self, writes), err)]
    pub async fn write_tag_batch(
        &self,
        server: &str,
        writes: impl IntoWriteBatch,
    ) -> OpcResult<Vec<WriteResult>> {
        TagWriter::write_tag_batch(self, server, writes.into_write_batch()).await
    }

    #[inline]
    pub async fn read_tags(&self, server: &str, tags: impl IntoTags) -> OpcResult<TagValues> {
        self.read_tag_values(server, tags).await
    }

    #[inline]
    pub async fn read_tag(&self, server: &str, tag_id: &str) -> OpcResult<TagValue> {
        self.read_tag_value(server, tag_id).await
    }

    #[inline]
    pub async fn write_tags(
        &self,
        server: &str,
        writes: impl IntoWriteBatch,
    ) -> OpcResult<Vec<WriteResult>> {
        self.write_tag_batch(server, writes).await
    }

    #[inline]
    pub async fn write_tag(
        &self,
        server: &str,
        tag_id: &str,
        value: impl Into<OpcValue>,
    ) -> OpcResult<WriteResult> {
        self.write_tag_value(server, tag_id, value).await
    }

    #[inline]
    pub async fn browse(&self, server: &str, collector: TagCollector) -> OpcResult<Vec<String>> {
        self.browse_tags(server, collector).await
    }
}
```

#### 3. Strict Trait Boundary Invariant (`src/provider.rs`)
The SPI traits [`TagWriter`], [`TagReader`], [`TagBrowser`], and composite [`OpcProvider`] strictly retain concrete types:
- `TagWriter::write_tag_batch(&self, server: &str, writes: WriteBatch) -> impl Future<Output = OpcResult<Vec<WriteResult>>> + Send;`
- `TagWriter::write_tag_value(&self, server: &str, tag_id: &str, value: OpcValue) -> impl Future<Output = OpcResult<WriteResult>> + Send;`
- `TagReader::read_tag_values(&self, server: &str, tags: TagBatch) -> impl Future<Output = OpcResult<TagValues>> + Send;`
- `TagBrowser::browse_tags(&self, server: &str, collector: TagCollector) -> impl Future<Output = OpcResult<Vec<String>>> + Send;`
Inherent methods call `.into_write_batch()`, `.into()`, `.into_tag_batch()` before trait delegation. This preserves full dynamic dispatch and `mockall` compatibility.

---

### Blast Radius Table
| Symbol | File | Direct Callers | Indirect Deps | Test-Only | Cross-Package? |
|--------|------|:---:|:---:|:---:|:---:|
| `WriteBatch::from` generic impls | `src/types/write_batch.rs` | All `into_write_batch` callers | Facades | No | No |
| `write_tag_batch` genericized | `src/client/gateway.rs` | 1 (`batch_write_test.rs`) | Gateway | No | No |
| `write_tag_value` genericized | `src/client/gateway.rs` | 1 (`client/tests.rs`) | Gateway | No | No |
| `read_tags` alias | `src/client/gateway.rs` | 0 (New) | Unbound | No | No |
| `read_tag` alias | `src/client/gateway.rs` | 0 (New) | Unbound | No | No |
| `write_tags` alias | `src/client/gateway.rs` | 0 (New) | Unbound | No | No |
| `write_tag` alias | `src/client/gateway.rs` | 0 (New) | Unbound | No | No |
| `browse` alias | `src/client/gateway.rs` | 0 (New) | Unbound | No | No |
| 13 Inherent Session methods rustdoc | `src/client/session.rs` | None (Documentation) | Public docs | No | No |
| 7 Inherent Gateway methods rustdoc | `src/client/gateway.rs` | None (Documentation) | Public docs | No | No |
| `connect_eager` rustdoc | `src/client/typestate.rs` | None (Documentation) | Public docs | No | No |
| `bind_new_remote` rustdoc | `src/client/mod.rs` | None (Documentation) | Public docs | No | No |
| `OpcDaClient::new` rustdoc | `src/client/mod.rs` | None (Documentation) | Public docs | No | No |
| `get_value_checked` doctest | `src/types/collection.rs` | None (Documentation) | Public docs | No | No |
| `OpcDaClientBuilder` rustdoc | `src/client/builder.rs` | None (Documentation) | Public docs | No | No |

---

### Test Plan (TDD)
1. **`test_generic_into_write_batch_conversions` (`src/types/write_batch.rs`)**:
   - Verify array of tuples `[("Tag.1", 1.0f64), ("Tag.2", 2.0f64)]` converts into `WriteBatch::Owned`.
   - Verify array reference `&[("Tag.Ref1", 10i32), ("Tag.Ref2", 20i32)]` converts into `WriteBatch::Owned`.
   - Verify single tuple `("Tag.Solo", 42i32)` converts into `WriteBatch::Single`.
   - Verify vector `vec![("Tag.Bool".to_string(), true)]` converts into `WriteBatch::Owned`.
   - Verify borrowed slice `&[("Tag.Str1", "running"), ("Tag.Str2", "stopped")][..]` converts into `WriteBatch::Owned`.
   - Verify iterator collection `.collect::<WriteBatch>()` succeeds for polymorphic items.
2. **`test_unbound_write_tag_value_generic_primitives` (`src/client/tests.rs`)**:
   - Instantiate mock unbound client using `setup_mock_client()`.
   - Assert `client.write_tag_value("Mock.Server.1", "Sensor.Temp", 42.5f64)` succeeds.
   - Assert `client.write_tag_value("Mock.Server.1", "Motor.Speed", 100i32)` succeeds.
   - Assert `client.write_tag_value("Mock.Server.1", "Switch.Enabled", true)` succeeds.
   - Assert `client.write_tag_value("Mock.Server.1", "System.Status", "RUNNING")` succeeds.
3. **`test_unbound_write_tag_batch_into_write_batch` (`src/client/tests.rs`)**:
   - Assert passing array literal `[("Tag.1", 1.0f64), ("Tag.2", 2.0f64)]` writes both items atomically.
   - Assert passing borrowed slice `&slice_data[..]` writes both items.
   - Assert passing array reference `&slice_data` writes both items.
   - Assert passing single tuple `("Tag.Solo", 42i32)` writes item.
4. **`test_unbound_shorthand_aliases` (`src/client/tests.rs`)**:
   - Assert `client.read_tag("Mock.Server.1", "Random.Int4")` returns valid `TagValue`.
   - Assert `client.read_tags("Mock.Server.1", ["Random.Int4", "Random.Real8"])` returns `TagValues` with 2 items.
   - Assert `client.write_tag("Mock.Server.1", "Random.Int4", 99i32)` returns success `WriteResult`.
   - Assert `client.write_tags("Mock.Server.1", [("Tag.A", 10i32), ("Tag.B", 20i32)])` returns 2 success `WriteResult`s.
   - Assert `client.browse("Mock.Server.1", TagCollector::new(50))` returns tag list.
5. **`test_unbound_write_tag_generic_into_opc_value` (`src/client/tests.rs`)**:
   - Test `write_tag` alias with polymorphic primitives (`123.456f64`, `"operational"`, `false`).
6. **`TagValues::get_value_checked` Executable Doctest (`src/types/collection.rs`)**:
   - Verify doctest compiles and runs in Gate 3 (`cargo test -p opc-da-client --doc types::collection`).
7. **Client I/O Doctest Compilation Check**:
   - Verify all ````rust,no_run` doctests compile cleanly across `session.rs`, `gateway.rs`, `typestate.rs`, `mod.rs`, and `builder.rs`.

---

### Global Execution Order

Step 1: [TEST] `opc-da-client/src/types/write_batch.rs` — [+] unit test `test_generic_into_write_batch_conversions`
- Pre: ALL
- Target: `src/types/write_batch.rs:mod tests`
- Action:
  ```rust
  #[test]
  fn test_generic_into_write_batch_conversions() {
      let arr_batch = [("Tag.1", 1.0f64), ("Tag.2", 2.0f64)].into_write_batch();
      assert_eq!(arr_batch.len(), 2);
      let items: Vec<(&str, &OpcValue)> = arr_batch.iter().collect();
      assert_eq!(items[0], ("Tag.1", &OpcValue::Float(1.0)));
      assert_eq!(items[1], ("Tag.2", &OpcValue::Float(2.0)));

      let ref_arr_batch = (&[("Tag.Ref1", 10i32), ("Tag.Ref2", 20i32)]).into_write_batch();
      assert_eq!(ref_arr_batch.len(), 2);

      let single_batch = ("Tag.Solo", 42i32).into_write_batch();
      assert_eq!(single_batch.len(), 1);
      let items: Vec<(&str, &OpcValue)> = single_batch.iter().collect();
      assert_eq!(items[0], ("Tag.Solo", &OpcValue::Int(42)));

      let vec_batch = vec![("Tag.Bool".to_string(), true)].into_write_batch();
      assert_eq!(vec_batch.len(), 1);
      let items: Vec<(&str, &OpcValue)> = vec_batch.iter().collect();
      assert_eq!(items[0], ("Tag.Bool", &OpcValue::Bool(true)));

      let slice_data = [("Tag.Str1", "running"), ("Tag.Str2", "stopped")];
      let slice_batch = (&slice_data[..]).into_write_batch();
      assert_eq!(slice_batch.len(), 2);
      let items: Vec<(&str, &OpcValue)> = slice_batch.iter().collect();
      assert_eq!(items[0], ("Tag.Str1", &OpcValue::String("running".into())));
      assert_eq!(items[1], ("Tag.Str2", &OpcValue::String("stopped".into())));

      let iter_batch: WriteBatch = [("Tag.Iter", 999i32)].into_iter().collect();
      assert_eq!(iter_batch.len(), 1);
  }
  ```
- Post: RED(test_generic_into_write_batch_conversions)

Step 2: [MODIFY] `opc-da-client/src/types/write_batch.rs` — [-] delete 7 concrete `From` impls + 1 `FromIterator`, [+] add 6 generic `From` impls + 1 generic `FromIterator`
- Pre: RED(test_generic_into_write_batch_conversions)
- Target: `src/types/write_batch.rs:L332-396`
- Action:
  **DELETE** the following 8 existing implementations (they conflict with the new generic impls via `E0119`):
  1. `impl From<Vec<(String, OpcValue)>> for WriteBatch` (L332)
  2. `impl From<(String, OpcValue)> for WriteBatch` (L339)
  3. `impl From<(&str, OpcValue)> for WriteBatch` (L346)
  4. `impl From<&[(String, OpcValue)]> for WriteBatch` (L360)
  5. `impl From<&[(&str, OpcValue)]> for WriteBatch` (L367)
  6. `impl<const N: usize> From<[(String, OpcValue); N]> for WriteBatch` (L378)
  7. `impl<const N: usize> From<[(&str, OpcValue); N]> for WriteBatch` (L385)
  8. `impl<S: Into<String>> FromIterator<(S, OpcValue)> for WriteBatch` (L392)

  **PRESERVE** (do NOT delete):
  - `impl From<Arc<[(String, OpcValue)]>> for WriteBatch` (L353) — no coherence conflict with generic `(S, V)` impls.

  **ADD** the 6 generic `From` impls + 1 generic `FromIterator` as specified in Interface Contracts §1.
- Post: CHECKPOINT 🔒 GREEN(test_generic_into_write_batch_conversions)

Step 3: [TEST] `opc-da-client/src/client/tests.rs` — [+] helper fixture `setup_mock_client` & [+] unit test `test_unbound_write_tag_value_generic_primitives`
- Pre: GREEN(Step 2)
- Target: `src/client/tests.rs`
- Action:
  ```rust
  fn setup_mock_client() -> (OpcDaClient<MockServerConnector, Unbound>, std::sync::Arc<MockState>) {
      let state = std::sync::Arc::new(MockState::default());
      let connector = MockServerConnector::with_state(state.clone());
      let client = OpcDaClient::new(connector).expect("mock client should initialize");
      (client, state)
  }

  #[tokio::test]
  async fn test_unbound_write_tag_value_generic_primitives() {
      let (client, _state) = setup_mock_client();

      let res_f64 = client
          .write_tag_value("Mock.Server.1", "Sensor.Temp", 42.5f64)
          .await
          .expect("write f64 primitive must succeed");
      assert!(res_f64.is_success());
      assert_eq!(res_f64.tag_id, "Sensor.Temp");

      let res_i32 = client
          .write_tag_value("Mock.Server.1", "Motor.Speed", 100i32)
          .await
          .expect("write i32 primitive must succeed");
      assert!(res_i32.is_success());

      let res_bool = client
          .write_tag_value("Mock.Server.1", "Switch.Enabled", true)
          .await
          .expect("write bool primitive must succeed");
      assert!(res_bool.is_success());

      let res_str = client
          .write_tag_value("Mock.Server.1", "System.Status", "RUNNING")
          .await
          .expect("write &str primitive must succeed");
      assert!(res_str.is_success());
  }
  ```
- Post: RED(test_unbound_write_tag_value_generic_primitives)

Step 4: [MODIFY] `opc-da-client/src/client/gateway.rs` — [~] update `write_tag_value` signature to `value: impl Into<OpcValue>`
- Pre: RED(test_unbound_write_tag_value_generic_primitives)
- Target: `src/client/gateway.rs:35-42`
- Action:
  1. Add `IntoWriteBatch` to the `use crate::types::{...}` import block at the top of `gateway.rs` (currently missing — required by Steps 6 and 8).
  2. Change parameter from `value: OpcValue` to `value: impl Into<OpcValue>`, delegating `value.into()` to `TagWriter::write_tag_value`.
- Post: GREEN(test_unbound_write_tag_value_generic_primitives)

Step 5: [TEST] `opc-da-client/src/client/tests.rs` — [+] unit test `test_unbound_write_tag_batch_into_write_batch`
- Pre: GREEN(Step 4)
- Target: `src/client/tests.rs`
- Action:
  ```rust
  #[tokio::test]
  async fn test_unbound_write_tag_batch_into_write_batch() {
      let (client, _state) = setup_mock_client();

      let res_arr = client
          .write_tag_batch("Mock.Server.1", [("Tag.1", 1.0f64), ("Tag.2", 2.0f64)])
          .await
          .expect("write array batch must succeed");
      assert_eq!(res_arr.len(), 2);
      assert_eq!(res_arr[0].tag_id, "Tag.1");
      assert_eq!(res_arr[1].tag_id, "Tag.2");

      let slice_data = [("Tag.3", 3.0f64), ("Tag.4", 4.0f64)];
      let res_slice = client
          .write_tag_batch("Mock.Server.1", &slice_data[..])
          .await
          .expect("write slice batch must succeed");
      assert_eq!(res_slice.len(), 2);

      let res_ref_arr = client
          .write_tag_batch("Mock.Server.1", &slice_data)
          .await
          .expect("write array ref batch must succeed");
      assert_eq!(res_ref_arr.len(), 2);

      let res_single = client
          .write_tag_batch("Mock.Server.1", ("Tag.Solo", 42i32))
          .await
          .expect("write single tuple batch must succeed");
      assert_eq!(res_single.len(), 1);
      assert!(res_single[0].is_success());
  }
  ```
- Post: RED(test_unbound_write_tag_batch_into_write_batch)

Step 6: [MODIFY] `opc-da-client/src/client/gateway.rs` — [~] update `write_tag_batch` signature to `writes: impl IntoWriteBatch` and fix tracing attribute
- Pre: RED(test_unbound_write_tag_batch_into_write_batch)
- Target: `src/client/gateway.rs:44-52`
- Action:
  Change attribute from `#[tracing::instrument(level = "info", skip(self, writes), fields(write_count = writes.len()), err)]` to `#[tracing::instrument(level = "info", skip(self, writes), err)]`.
  Change parameter to `writes: impl IntoWriteBatch`, delegating `writes.into_write_batch()` to `TagWriter::write_tag_batch`.
- Post: GREEN(test_unbound_write_tag_batch_into_write_batch)

Step 7: [TEST] `opc-da-client/src/client/tests.rs` — [+] unit tests `test_unbound_shorthand_aliases` and `test_unbound_write_tag_generic_into_opc_value`
- Pre: GREEN(Step 6)
- Target: `src/client/tests.rs`
- Action:
  ```rust
  #[tokio::test]
  async fn test_unbound_shorthand_aliases() {
      let (client, _state) = setup_mock_client();

      let tag_val = client
          .read_tag("Mock.Server.1", "Random.Int4")
          .await
          .expect("read_tag alias must succeed");
      assert_eq!(tag_val.tag_id, "Random.Int4");
      assert!(tag_val.is_success());

      let tag_vals = client
          .read_tags("Mock.Server.1", ["Random.Int4", "Random.Real8"])
          .await
          .expect("read_tags alias must succeed");
      assert_eq!(tag_vals.len(), 2);
      assert!(tag_vals.contains_tag("Random.Int4"));
      assert!(tag_vals.contains_tag("Random.Real8"));

      let write_single = client
          .write_tag("Mock.Server.1", "Random.Int4", 99i32)
          .await
          .expect("write_tag alias must succeed");
      assert_eq!(write_single.tag_id, "Random.Int4");
      assert!(write_single.is_success());

      let write_multi = client
          .write_tags("Mock.Server.1", [("Tag.A", 10i32), ("Tag.B", 20i32)])
          .await
          .expect("write_tags alias must succeed");
      assert_eq!(write_multi.len(), 2);
      assert_eq!(write_multi[0].tag_id, "Tag.A");
      assert_eq!(write_multi[1].tag_id, "Tag.B");
      assert!(write_multi[0].is_success());
      assert!(write_multi[1].is_success());

      let collector = crate::types::TagCollector::new(50);
      let tags = client
          .browse("Mock.Server.1", collector)
          .await
          .expect("browse alias must succeed");
      assert!(!tags.is_empty());
  }

  #[tokio::test]
  async fn test_unbound_write_tag_generic_into_opc_value() {
      let (client, _state) = setup_mock_client();

      let res1 = client.write_tag("Mock.Server.1", "Tag.F64", 123.456f64).await.unwrap();
      assert!(res1.is_success());

      let res2 = client.write_tag("Mock.Server.1", "Tag.Str", "operational").await.unwrap();
      assert!(res2.is_success());

      let res3 = client.write_tag("Mock.Server.1", "Tag.Flag", false).await.unwrap();
      assert!(res3.is_success());
  }
  ```
- Post: RED(test_unbound_shorthand_aliases)

Step 8: [MODIFY] `opc-da-client/src/client/gateway.rs` — [+] add 5 shorthand aliases on `OpcDaClient<C, Unbound>` (`read_tags`, `read_tag`, `write_tags`, `write_tag`, `browse`)
- Pre: RED(test_unbound_shorthand_aliases)
- Target: `src/client/gateway.rs:62-100`
- Action:
  Implement `read_tags`, `read_tag`, `write_tags`, `write_tag`, and `browse` as `#[inline]` forwarding methods.
- Post: CHECKPOINT 🔒 GREEN(test_unbound_shorthand_aliases)

Step 9: [MODIFY] `opc-da-client/src/types/collection.rs` — [~] enhance `TagValues::get_value_checked` with `#[must_use]`, `# Panics`, and runnable `# Examples` doctest (existing Summary and `# Errors` preserved)
- Pre: GREEN(Step 8)
- Target: `src/types/collection.rs:448-468`
- Action:
  **Already present** (preserve as-is): Summary doc comment and `# Errors` section listing `NotRequested`, `NoValue`, `ReadFailed`.
  **Add** `#[must_use]` attribute, `# Panics` section ("This function does not panic."), and `# Examples` section with runnable doctest using `OpcValue::Float`:
  ```rust
  /// Looks up an OPC value by tag identifier with strict error checking.
  ///
  /// Unlike [`TagValues::get_value`], which collapses errors into `None`, this method distinguishes
  /// between tags that were never requested, tags that returned null/empty, and tags that failed.
  ///
  /// # Errors
  ///
  /// Returns [`TagExtractError::NotRequested`] if `tag` was not present in this read batch.
  /// Returns [`TagExtractError::NoValue`] if the server returned a null or empty variant.
  /// Returns [`TagExtractError::ReadFailed`] if the item read failed on the server with an [`OpcError`].
  ///
  /// # Panics
  ///
  /// This function does not panic.
  ///
  /// # Examples
  ///
  /// ```
  /// use opc_da_client::types::{OpcQuality, OpcValue, TagExtractError, TagValue, TagValues};
  ///
  /// let item = TagValue::new("Sensor.Temp", Some(OpcValue::Float(72.5)), OpcQuality::GOOD, None);
  /// let values = TagValues::new(vec![item]);
  ///
  /// let val = values.get_value_checked("sensor.temp").unwrap();
  /// assert_eq!(val, &OpcValue::Float(72.5));
  ///
  /// assert!(matches!(values.get_value_checked("Missing.Tag").unwrap_err(), TagExtractError::NotRequested(_)));
  /// ```
  #[must_use]
  pub fn get_value_checked(&self, tag: &str) -> Result<&OpcValue, TagExtractError>
  ```
- Post: CHECKPOINT 🔒 GREEN(cargo test -p opc-da-client --doc types::collection)

Step 10: [MODIFY] `opc-da-client/src/client/session.rs` — [~] add comprehensive rustdoc comments across all 13 inherent `Bound` methods (Finding #5)
- Pre: CHECK
- Target: `src/client/session.rs:22-117`
- Action:
  Add full rustdoc comments (`Summary`, `Details`, `# Errors`, `# Panics`, ````rust,no_run` `# Examples`) across:
  - `read_f64`, `read_i32`, `read_bool`, `read_string`, `read_f32`, `read_i64`, `read_u32`, `read_u64`
  - `read_tag`, `read_tags`
  - `write_tag`, `write_tags`
  - `browse`
- Post: CHECK

Step 11: [MODIFY] `opc-da-client/src/client/gateway.rs` — [~] add comprehensive rustdoc comments across all 7 original methods and 5 alias methods (Finding #5)
- Pre: CHECK
- Target: `src/client/gateway.rs:14-100`
- Action:
  Add full rustdoc comments (`Summary`, `Details`, `# Errors`, `# Panics`, ````rust,no_run` `# Examples`) across:
  - `list_servers`, `list_server_details`
  - `browse_tags`, `browse`
  - `write_tag_value`, `write_tag`
  - `write_tag_batch`, `write_tags`
  - `read_tag_values`, `read_tags`
  - `read_tag_value`, `read_tag`
- Post: CHECK

Step 12: [MODIFY] `opc-da-client/src/client/typestate.rs` & `src/client/mod.rs` — [~] document `connect_eager`, `bind_new_remote`, and `OpcDaClient::new` (Finding #15)
- Pre: CHECK
- Target: `src/client/typestate.rs:86-96`, `src/client/mod.rs:101-131`
- Action:
  1. In `typestate.rs`: `connect_eager` already has Summary doc. **Add** `# Errors` (documenting `OpcError::Connection`, `OpcError::Timeout`, `OpcError::Worker`), `# Panics`, and ````rust,no_run` example.
  2. In `client/mod.rs`: `bind_new_remote` already has Summary + `# Errors`. **Add** `# Panics` and ````rust,no_run` example.
  3. In `client/mod.rs`: `OpcDaClient::new` has **no rustdoc**. **Add** full Summary, Details, `# Errors` (documenting `OpcError::Worker`), `# Panics`, and ````rust,no_run` example.
- Post: CHECK

Step 13: [MODIFY] `opc-da-client/src/client/builder.rs` — [~] enhance existing rustdoc on `OpcDaClientBuilder` constructors and build methods with `# Errors`, `# Panics`, `# Examples` (Finding #15)
- Pre: CHECK
- Target: `src/client/builder.rs:25-160`
- Action:
  All 5 target methods already have summary rustdoc comments. `build_bound` already has `# Errors`. **Enhance** (do not replace existing summaries) with missing sections:
  - `new`: **Add** `# Examples`
  - `new_with_connector`: **Add** `# Examples`
  - `build` (currently summary-only): **Add** `# Errors` (documenting `OpcError::Worker`), `# Panics`, `# Examples`
  - `build_with_connector` (currently summary-only): **Add** `# Errors` (documenting `OpcError::Worker`), `# Panics`, `# Examples`
  - `build_bound` (has summary + `# Errors`): **Add** `# Panics`, `# Examples`
- Post: CHECK

Step 14: [TEST] Workspace integration & quality verification — `pwsh -File scripts/verify.ps1` 🔒
- Pre: ALL
- Target: Workspace root
- Action: Execute full 9-gate quality pipeline to guarantee zero warnings under `-D warnings`.
- Post: VERIFIED 🔒

---

### Verification Plan
| Type | Command | Expected Result |
|------|---------|-----------------|
| WriteBatch Unit Tests | `cargo test -p opc-da-client --lib types::write_batch` | All generic batch conversion tests pass |
| Client Facade Unit Tests | `cargo test -p opc-da-client --lib client::tests` | All alias, generic write, and bound tests pass |
| Collection Unit & Doc Tests | `cargo test -p opc-da-client --lib types::collection` | `get_value_checked` unit tests and doctests pass |
| Doctest Compilation Check (Gate 3) | `cargo test -p opc-da-client --doc` | All doctests compile and pass with zero warnings |
| Full 9-Gate Pipeline | `pwsh -File scripts/verify.ps1` | All 9 gates pass with exit code 0 |

---

### Plan Summary
| Metric | Value |
|---|---|
| Tier | M (Feature / Ergonomic Symmetry & Documentation) |
| Files Modified | 7 (`types/write_batch.rs`, `client/gateway.rs`, `client/session.rs`, `types/collection.rs`, `client/typestate.rs`, `client/mod.rs`, `client/builder.rs`) |
| Test Files Extended | 2 (`types/write_batch.rs:mod tests`, `client/tests.rs`) |
| Steps | 14 steps |
| Checkpoints | 4 (Step 2, Step 8, Step 9, Step 14) |
| Estimated Effort | Medium |
