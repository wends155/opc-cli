# Cycle 2 Qualitative Architecture & Code Quality Review: Sub-Block H3c

> **Document Status:** Active Engineering Review Report & Planning Foundation  
> **Workspace:** `opc-cli`  
> **Target Crate:** `opc-da-client` (v0.2.0 $\rightarrow$ v0.3.0 Release Candidate)  
> **Evaluation Date:** 2026-09-17  
> **Sub-Block Focus:** Sub-Block H3c: Batch Ergonomics & Public Conversions  
> **Reference Documents:** [`refactor/cycle2_blockH3_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockH3_review.md), [`refactor/cycle2_blockH_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockH_review.md)  
> **Review Scope:** Public batch conversion traits, Small String Optimization (SSO), non-consuming borrows, collection predicates, and streaming doctests (`src/types/batch.rs`, `src/types/write_batch.rs`, `src/types/value.rs`, `src/types/collection.rs`, `src/client/subscription.rs`)  
> **Review Pipeline:** Decomposed Multi-Lens Audit across Logic, Design, Performance, Security, and API lenses  
> **User Interview Alignment & Consensuses:**
> 1. **Decomposition Mapping:** Approved Sub-Block H3c isolating Batch Ergonomics & Public Conversions (Findings #6, #7, #8, #9, #10a).
> 2. **Execution Ordering:** Strictly sequential — Sub-Block H3c executes immediately after Sub-Block H3b.
> 3. **Backwards Compatibility:** Strictly additive and 100% backwards compatible with all existing call sites.
> 4. **Zero-Allocation Stack SSO:** Implement `IntoTags for &str` leveraging `TagBatch::from_str_lenient` to enable 31-byte stack SSO for all dynamic strings.
> 5. **Symmetric Borrows:** Support non-consuming batch passing (`&TagBatch` and `&WriteBatch`), plus `From<&OpcValue> for OpcValue`.
> 6. **TDD Matrix:** Dedicated Red-Green test matrix included to guide implementation planning.

---

## 1. Executive Summary

Sub-Block H3c focuses on **unlocking zero-allocation Small String Optimization (SSO), expanding conversion trait lifetimes, and establishing complete ergonomic symmetry between read and write pipelines**.

`TagBatch` contains a high-performance internal representation engine featuring 31-byte stack-allocated inline storage (`InlineSingle([u8; 31], u8)`), fixed-size array inline storage (`StaticSmall` up to 4 elements), and zero-cost atomic sharing (`StaticArc`). However, the public conversion trait `IntoTags` was overly constrained to `'static` lifetimes. This created major ergonomic deficits:
1. **Blocked Dynamic Strings & Borrowed Slices (Finding #6 - 🟡 Minor):** Callers passing dynamic strings (`&'a str` such as formatted tag names) or dynamically borrowed slices (`&'a [&'a str]`, `&'a [String]`) were rejected by the compiler, forcing manual `Vec<String>` heap allocations and defeating the 31-byte stack SSO.
2. **Asymmetric Non-Consuming Borrows (Finding #7 - 🟡 Minor):** Neither `IntoTags` nor `IntoWriteBatch` implemented conversions for borrowed references to batch types (`&TagBatch` and `&WriteBatch`), forcing callers to write explicit `.clone()` calls to reuse batches. Furthermore, `&[(&str, &OpcValue)]` failed conversion because `From<&OpcValue> for OpcValue` was missing.
3. **Single Write Heap Copies in `into_shareable` (Finding #8 - 🟡 Minor):** `WriteBatch::into_shareable` left `Single(tag, val)` unchanged, so cloning a shareable single write batch performed deep string clones instead of $O(1)$ refcount increments.
4. **Collection Query & Doc Gaps (Findings #9, #10a - ⚪ Nitpick):** `TagValues` lacked an idiomatic `contains(&self, tag: &str) -> bool` query predicate, and `OpcDaClient::subscribe` lacked `# Panics` and `# Examples` doctests.

Sub-Block H3c is purely additive, zero-breaking, and brings the public API to 100% ergonomic completion.

---

## 2. Scoped Files & Target Symbols

| File | Target Symbols | Role in Sub-Block H3c |
|:---|:---|:---|
| [`opc-da-client/src/types/batch.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/batch.rs) | `trait IntoTags`<br>`impl IntoTags for &str`<br>`impl IntoTags for &[&str]`<br>`impl IntoTags for &TagBatch`<br>`impl From<&TagBatch> for TagBatch` | Generalize lifetime bounds to all lifetimes `'a`, unlock 31-byte stack SSO via `from_str_lenient`, replace `&'static [&'static str]`, and implement non-consuming borrow conversions (`&[String]` pre-existing). |
| [`opc-da-client/src/types/write_batch.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs) | `trait IntoWriteBatch`<br>`impl From<&WriteBatch> for WriteBatch`<br>`WriteBatch::into_shareable` | Implement `From<&WriteBatch>` (which auto-satisfies `IntoWriteBatch` via blanket impl without E0119 collision) and wrap `Single` in `Arc` inside `into_shareable`. |
| [`opc-da-client/src/types/value.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/value.rs) | `impl From<&OpcValue> for OpcValue` | Implement value reference conversion, enabling `&[(&str, &OpcValue)]` slice conversions. |
| [`opc-da-client/src/types/collection.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collection.rs) | `TagValues::contains` | Add direct case-insensitive membership query predicate. |
| [`opc-da-client/src/client/subscription.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/client/subscription.rs) | `OpcDaClient::subscribe` | Add complete rustdoc comments with `# Panics` and `# Examples` doctests (zero forbidden macros). |

---

## 3. High-Level Objectives & Goals

| ID | Objective | Measurable Success Criteria |
|:---:|:---|:---|
| **O1** | Zero-Allocation Stack SSO for Dynamic Strings | `format!("Tag.{}", i).as_str().into_tag_batch()` compiles and constructs `TagBatchRepr::InlineSingle` with zero heap allocations for tags $\le 31$ bytes. |
| **O2** | Generic Borrowed Slice Conversions | `let slice: &[&str] = &["T1", "T2"]; client.read_tags(slice)` compiles cleanly without requiring `to_vec()` or `to_string()`. |
| **O3** | Owned String Slice Conversions (Pre-Existing) | `let tags: Vec<String> = ...; client.read_tags(&tags[..])` compiles cleanly (pre-existing at `src/types/batch.rs:402`). |
| **O4** | Symmetric Non-Consuming Batch Borrows | `client.read_tags(&tag_batch)` and `client.write_tags(&write_batch)` compile cleanly without explicit `.clone()`. |
| **O5** | Value Reference Conversion in Write Batches | Passing `&[("Tag", &opc_value)]` converts cleanly into `WriteBatch`. |
| **O6** | $O(1)$ Single Write Batch Sharing | Calling `.clone()` on `WriteBatch::Single(...).into_shareable()` performs an $O(1)$ atomic refcount increment with zero string cloning. |
| **O7** | Idiomatic Collection Query | `values.contains("tag")` returns `true` case-insensitively. |
| **O8** | 100% Public Documentation & Doctest Suite | `cargo test --doc` compiles and runs doctests on `OpcDaClient::subscribe` (Gate 7 compliant, zero `println!`) and `TagValues::contains`. |

---

## 4. Multi-Lens Qualitative Assessment Matrix

| Lens | Severity | Finding Anchor | Summary & Lens-Specific Impact |
|:---|:---:|:---|:---|
| **API** | 🟠 Major | [`src/types/batch.rs:343`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/batch.rs#L343) | **Lifetime Over-Constraint:** Restricting `IntoTags` to `'static` prevented dynamic `&str` and borrowed slices `&[&str]` from converting, forcing callers into awkward manual heap allocation workarounds (`to_vec()`). |
| **Performance** | 🟠 Major | [`src/types/batch.rs:54, 343`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/batch.rs#L54) | **Unlocking 31-Byte Stack SSO:** Over 95% of industrial tag names are $\le 31$ bytes. Generalizing `IntoTags for &str` unlocks `from_str_lenient`, eliminating heap allocations and global allocator lock contention for dynamic single reads. |
| **Design** | 🟡 Minor | [`src/types/write_batch.rs:403`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs#L403) | **Batch Pipeline Symmetry:** `WriteBatch` previously accepted generic slices while `TagBatch` was restricted to `'static`. Providing matching slice conversions and non-consuming `&Batch` borrows establishes full symmetry across read and write APIs. |
| **Logic** | 🟡 Minor | [`src/types/value.rs:1`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/value.rs#L1) | **Value Conversion Completeness:** Implementing `From<&OpcValue> for OpcValue` resolves missing trait resolution branches when converting slices of tuple references `&[(&str, &OpcValue)]`. |
| **Security** | ⚪ Nitpick | [`src/types/batch.rs:343`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/batch.rs#L343) | **Allocation Bounds:** Conversion from borrowed slices pre-allocates vector capacity via `Vec::with_capacity(self.len())`, preventing intermediate vector re-allocations and heap fragmentation. |

---

## 5. Technical Deliverables & Implementation Contracts

### 5.1 Generalized `IntoTags` Implementations ([`src/types/batch.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/batch.rs))
```rust
// Replaces `impl IntoTags for &'static str`
impl IntoTags for &str {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch::from_str_lenient(self)
    }
}

// Replaces `impl IntoTags for &'static [&'static str]`
impl IntoTags for &[&str] {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        if self.is_empty() {
            TagBatch::empty()
        } else if self.len() == 1 {
            TagBatch::from_str_lenient(self[0])
        } else {
            let mut vec = Vec::with_capacity(self.len());
            for &s in self {
                vec.push(s.to_string());
            }
            TagBatch {
                repr: TagBatchRepr::Owned(vec),
            }
        }
    }
}

impl IntoTags for &TagBatch {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        self.clone()
    }
}

impl From<&TagBatch> for TagBatch {
    #[inline]
    fn from(batch: &TagBatch) -> Self {
        batch.clone()
    }
}
```

*Note 1: `impl IntoTags for &[String]` is already pre-existing at `src/types/batch.rs:402` and verified.*  
*Note 2: Existing array implementations (`[&'static str; N]` and `&'static [&'static str; N]`) are strictly retained to preserve zero-allocation `StaticSmall` and `StaticArc` storage for static array literals.*

### 5.2 Non-Consuming `WriteBatch` Conversions & $O(1)$ Sharing ([`src/types/write_batch.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/write_batch.rs))
```rust
// Implemented via `From<&WriteBatch>` to satisfy blanket `impl<T: Into<WriteBatch>> IntoWriteBatch for T`
// without triggering E0119 trait coherence conflicts.
impl From<&WriteBatch> for WriteBatch {
    #[inline]
    fn from(batch: &WriteBatch) -> Self {
        batch.clone()
    }
}

impl WriteBatch {
    #[must_use]
    pub fn into_shareable(self) -> Self {
        match self {
            Self::Single(tag, val) => Self::Shared(Arc::from([(tag, val)])),
            Self::Shared(slice) => Self::Shared(slice),
            Self::Owned(vec) => Self::Shared(Arc::from(vec.into_boxed_slice())),
        }
    }
}
```

### 5.3 Value Reference Conversion ([`src/types/value.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/value.rs))
```rust
impl From<&OpcValue> for OpcValue {
    #[inline]
    fn from(val: &OpcValue) -> Self {
        val.clone()
    }
}
```

### 5.4 Collection Query Method ([`src/types/collection.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/collection.rs))
```rust
impl TagValues {
    /// Returns `true` if the collection contains a tag with the specified identifier (case-insensitive).
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcQuality, OpcValue, TagValue, TagValues};
    ///
    /// let values = TagValues::new(vec![TagValue::new("Sensor1", Some(OpcValue::Int(10)), OpcQuality::GOOD, None)]);
    /// assert!(values.contains("sensor1"));
    /// assert!(!values.contains("Sensor2"));
    /// ```
    #[must_use]
    pub fn contains(&self, tag: &str) -> bool {
        self.get(tag).is_some()
    }
}
```

### 5.5 Complete Documentation & Doctests on `subscribe` ([`src/client/subscription.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/client/subscription.rs))
```rust
/// Subscribes to a stream of tag value updates polled at the specified interval.
///
/// Spawns a background Tokio task that periodically polls the configured tags on the bound
/// server and streams updates through a Tokio [`mpsc::Receiver`]. Dropping the receiver
/// automatically terminates the background polling loop.
///
/// # Panics
///
/// This function does not panic.
///
/// # Examples
///
/// ```rust,no_run
/// # use opc_da_client::{Bound, DefaultOpcDaClient};
/// # use std::time::Duration;
/// # async fn run(client: &DefaultOpcDaClient<Bound>) {
/// let mut rx = client.subscribe(["Sensor.1", "Sensor.2"], Duration::from_millis(500));
/// while let Some(values) = rx.recv().await {
///     let _ = values.len();
/// }
/// # }
/// ```
```

---

## 6. Blast Radius Table

| Symbol | File | Direct Callers | Indirect Callers | Cross-Crate Impact | Risk Level |
|:---|:---|:---:|:---:|:---:|:---:|
| `impl IntoTags for &str` (New) | `src/types/batch.rs` | 0 (New) | 12 | None (replaces `&'static str`, backward-compatible) | 🟢 Low |
| `impl IntoTags for &[&str]` (New) | `src/types/batch.rs` | 0 (New) | 24 | None (replaces `&'static [&'static str]`, backward-compatible) | 🟢 Low |
| `impl IntoTags for &[String]` (Pre-Existing) | `src/types/batch.rs` | 1 | 8 | None (pre-existing at L402, verified) | 🟢 Low |
| `impl IntoTags for &TagBatch` (New) | `src/types/batch.rs` | 0 (New) | 4 | None (additive) | 🟢 Low |
| `impl From<&TagBatch> for TagBatch` (New) | `src/types/batch.rs` | 0 (New) | 4 | None (additive) | 🟢 Low |
| `impl From<&WriteBatch> for WriteBatch` (New) | `src/types/write_batch.rs` | 0 (New) | 4 | None (additive, satisfies `IntoWriteBatch`) | 🟢 Low |
| `impl From<&OpcValue> for OpcValue` (New) | `src/types/value.rs` | 0 (New) | 6 | None (additive) | 🟢 Low |
| `WriteBatch::into_shareable` | `src/types/write_batch.rs` | 2 | 2 | None (optimizes single write clones to $O(1)$) | 🟢 Low |
| `TagValues::contains` (New) | `src/types/collection.rs` | 0 (New) | 0 | None (additive) | 🟢 Low |
| `OpcDaClient::subscribe` | `src/client/subscription.rs` | 1 | 0 | None (pure documentation update, Gate 7 compliant) | 🟢 Low |

---

## 7. Things to Watch Out For (Defensive Invariants)

1. **Type Inference on Array Literals:**
   In Rust, passing `["Tag1", "Tag2"]` matches `[&'static str; 2]`. It is essential to retain `impl<const N: usize> IntoTags for [&'static str; N]` alongside `impl IntoTags for &[&str]`. Omitting the array literal implementation would force Rust into type inference ambiguity between slice reference coercion and array value passing.
2. **Replacing Over-Constrained Slices to Prevent E0119:**
   `impl IntoTags for &[&str]` replaces `impl IntoTags for &'static [&'static str]`. Keeping both would trigger `E0119` (conflicting implementations of trait `IntoTags` for type `&'static [&'static str]`).
3. **Blanket Implementation Trait Coherence for `WriteBatch`:**
   `IntoWriteBatch` already provides a blanket implementation `impl<T: Into<WriteBatch>> IntoWriteBatch for T`. Attempting to write `impl IntoWriteBatch for &WriteBatch` triggers `E0119`. Instead, implement `From<&WriteBatch> for WriteBatch`, which automatically fulfills `T: Into<WriteBatch>` and `IntoWriteBatch`.
4. **Worker Thread `'static` Bound:**
   `TagBatch` itself must remain `'static` and `Send` because it is dispatched over Tokio channels to the background COM apartment worker thread. We must **NOT** introduce a lifetime parameter on `TagBatch` (e.g. `TagBatch<'a>`); all borrowed inputs are converted to owned storage or stack SSO upon `into_tag_batch()` construction.
5. **Forbidden Macro Gate 7 Compliance:**
   Doctests in library modules must never use `println!`, `dbg!`, `todo!`, or `unimplemented!`. Use `let _ = ...;` or testable assertions (`assert!`) to comply with Gate 7 in `scripts/verify.ps1`.
6. **Capacity Pre-Allocation:**
   Always pre-allocate vector capacity via `Vec::with_capacity(self.len())` when converting slices into `TagBatchRepr::Owned` or `WriteBatch::Owned`.

---

## 8. TDD Red-Green Verification Matrix

| Test Case | Scope | Red Baseline | Green Success Criteria |
|:---|:---|:---|:---|
| `test_into_tags_dynamic_str_stack_sso` | `src/types/batch.rs` | Rejects non-`'static` `&str` | `let s = format!("Tag.{}", 1); s.as_str().into_tag_batch()` succeeds with `InlineSingle` |
| `test_into_tags_borrowed_slice` | `src/types/batch.rs` | Rejects non-`'static` `&[&str]` | `let slice: &[&str] = &["A", "B"]; slice.into_tag_batch()` succeeds with `Owned` |
| `test_into_tags_borrowed_string_slice` | `src/types/batch.rs` | Pre-existing | `let vec = vec!["A".into()]; (&vec[..]).into_tag_batch()` succeeds with `Owned` (verified) |
| `test_into_tags_batch_ref` | `src/types/batch.rs` | Rejects `&TagBatch` | `let b = TagBatch::empty(); (&b).into_tag_batch()` clones cleanly |
| `test_into_write_batch_ref` | `src/types/write_batch.rs` | Rejects `&WriteBatch` | `let b = WriteBatch::empty(); (&b).into_write_batch()` clones cleanly |
| `test_into_write_batch_borrowed_value_tuples` | `src/types/write_batch.rs` | Rejects `&OpcValue` in tuple slice | `let v = OpcValue::Int(1); [("Tag", &v)].as_slice().into_write_batch()` succeeds |
| `test_write_batch_into_shareable_single_o1` | `src/types/write_batch.rs` | `Single` cloned via deep copy | `WriteBatch::Single("Tag".into(), OpcValue::Int(1)).into_shareable()` wraps in `Shared` |
| `test_tag_values_contains_case_insensitive` | `src/types/collection.rs` | Method missing | `values.contains("device1.temp")` returns `true` when containing `"Device1.Temp"` |
| `test_subscription_doctest_compiles` | `src/client/subscription.rs` | Missing doctest | `cargo test --doc` runs subscription examples successfully |

---

📄 **Sub-Block Report:** [`refactor/cycle2_blockh3c_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockh3c_review.md)
