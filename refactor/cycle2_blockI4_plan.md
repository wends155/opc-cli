# Implementation Plan: Sub-Block I4 — WriteBatch Encapsulation & Small String Optimization (SSO)

| Role | Date | Scope | Tier | Target Path | Reference Review |
|:---|:---|:---|:---:|:---|:---|
| Lead Architect | 2026-09-18 | Sub-Block I4 (`WriteBatch` SSO & Encapsulation) | **M** | [`refactor/cycle2_blockI4_plan.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockI4_plan.md) | [`refactor/cycle2_blockI4_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockI4_review.md) |

---

### Builder Context & Orientation

This plan directs the modernization of [`WriteBatch`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs#L123) in `opc-da-client`. It encapsulates the public enum into an opaque struct (`pub struct WriteBatch { pub(crate) repr: WriteBatchRepr }`) and adds zero-allocation Small String Optimization (SSO) via `InlineSingle([u8; 31], u8, OpcValue)` and `StaticSingle(&'static str, OpcValue)`.

- **Compiler Baseline:** Rust 2024 (MSRV 1.93.1), `windows` 0.61.3, `tokio` multi-thread runtime.
- **Verification Command:** `powershell -File scripts/verify.ps1` (all 8 gates must pass with 0 warnings and 0 errors).
- **Blast Radius:** Tightly bounded to exactly 3 internal files in `opc-da-client/src/` (`types/write_batch.rs`, `com/worker/write.rs`, `provider.rs`) and 1 documentation file (`spec.md`). Exactly **0 call sites** in `opc-cli/` or integration test suites require modification.
- **Execution Style:** Single atomic sweep with zero deprecated stubs.

#### Read Before Starting:
- [`opc-da-client/src/types/batch.rs:1-125, 330-440`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/batch.rs#L1-L125) (`TagBatch` encapsulation & `IntoTags` reference architecture)
- [`opc-da-client/src/types/write_batch.rs:110-475`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs#L110-L475) (Existing `WriteBatch` enum, iterators, and trait implementations)
- [`opc-da-client/src/com/worker/write.rs:30-65, 145-165`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/write.rs#L30-L65) (`handle_write` scalar delegation and batch partitioning)
- [`opc-da-client/src/provider.rs:510-575`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/provider.rs#L510-L575) (Mock provider unit tests requiring `.into_write_batch()` migration)
- [`opc-da-client/spec.md:155-192, 640-660`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/spec.md#L155-L192) (`WriteBatch` specification & method deprecation table)

---

### Plan Objectives

| ID | Objective | Success Criteria | Steps |
|:---|:---|:---|:---|
| **O1** | Encapsulate `WriteBatch` behind an opaque struct with crate-private `WriteBatchRepr` | `WriteBatch` is an opaque struct (`pub struct WriteBatch { pub(crate) repr: WriteBatchRepr }`); representation enum is `pub(crate)`; zero representation leakage | Step 1, Step 3 |
| **O2** | Implement 31-byte stack Small String Optimization (SSO) and static literal storage | `InlineSingle([u8; 31], u8, OpcValue)` and `StaticSingle(&'static str, OpcValue)` eliminate 100% of heap allocations on scalar writes $\le 31$ bytes; `WriteBatch` size is exactly 72 bytes (align 8, 0 internal padding) | Step 2, Step 3 |
| **O3** | Resolve trait coherence (`E0119`, `E0277`) and enforce `Send` supertrait bound | Blanket tuple `From` and blanket `IntoWriteBatch` removed; concrete disjoint `From` for tuples; fully generic collection implementations in `IntoWriteBatch: Send` with `V: Sync` on borrowed slices | Step 4, Step 5 |
| **O4** | Implement semantic sequence `PartialEq` and exact size iterators | Manual sequence equality (`self.len() == other.len() && self.iter().eq(other.iter())`) equates all 5 variant representations; iterators enforce monotonic `ExactSizeIterator` | Step 6, Step 7 |
| **O5** | Modernize worker scalar write delegation and internal test fixtures | `handle_write` delegates via `WriteBatch::from_str_lenient`; mock tests in `provider.rs` use `.into_write_batch()`; zero deprecated stubs; `spec.md` updated | Step 8, Step 9, Step 10, Step 11 |

---

## 1. Problem Statement & Architectural Context

In the current codebase, `WriteBatch` ([`opc-da-client/src/types/write_batch.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs)) is an exposed public enum with three variants: `Single(String, OpcValue)`, `Shared(Arc<[(String, OpcValue)]>)`, and `Owned(Vec<(String, OpcValue)>)`. This exposes internal storage mechanics, prevents future non-breaking storage optimizations, and forces a heap allocation (`String`) for every scalar write operation. In high-frequency industrial automation control loops (e.g. 50 Hz writes via `OpcDaClient::write_tags` or COM worker `handle_write`), this creates unnecessary allocator churn and cache pollution.

Furthermore, Sub-Block I1 introduced an opaque encapsulation and Small String Optimization (SSO) pattern for tag reads via [`TagBatch`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/batch.rs), leaving `WriteBatch` architecturally asymmetric. 

This implementation plan modernizes `WriteBatch` by:
1. Encapsulating it behind an opaque struct backed by a crate-private 5-variant representation enum (`StaticSingle`, `InlineSingle([u8; 31], u8, OpcValue)`, `OwnedSingle`, `Shared`, `Owned`).
2. Achieving a zero-allocation 72-byte memory layout (with 0 internal padding before `OpcValue`) for tags $\le 31$ bytes and static string literals.
3. Resolving trait coherence collisions (`E0119`) via disjoint concrete `From` implementations for tuples and fully generic collection implementations in `IntoWriteBatch: Send` (with `V: Sync` for borrowed slices avoiding `E0277`).
4. Implementing semantic sequence equality (`PartialEq`) across all 5 storage representations.
5. Mitigating critical security hazards: UTF-8 codepoint tearing (CWE-20 / CWE-787), BSTR null-byte truncation (CWE-626), length calculation errors (CWE-682), and allocation starvation (CWE-400).
6. Modernizing COM worker delegation (`handle_write`), migrating mock provider test call sites, and synchronizing `spec.md` with zero deprecated stubs.

---

## 2. User Decisions & Pre-Planning Alignment

From the Pre-Planning Interview (`/grill-me`), the following core decisions govern this plan:
1. **Target Plan Path:** `refactor/cycle2_blockI4_plan.md` exclusively.
2. **Scope Boundary:** Full scope encompassing `WriteBatch` modernization, COM worker delegation, mock provider updates, comprehensive unit and integration tests, AND `spec.md` synchronization (Finding 11).
3. **Execution Phasing:** Single atomic execution block with all type, caller, and test updates implemented in a single unified sweep.
4. **Verification Depth:** Standard unit and integration test suite with explicit boundary tests (0-byte, 31-byte, 32-byte spillover, multibyte UTF-8, null bytes) and $5 \times 5$ cross-variant equality permutations.
5. **Clean Replacement:** Zero deprecated stubs (`#[deprecated]`). Blast radius is confirmed to be 0 external callers and 0 `opc-cli` callers.

---

## 3. Blast Radius & Negative Scope Analysis

### Blast Radius Matrix

| Component / File | Impact Level | Call Sites Affected | Planned Change |
|:---|:---:|:---:|:---|
| [`opc-da-client/src/types/write_batch.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs) | `[MODIFY]` | Core Definition | Encapsulate `WriteBatch`, add `WriteBatchRepr` with 5 variants, add stack SSO, implement sequence `PartialEq`, iterators, trait impls, and doc-tests. |
| [`opc-da-client/src/com/worker/write.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/write.rs) | `[MODIFY]` | Line 157 | Migrate `handle_write` to construct batch via `WriteBatch::from_str_lenient(tag_id, value.clone())` instead of `&WriteBatch::Single(...)`. |
| [`opc-da-client/src/provider.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/provider.rs) | `[MODIFY]` | Lines 517, 566 | Update mock provider unit tests to use `.into_write_batch()` instead of private `WriteBatch::Owned(...)`. |
| [`opc-da-client/spec.md`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/spec.md) | `[MODIFY]` | Lines 188, 645-646 | Synchronize stale `write_batch` reference to `write_tags`; prune deprecated table rows under clean slate. |
| `opc-cli/` application crate | `[NONE]` | 0 occurrences | Zero usages of `WriteBatch` or any variant across the CLI application. |
| `opc-da-client/tests/batch_write_test.rs` | `[TEST]` | 8 test functions | Zero breaking changes. Tests use `.into_write_batch()` and will pass unchanged. |

### Negative Scope (Explicit Exclusions)
- **NO Changes to `TagBatch`:** [`opc-da-client/src/types/batch.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/batch.rs) was modernized in Sub-Block I1 and remains completely untouched.
- **NO Changes to `OpcValue` Layout:** [`opc-da-client/src/types/value.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/value.rs) remains untouched.
- **NO Changes to `opc-cli`:** The CLI crate does not use `WriteBatch` directly and requires 0 edits.
- **NO Deprecated Aliases:** Deprecated aliases or bridges (e.g. `WriteBatch::Single`) are explicitly excluded.
- **NO New Dependencies:** Zero external crates added to `Cargo.toml`.

---

## 4. Formal Interface Contracts

| Symbol | Signature | Stability | Ownership / Borrowing | Error Contract | Notes |
|---|---|---|---|---|---|
| `struct WriteBatch` | `pub struct WriteBatch { pub(crate) repr: WriteBatchRepr }` | `[MODIFY]` | Owns internal `WriteBatchRepr` (72B, align 8) | Infallible | Opaque struct encapsulating 5 storage representations; establishes symmetry with `TagBatch` |
| `WriteBatch::empty()` | `pub const fn empty() -> Self` | `[MODIFY]` | Moves empty `Owned(Vec::new())` | Infallible | Zero-allocation `const` constructor |
| `WriteBatch::from_str_lenient()` | `pub fn from_str_lenient(tag: &str, val: impl Into<OpcValue>) -> Self` | `[NEW]` | Borrows `&str`, takes owned/convertible `val` | Infallible | 31-byte stack SSO; falls back cleanly to `OwnedSingle` without slicing or codepoint tearing |
| `WriteBatch::from_static()` | `pub fn from_static(tag: &'static str, val: impl Into<OpcValue>) -> Self` | `[NEW]` | Takes `'static str`, takes owned/convertible `val` | Infallible | Inherent constructor for zero-allocation compile-time static literals of arbitrary length |
| `WriteBatch::len(&self)` | `pub fn len(&self) -> usize` | `[MODIFY]` | Borrows `&self` | Infallible | Returns constant `1` for all scalar variants; prevents CWE-682 byte-length hazard |
| `WriteBatch::is_empty(&self)` | `pub fn is_empty(&self) -> bool` | `[MODIFY]` | Borrows `&self` | Infallible | `self.len() == 0` |
| `WriteBatch::as_slice(&self)` | `pub fn as_slice(&self) -> Option<&[(String, OpcValue)]>` | `[MODIFY]` | Borrows `&self`, returns `Option<&[...]>` | Infallible | Returns `Some` only when backed by contiguous heap storage (`Owned` \| `Shared`) |
| `WriteBatch::into_shareable()` | `pub fn into_shareable(self) -> Self` | `[MODIFY]` | Consumes `self`, returns `Self` | Infallible | Preserves `StaticSingle` and `InlineSingle` on stack; promotes `OwnedSingle` and `Owned` to `Shared(Arc<...>)` |
| `WriteBatch::into_vec()` | `pub fn into_vec(self) -> Vec<(String, OpcValue)>` | `[NEW]` | Consumes `self`, returns `Vec<(String, OpcValue)>` | Infallible | Consumes batch and returns vector; zero reallocation for `Owned` |
| `<WriteBatch as Default>::default()` | `fn default() -> Self` | `[MODIFY]` | Infallible, returns `Self` | Infallible | Delegates to `WriteBatch::empty()` |
| `WriteBatch::iter(&self)` | `pub fn iter(&self) -> WriteBatchIter<'_>` | `[MODIFY]` | Borrows `&self`, yields `(&str, &OpcValue)` | Infallible | Zero-allocation iterator across all 5 representations |
| `struct WriteBatchIter<'a>` | `pub struct WriteBatchIter<'a> { ... }` | `[MODIFY]` | Borrows `&'a WriteBatch` | Infallible | Implements `Iterator`, `ExactSizeIterator`, `FusedIterator` |
| `struct WriteBatchIntoIter` | `pub struct WriteBatchIntoIter { ... }` | `[MODIFY]` | Owns consumed items | Infallible | Implements `Iterator`, `ExactSizeIterator`, `FusedIterator` |
| `<WriteBatch as PartialEq>::eq()` | `fn eq(&self, other: &Self) -> bool` | `[MODIFY]` | Borrows `&self`, `&other` | Infallible | Sequence equality via `self.len() == other.len() && self.iter().eq(other.iter())` |
| `<WriteBatch as IntoIterator>::into_iter()` | `fn into_iter(self) -> WriteBatchIntoIter` | `[MODIFY]` | Consumes `self`, yields `(String, OpcValue)` | Infallible | Consuming iterator |
| `<&'a WriteBatch as IntoIterator>::into_iter()` | `fn into_iter(self) -> WriteBatchIter<'a>` | `[MODIFY]` | Borrows `&'a self`, yields `(&'a str, &'a OpcValue)` | Infallible | Delegates to `self.iter()` |
| `<WriteBatch as From<(&'static str, V)>>::from()` | `fn from(pair: (&'static str, V)) -> Self` | `[NEW]` | Takes `'static` str & convertible `V` | Infallible | Routes static literals directly to `StaticSingle` with 0 heap allocations |
| `<WriteBatch as From<(String, V)>>::from()` | `fn from(pair: (String, V)) -> Self` | `[NEW]` | Takes owned `String` & convertible `V` | Infallible | Routes owned strings to `OwnedSingle` |
| `<WriteBatch as From<Vec<(String, OpcValue)>>>::from()` | `fn from(vec: Vec<(String, OpcValue)>) -> Self` | `[NEW]` | Takes owned `Vec<(String, OpcValue)>` | Infallible | Direct zero-reallocation ingestion into `Owned` |
| `<WriteBatch as From<Arc<[(String, OpcValue)]>>>::from()` | `fn from(arc: Arc<[(String, OpcValue)]>) -> Self` | `[MODIFY]` | Takes owned `Arc` | Infallible | Shared slice ingestion |
| `<WriteBatch as From<&Self>>::from()` | `fn from(batch: &Self) -> Self` | `[MODIFY]` | Borrows `&WriteBatch` | Infallible | Delegates to `batch.clone()` |
| `<WriteBatch as FromIterator<(S, V)>>::from_iter()` | `fn from_iter<I: IntoIterator<Item = (S, V)>>(iter: I) -> Self` | `[MODIFY]` | Consumes iterator | Infallible | Collects pairs into `WriteBatchRepr::Owned` |
| `trait IntoWriteBatch` | `pub trait IntoWriteBatch: Send { fn into_write_batch(self) -> WriteBatch; }` | `[MODIFY]` | Consumes `self`, returns `WriteBatch` | Infallible | Added `Send` supertrait bound for MTA thread-safety and symmetry with `IntoTags` |
| `<(&'a str, V) as IntoWriteBatch>::into_write_batch()` | `fn into_write_batch(self) -> WriteBatch` | `[NEW]` | Consumes tuple `(&str, V)` | Infallible | Delegates to `WriteBatch::from_str_lenient(self.0, self.1)` |
| `<(String, V) as IntoWriteBatch>::into_write_batch()` | `fn into_write_batch(self) -> WriteBatch` | `[NEW]` | Consumes tuple `(String, V)` | Infallible | Delegates to `WriteBatch::from((self.0, self.1))` |
| `<[(S, V); N] as IntoWriteBatch>::into_write_batch()` | `fn into_write_batch(self) -> WriteBatch` | `[MODIFY]` | Consumes array, `S: Into<String> + Send`, `V: Into<OpcValue> + Send` | Infallible | Fully generic over string and value types |
| `<&'a [(S, V); N] as IntoWriteBatch>::into_write_batch()` | `fn into_write_batch(self) -> WriteBatch` | `[MODIFY]` | Borrows array, `S: AsRef<str> + Sync`, `V: Clone + Into<OpcValue> + Send + Sync` | Infallible | Enforces `Send + Sync` on borrowed slice references |
| `<&'a [(S, V)] as IntoWriteBatch>::into_write_batch()` | `fn into_write_batch(self) -> WriteBatch` | `[MODIFY]` | Borrows slice, `S: AsRef<str> + Sync`, `V: Clone + Into<OpcValue> + Send + Sync` | Infallible | Enforces `Send + Sync` on borrowed slice references |
| `<Vec<(S, V)> as IntoWriteBatch>::into_write_batch()` | `fn into_write_batch(self) -> WriteBatch` | `[MODIFY]` | Consumes vector, `S: Into<String> + Send`, `V: Into<OpcValue> + Send` | Infallible | Fully generic over string and value types |
| `<WriteBatch as IntoWriteBatch>::into_write_batch()` | `fn into_write_batch(self) -> WriteBatch` | `[NEW]` | Consumes `WriteBatch` | Infallible | Identity move |
| `<&WriteBatch as IntoWriteBatch>::into_write_batch()` | `fn into_write_batch(self) -> WriteBatch` | `[NEW]` | Borrows `&WriteBatch` | Infallible | Clones batch |
| `<Arc<[(String, OpcValue)]> as IntoWriteBatch>::into_write_batch()` | `fn into_write_batch(self) -> WriteBatch` | `[NEW]` | Consumes `Arc` | Infallible | Ingests into `WriteBatchRepr::Shared` |

---

## 5. Global Execution Order (Topologically Sorted & TDD-First)

The implementation order follows strict bottom-up dependency ordering in a single atomic sweep:

```
Step 1: [TEST] types/write_batch.rs (Scaffold comprehensive unit tests)
  │
  ▼
Step 2: [MODIFY] types/write_batch.rs (Core Struct, Repr, Constructors, SSO, len)
  │
  ▼
Step 3: [MODIFY] types/write_batch.rs (Lifecycle: into_shareable, as_slice, into_vec)
  │
  ▼
Step 4: [MODIFY] types/write_batch.rs (Iterators, ExactSizeIterator, Sequence PartialEq)
  │
  ▼
Step 5: [MODIFY] types/write_batch.rs (Trait Impls: From, IntoWriteBatch: Send, Doctests)
  │
  ▼
Step 6: [MODIFY] com/worker/write.rs (Worker handle_write SSO Delegation)
  │
  ▼
Step 7: [MODIFY] provider.rs (Mock Provider Test Migration)
  │
  ▼
Step 8: [MODIFY] spec.md (Specification Alignment & Clean Slate Pruning)
  │
  ▼
Step 9: [VERIFY] Workspace Verification Pipeline (fmt -> clippy -> test)
```

### Detailed Execution Steps

- [ ] **Step 1: [TEST] `opc-da-client/src/types/write_batch.rs` — Comprehensive Unit Test Scaffolding**
  - **Pre:** `opc-da-client/src/types/write_batch.rs` has open enum representation.
  - **Target:** [`opc-da-client/src/types/write_batch.rs:500+`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs)
  - **Action:**
    - Note that existing test [`test_write_batch_into_shareable_lifecycle`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs#L633-L682) (which pattern-matches on `WriteBatch::Single`) is replaced by `test_write_batch_into_shareable_stack_preservation` using public methods (`.len()`, `.as_slice()`, `.iter()`).
    - Add comprehensive unit test functions in `mod tests`:
      - `test_write_batch_sso_boundary_ascii`: 0-byte (`""`), 1-byte (`"A"`), 31-byte boundary (`InlineSingle`), 32-byte spillover (`OwnedSingle`).
      - `test_write_batch_sso_multibyte_utf8_safety`: 2-byte Cyrillic, 3-byte CJK, 4-byte emoji boundary transitions, and corrupted byte prefix recovery via `inline_as_str`.
      - `test_write_batch_interior_null_bytes_preserved`: Raw byte preservation for tags containing `\0` across inline, static, and owned variants (CWE-626 defense).
      - `test_write_batch_len_invariants_cwe682`: Constant 1 returned for `StaticSingle`, `InlineSingle`, `OwnedSingle`, never tag byte length.
      - `test_write_batch_cross_variant_partial_eq_permutations`: All 25 pairwise equality permutations equating logically identical batches across all 5 representations.
      - `test_write_batch_into_shareable_stack_preservation`: `StaticSingle` and `InlineSingle` retained on stack; `OwnedSingle` and `Owned` promoted to `Shared(Arc<...>)`.
      - `test_write_batch_iterators_all_variants`: Monotonic `ExactSizeIterator` length and item yielding across borrowed and consuming iterators for all 5 variants.
      - `test_write_batch_from_conversions_coherence`: Trait routing verification for `&'static str` vs `String` vs `Vec` without `E0119`.
      - `test_into_write_batch_send_bound`: Compile-time `Send` supertrait validation across channel boundaries.
  - **Post:** `RED (cargo test -p opc-da-client --lib types::write_batch::tests expects: exit non-zero)`

- [ ] **Step 2: [MODIFY] `opc-da-client/src/types/write_batch.rs` — Core Struct Encapsulation, Repr & Constructors**
  - **Pre:** `WriteBatch` is an open public enum.
  - **Target:** [`opc-da-client/src/types/write_batch.rs:105-180`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs#L105-L180)
  - **Action:**
    - Declare opaque struct:
      ```rust
      #[derive(Debug, Clone)]
      pub struct WriteBatch {
          pub(crate) repr: WriteBatchRepr,
      }
      ```
    - Declare crate-private algebraic representation:
      ```rust
      #[derive(Debug, Clone)]
      pub(crate) enum WriteBatchRepr {
          StaticSingle(&'static str, OpcValue),
          InlineSingle([u8; 31], u8, OpcValue),
          OwnedSingle(String, OpcValue),
          Shared(Arc<[(String, OpcValue)]>),
          Owned(Vec<(String, OpcValue)>),
      }
      ```
    - Implement inherent constructors and accessors on `WriteBatch`:
      - `pub const fn empty() -> Self` backed by `WriteBatchRepr::Owned(Vec::new())`.
      - `pub fn from_static(tag: &'static str, val: impl Into<OpcValue>) -> Self` backed by `WriteBatchRepr::StaticSingle(tag, val.into())`.
      - `pub fn from_str_lenient(tag: &str, val: impl Into<OpcValue>) -> Self`:
        ```rust
        let opc_val = val.into();
        if tag.len() <= 31 {
            let mut buf = [0u8; 31];
            buf[..tag.len()].copy_from_slice(tag.as_bytes());
            #[allow(clippy::cast_possible_truncation)]
            Self {
                repr: WriteBatchRepr::InlineSingle(buf, tag.len() as u8, opc_val),
            }
        } else {
            Self {
                repr: WriteBatchRepr::OwnedSingle(tag.to_string(), opc_val),
            }
        }
        ```
      - Helper `inline_as_str(buf: &[u8; 31], len: u8) -> &str` with `min(31)` clamping and `valid_up_to()` recovery.
      - `pub fn len(&self) -> usize`:
        ```rust
        match &self.repr {
            WriteBatchRepr::StaticSingle(_, _)
            | WriteBatchRepr::InlineSingle(_, _, _)
            | WriteBatchRepr::OwnedSingle(_, _) => 1,
            WriteBatchRepr::Shared(slice) => slice.len(),
            WriteBatchRepr::Owned(vec) => vec.len(),
        }
        ```
      - `pub fn is_empty(&self) -> bool` (`self.len() == 0`).
  - **Post:** `CHECK (cargo check -p opc-da-client expects: exit 0)`

- [ ] **Step 3: [MODIFY] `opc-da-client/src/types/write_batch.rs` — Lifecycle Optimization & Slice Projection**
  - **Pre:** `into_shareable` wraps single items unconditionally in `Arc`.
  - **Target:** [`opc-da-client/src/types/write_batch.rs:185-250`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs#L185-L250)
  - **Action:**
    - Implement `WriteBatch::as_slice(&self) -> Option<&[(String, OpcValue)]>` returning `Some` for `Shared` and `Owned`, `None` for single-tag variants.
    - Implement `WriteBatch::into_shareable(self) -> Self`:
      ```rust
      match self.repr {
          WriteBatchRepr::StaticSingle(tag, val) => Self {
              repr: WriteBatchRepr::StaticSingle(tag, val),
          },
          WriteBatchRepr::InlineSingle(buf, len, val) => Self {
              repr: WriteBatchRepr::InlineSingle(buf, len, val),
          },
          WriteBatchRepr::Shared(slice) => Self {
              repr: WriteBatchRepr::Shared(slice),
          },
          WriteBatchRepr::OwnedSingle(tag, val) => Self {
              repr: WriteBatchRepr::Shared(Arc::from([(tag, val)])),
          },
          WriteBatchRepr::Owned(vec) => Self {
              repr: WriteBatchRepr::Shared(Arc::from(vec.into_boxed_slice())),
          },
      }
      ```
    - Implement `WriteBatch::into_vec(self) -> Vec<(String, OpcValue)>`:
      ```rust
      match self.repr {
          WriteBatchRepr::StaticSingle(tag, val) => vec![(tag.to_string(), val)],
          WriteBatchRepr::InlineSingle(buf, len, val) => vec![(Self::inline_as_str(&buf, len).to_string(), val)],
          WriteBatchRepr::OwnedSingle(tag, val) => vec![(tag, val)],
          WriteBatchRepr::Shared(slice) => slice.to_vec(),
          WriteBatchRepr::Owned(vec) => vec,
      }
      ```
    - Implement `<WriteBatch as Default>::default() -> Self` delegating to `Self::empty()`.
  - **Post:** `CHECK (cargo check -p opc-da-client expects: exit 0)`

- [ ] **Step 4: [MODIFY] `opc-da-client/src/types/write_batch.rs` — Iterators & Sequence Equality**
  - **Pre:** `PartialEq` derived on enum; iterators assume 3-variant enum.
  - **Target:** [`opc-da-client/src/types/write_batch.rs:255-365`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs#L255-L365)
  - **Action:**
    - Define `WriteBatchIter<'a>` struct wrapping `WriteBatchIterInner<'a>` (`Single`, `Slice`).
    - Implement `Iterator`, `ExactSizeIterator`, and `FusedIterator` for `WriteBatchIter<'a>`.
    - Define `WriteBatchIntoIter` struct wrapping `WriteBatchIntoIterInner` (`Single`, `Shared`, `Owned`).
    - Implement `Iterator`, `ExactSizeIterator`, and `FusedIterator` for `WriteBatchIntoIter`.
    - Implement `IntoIterator` for `&'a WriteBatch` and `WriteBatch`.
    - Implement manual sequence equality `<WriteBatch as PartialEq>::eq`:
      ```rust
      impl PartialEq for WriteBatch {
          #[inline]
          fn eq(&self, other: &Self) -> bool {
              self.len() == other.len() && self.iter().eq(other.iter())
          }
      }
      ```
  - **Post:** `CHECK (cargo check -p opc-da-client expects: exit 0)`

- [ ] **Step 5: [MODIFY] `opc-da-client/src/types/write_batch.rs` — Trait Coherence, Generic Collections & Rustdoc Governance**
  - **Pre:** Blanket `From<(S, V)>` and blanket `IntoWriteBatch` present; missing doc-tests.
  - **Target:** [`opc-da-client/src/types/write_batch.rs:33-100, 370-490`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs)
  - **Action:**
    - Remove blanket `From<(S, V)>` and blanket `IntoWriteBatch for T`.
    - Implement concrete disjoint `From` conversions for tuples and direct vectors:
      - `impl<V: Into<OpcValue>> From<(&'static str, V)> for WriteBatch` $\rightarrow$ `StaticSingle`
      - `impl<V: Into<OpcValue>> From<(String, V)> for WriteBatch` $\rightarrow$ `OwnedSingle`
      - `impl From<Vec<(String, OpcValue)>> for WriteBatch` $\rightarrow$ `Owned` (move-only $O(1)$)
      - `impl From<Arc<[(String, OpcValue)]>> for WriteBatch` $\rightarrow$ `Shared`
      - `impl From<&Self> for WriteBatch` $\rightarrow$ `self.clone()`
      - `impl<S: Into<String>, V: Into<OpcValue>> FromIterator<(S, V)> for WriteBatch`
    - Define `pub trait IntoWriteBatch: Send` with discrete tuple and fully generic collection implementations (enforcing `V: Sync` on borrowed slice references to avoid `E0277`):
      - `impl IntoWriteBatch for WriteBatch`
      - `impl IntoWriteBatch for &WriteBatch`
      - `impl IntoWriteBatch for Arc<[(String, OpcValue)]>`
      - `impl<'a, V: Into<OpcValue> + Send> IntoWriteBatch for (&'a str, V)` $\rightarrow$ delegates to `from_str_lenient`
      - `impl<V: Into<OpcValue> + Send> IntoWriteBatch for (String, V)` $\rightarrow$ delegates to `From`
      - `impl<S: Into<String> + Send, V: Into<OpcValue> + Send> IntoWriteBatch for Vec<(S, V)>`
      - `impl<S: Into<String> + Send, V: Into<OpcValue> + Send, const N: usize> IntoWriteBatch for [(S, V); N]`
      - `impl<'a, S: AsRef<str> + Sync, V: Clone + Into<OpcValue> + Send + Sync, const N: usize> IntoWriteBatch for &'a [(S, V); N]`
      - `impl<'a, S: AsRef<str> + Sync, V: Clone + Into<OpcValue> + Send + Sync> IntoWriteBatch for &'a [(S, V)]`
    - Add `# Panics` declarations and runnable `# Examples` doc-tests for `WriteResult` accessors (`success`, `failure`, `is_success`, `is_error`, `error`) and `WriteBatch` public methods (`empty`, `from_static`, `from_str_lenient`, `len`, `is_empty`, `as_slice`, `into_shareable`, `into_vec`, `iter`) per `coding-standard.md §4.5`.
  - **Post:** `TEST (cargo test -p opc-da-client --lib types::write_batch::tests expects: exit 0)`

- [ ] **🔒 CHECKPOINT 1: `write_batch.rs` Verification**
  - Run `cargo test -p opc-da-client --lib types::write_batch`
  - Invariants confirmed: 72-byte size, stack SSO, zero-allocation literals, sequence equality, trait coherence.

- [ ] **Step 6: [MODIFY] `opc-da-client/src/com/worker/write.rs` — Worker Scalar Write SSO Delegation**
  - **Pre:** `handle_write` constructs `&WriteBatch::Single(tag_id.to_string(), value.clone())` allocating on heap.
  - **Target:** [`opc-da-client/src/com/worker/write.rs:155-163`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/write.rs#L155-L163)
  - **Action:** Replace lines 155-163 with zero-allocation stack SSO delegation:
    ```rust
    let batch = WriteBatch::from_str_lenient(tag_id, value.clone());
    let mut results = handle_write_batch(server_id, &batch, opc_server)?;
    results
        .pop()
        .ok_or_else(|| OpcError::Internal("No write result returned".into()))
    ```
  - **Post:** `CHECK (cargo check -p opc-da-client expects: exit 0)`

- [ ] **Step 7: [MODIFY] `opc-da-client/src/provider.rs` — Mock Test Call Site Migration**
  - **Pre:** Lines 517 and 566 construct private variant `crate::types::WriteBatch::Owned(...)`.
  - **Target:** [`opc-da-client/src/provider.rs:428-432, 514-523, 563-572`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/provider.rs#L514-L523)
  - **Action:**
    - Import `use crate::types::IntoWriteBatch;` in `mod tests`.
    - In `test_provider_default_write_tag_batch_success` (line 517), replace `crate::types::WriteBatch::Owned(vec![...])` with `vec![...].into_write_batch()`.
    - In `test_provider_default_write_tag_batch_partial_failure` (line 566), replace `crate::types::WriteBatch::Owned(vec![...])` with `vec![...].into_write_batch()`.
  - **Post:** `TEST (cargo test -p opc-da-client --lib provider::tests expects: exit 0)`

- [ ] **Step 8: [MODIFY] `opc-da-client/spec.md` — Specification Modernization & Pruning**
  - **Pre:** Line 188 references deleted `OpcDaClient::write_batch`; rows 645-646 contain deprecated methods.
  - **Target:** [`opc-da-client/spec.md:188, 645-646`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/spec.md#L188)
  - **Action:**
    - Update line 188 to reference canonical `OpcDaClient::write_tags`.
    - Prune deprecated method rows 645-646 (`write` and `write_batch`) under clean slate governance.
    - Update lines 155-183 to document opaque `struct WriteBatch` with 31-byte stack SSO and sequence `PartialEq`.
  - **Post:** `spec.md` strictly aligned with implementation and clean-slate rules.

- [ ] **🔒 CHECKPOINT 2: Workspace Integration Gate**
  - Run `cargo check -p opc-da-client --all-targets`
  - Run `cargo test -p opc-da-client --test batch_write_test`
  - Invariants confirmed: all 8 integration tests pass unchanged; 0 compiler errors.

- [ ] **Step 9: [VERIFY] Workspace Verification Pipeline**
  - **Pre:** Steps 1–8 complete.
  - **Action:**
    - Execute `cargo fmt --check`
    - Execute `cargo clippy -p opc-da-client --all-targets -- -D warnings`
    - Execute `cargo test -p opc-da-client`
    - Execute `powershell -File scripts/verify.ps1`
  - **Post:** `TEST (powershell -File scripts/verify.ps1 expects: exit 0)`

---

## 6. Security & Defensive Invariant Matrix

| Vulnerability / Flaw | Target Subsystem | Root Cause | Defense & Mitigation Strategy |
|:---|:---|:---|:---|
| **CWE-20 / CWE-787** (UTF-8 Codepoint Tearing) | `types/write_batch.rs` | Naive string slicing at byte 31 across multibyte codepoints | `from_str_lenient` checks `tag.len() <= 31`. If $> 31$, it overflows cleanly to `OwnedSingle` without slicing or codepoint tearing. `inline_as_str` validates bytes with `core::str::from_utf8` and recovers via `valid_up_to()`. |
| **CWE-626** (Interior Null Byte Truncation) | `com/worker/write.rs` | BSTR truncation in Win32 COM server APIs | `WriteBatch` preserves raw bytes with `\0` verbatim across all variants; `partition_write_inputs` quarantines tags containing `\0` into `WriteResult::failure` prior to COM marshaling. |
| **CWE-682** (Length Calculation Hazard) | `types/write_batch.rs` | Reading string byte length `*len as usize` in `InlineSingle` | All scalar arms (`StaticSingle`, `InlineSingle`, `OwnedSingle`) return constant `1`; preserves `ExactSizeIterator` monotonicity and prevents buffer miscalculations. |
| **CWE-400** (Unbounded Resource Exhaustion) | `types/write_batch.rs` | Redundant vector reallocation during ingestion | Move-only vector buffer transfer in `From<Vec<(String, OpcValue)>>` consumes existing capacity directly without element clones or buffer reallocations. |

---

## 7. Performance & Allocation Budgets

```
========================================================================================
WriteBatchRepr::InlineSingle Layout (Total: 72 Bytes, Align: 8)
----------------------------------------------------------------------------------------
Offset  0..7   [8 Bytes]: Enum Discriminant (1 Byte) + Trailing Alignment Padding (7 Bytes)
Offset  8..38 [31 Bytes]: Inline Tag Buffer (buf: [u8; 31])
Offset 39     [ 1 Byte ]: Tag Byte Length (len: u8)
Offset 40..71 [32 Bytes]: OpcValue Payload (align 8, exactly 0 internal padding bytes!)
========================================================================================
L1 Cache Line 0 (Bytes  0..63): [Discriminant (8B) | buf (31B) | len (1B) | OpcValue (24B)]
L1 Cache Line 1 (Bytes 64..71): [OpcValue tail (8B)]
========================================================================================
```

- **Memory Layout:** Exactly 72 bytes on `x86_64` (8B leaner than `TagBatch`'s 80 bytes). Sizing accommodates one 32-byte `OpcValue` without inflating the enum size.
- **Zero-Allocation Hot Paths:**
  - `WriteBatch::from_str_lenient("Tag", val)`: 0 heap allocations for tags $\le 31$ bytes.
  - `WriteBatch::from_static("StaticTag", val)` & `WriteBatch::from(("StaticTag", val))`: 0 heap allocations for compile-time string literals.
  - `WriteBatch::into_shareable()`: Retains `StaticSingle` and `InlineSingle` on the stack (0 heap allocations, 0 atomic refcount increments).
  - COM worker `handle_write`: Eliminates 3,000 throwaway heap string allocations per minute in 50 Hz background control loops.

---

## 8. Review History & Verdict

| Review Cycle | Reviewer | Verdict | Adjustments Made |
|:---:|:---:|:---:|:---|
| Cycle 1 | `plan-reviewer` | ⚠️ Revisions Recommended | Identified 6 items: generic collection impls, `Clone` bound on borrowed slices, `from_static` constructor, clippy truncation suppression, legacy test refactoring, and Tier M IPR scaffolding. |
| Cycle 2 | `plan-reviewer` | ⚠️ Revisions Recommended | Identified 4 items: added `V: Sync` bound on borrowed slice/array `IntoWriteBatch` impls, added missing contract rows (`into_vec`, `default`, iterators) to Section 4, added 'Read Before Starting' checklist in Builder Context, and standardized GEO Post annotations. |
| Cycle 3 | `plan-reviewer` | ✅ Approved | Verified all 4 items resolved: `V: Clone + Into<OpcValue> + Send + Sync` enforced, interface contracts complete, Builder Context checklist complete, and Post assertions standardized. Fully approved for Builder implementation. |

---

## 9. 🛡️ Plan Review Validation Result

> **Validation Status: ✅ Approved (Cycle 3 of 3)**  
> - **Reviewer:** Senior Implementation Plan Design Reviewer (`plan_reviewer`)  
> - **Date:** 2026-09-18  
> - **Verdict:** **Approved**  
> - **Sign-off Summary:** The implementation plan satisfies all architecture, memory alignment (72 bytes on x86_64, 0 internal padding), trait coherence (`E0119` and `E0277` avoidance), security (CWE-20/787, CWE-626, CWE-682, CWE-400), and testing requirements. It is certified ready for the Builder execution phase (`/build`).
