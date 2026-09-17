# Implementation Plan: Sub-Block H3c: Batch Ergonomics & Public Conversions

**Role:** Architect • **Date:** 2026-09-17 • **Tier:** M  
**Scope:** Batch Ergonomics (`IntoTags`, `IntoWriteBatch`, `From<&TagBatch>`, `From<&WriteBatch>`, `From<&OpcValue>`), Small String Optimization (SSO) Stack Expansion, Single Write Batch Sharing (`WriteBatch::into_shareable`), Collection Membership Query (`TagValues::contains`), and Subscription Doctests in `opc-da-client`

### Builder Context
Read before starting:
- `opc-da-client/src/types/batch.rs` (L50-70 `from_str_lenient`, L330-410 `IntoTags`, L428-478 `From`)
- `opc-da-client/src/types/write_batch.rs` (L80-95 `WriteBatch`, L180-195 `into_shareable`, L360-418 `From`, `IntoWriteBatch`)
- `opc-da-client/src/types/value.rs` (L15-40 `OpcValue`, L50-70 `as_int`)
- `opc-da-client/src/types/collection.rs` (L350-375 `TagValues`, L418-446 `get`, `get_value`)
- `opc-da-client/src/client/subscription.rs` (L10-48 `subscribe`)
- `refactor/cycle2_blockh3c_review.md` (Findings #6, #7, #8, #9, #10a)
- `.agents/rules/coding-standard.md` (governance core rules, zero forbidden macros, zero compiler warnings)

### Phase Context
- **Phase:** 3 of 3 (Cycle 2 Modernization: Block H — Phase 3: Sub-Block H3c Batch Ergonomics & Public Conversions)
- **Prior phase:** Sub-Block H3b delivered COM worker active group caching, response tag casing preservation, and batch write resource bounding.
- **Stubs for this phase:** None (pure production domain and conversion implementations).
- **Following phase:** Cycle 2 Modernization Complete (Block H finished).

### Problem Statement
In `refactor/cycle2_blockh3c_review.md` (and upstream `refactor/cycle2_blockH3_review.md`), five ergonomic and allocation shortcomings were identified in `opc-da-client`:
1. **Finding #6 (Minor - API / Perf)**: `IntoTags` conversion trait implementations in `src/types/batch.rs` were over-constrained to `'static` lifetimes (`impl IntoTags for &'static str`, `impl IntoTags for &'static [&'static str]`). This prevented callers passing dynamic strings (`&str` such as formatted tag IDs) or borrowed string slices (`&[&str]`) without heap allocations (`to_vec()`), defeating the pre-existing 31-byte stack SSO engine (`TagBatchRepr::InlineSingle`).
2. **Finding #7 (Minor - Design / API)**: Symmetrical non-consuming batch passing was missing: neither `&TagBatch` nor `&WriteBatch` had reference conversions, forcing callers to write `.clone()`. Slice tuple references `&[(&str, &OpcValue)]` failed due to missing `From<&OpcValue> for OpcValue`.
3. **Finding #8 (Minor - Perf / Design)**: `WriteBatch::into_shareable` left `Single(tag, val)` as an unshared owned pair, resulting in deep string copies on `.clone()` during subscription stream polling and retries.
4. **Finding #9 (Nitpick - API)**: `TagValues` lacked an idiomatic `contains(&self, tag: &str) -> bool` query predicate, forcing callers to check `get(tag).is_some()`.
5. **Finding #10a (Nitpick - Docs / Quality)**: `OpcDaClient::subscribe` lacked complete `# Panics` and `# Examples` doctests.

In accordance with user interview decisions:
- 1-element slices `&[s]` route to `TagBatch::from_str_lenient(s)` to leverage 31-byte stack SSO; $N=0$ routes to `empty()`; $N > 1$ allocates an `Owned` vector with pre-allocated capacity.
- `TagValues::contains(&self, tag: &str) -> bool` uses `self.get(tag).is_some()`, providing case-insensitive ASCII comparison matching OPC DA specifications.
- `From<&WriteBatch>`, `From<&TagBatch>`, and `From<&OpcValue>` are implemented in tandem for complete ergonomic symmetry across read and write pipelines.
- Trait coherence collision (`E0119`) on blanket `IntoWriteBatch` is strictly avoided by implementing `From<&WriteBatch> for WriteBatch`.
- `OpcDaClient::subscribe` doctests strictly conform to Gate 7 (zero `println!`, using root-exported `Bound` and `DefaultOpcDaClient`).

### Plan Objectives
| ID | Objective | Success Criteria | Steps |
|----|-----------|-----------------|-------|
| O1 | Zero-allocation stack SSO for dynamic tag names (Finding #6) | `format!("Tag.{}", i).as_str().into_tag_batch()` succeeds with `InlineSingle` (0 heap allocations) for $\le 31$ bytes; overflows gracefully to `OwnedSingle` for $> 31$ bytes | 1, 2 |
| O2 | Generic borrowed string slice conversions (Finding #6) | `let slice: &[&str] = &["T1", "T2"]; slice.into_tag_batch()` succeeds with `Owned` vector; 1-element slice uses stack SSO; `impl IntoTags for &[&str]` replaces `&'static [&'static str]` without `E0119` conflict | 3, 4 |
| O3 | Symmetric non-consuming batch borrow conversions (Finding #7) | `(&tag_batch).into_tag_batch()`, `TagBatch::from(&tag_batch)`, and `(&write_batch).into_write_batch()` clone cleanly without consuming original; `From<&WriteBatch> for WriteBatch` satisfies blanket `IntoWriteBatch` without `E0119` conflict | 5, 6, 9, 10 |
| O4 | Value reference conversions for borrowed write tuple slices (Finding #7) | `From<&OpcValue> for OpcValue` implemented; `[("Tag", &v)].as_slice().into_write_batch()` compiles and succeeds | 7, 8, 11 |
| O5 | $O(1)$ single write batch sharing across threads (Finding #8) | `WriteBatch::Single("Tag".into(), val).into_shareable()` wraps into `WriteBatch::Shared(Arc::from(vec![(tag, val)].into_boxed_slice()))`, turning `.clone()` into $O(1)$ atomic refcount increment | 12, 13 |
| O6 | Idiomatic case-insensitive collection query predicate (Finding #9) | `TagValues::contains(&self, tag: &str) -> bool` implemented delegating to `self.get(tag).is_some()`; case-insensitive matches verified | 14, 15 |
| O7 | Public subscription documentation and Gate 7 compliant doctests (Finding #10a) | `OpcDaClient::subscribe` rustdoc includes `# Panics` and `# Examples` doctest with zero `println!`; passes `cargo test --doc` | 16, 17 |
| O8 | Full workspace 8-gate quality pipeline green | `pwsh -File scripts/verify.ps1` exits 0 with zero warnings under `-D warnings` | 18 |

### Review History & Verdict
| Cycle | Reviewer | Model | Verdict | Adjustments Applied |
|---|---|---|---|---|
| 1 | `plan-reviewer` | `flash` | `⚠️ Revisions Recommended` | Applied 4 adjustments: (1) fixed `WriteBatch::into_shareable` single variant boxing type mismatch, (2) inverted Steps 16/17 TDD sequence for subscription doctests, (3) restored full rustdoc documentation in Step 15 Action, (4) added Deprecation Schedule exemption footnote under Blast Radius Table |
| 2 | `plan-reviewer` | `flash` | `✅ Approved` | No further adjustments required. All ergonomic contracts, type models, and quality gates verified. |
| 3 | 7-lens assessment (Plan Auditor, Improvement Analyst, API Design, Type System, Performance, Test Strategy, Security) | `flash` × 7 | `⚠️ Revisions Applied` | 17 improvements applied: (A1) E0119 prohibition in Negative Scope, (A2) 1+N allocation budget, (A3) TDD Red State compile-time note, (A4) Step 16→17 checkpoint move, (A5) Step 11 integration note, (A6) Deprecation Schedule heading, (A7) UTF-8 security fix, (A8) ASCII case-folding doc, (C1) into_shareable optimization note, (C2) idiomatic iterator, (C3) Performance Note rustdoc, (C4) representation divergence test, (C5) expanded Step 12 lifecycle test, (C6) empty tuple edge case, (C7) empty batch ref case, (C8) reorder consideration, (C9) enriched doctest. 4 proposals rejected: replacing From<&'static str> (perf regression), adding From<&str> (E0119), adding From<&[&str]> (E0119), out-of-scope From impls. |

### Negative Scope
**Out of Scope for Sub-Block H3c:**
- Do NOT modify the `TagBatch` struct definition to add lifetimes (e.g. `TagBatch<'a>`); `TagBatch` must remain `'static + Send` for cross-thread dispatch to the COM worker thread.
- Do NOT remove or modify static array implementations `[&'static str; N]` and `&'static [&'static str; N]` (retained to preserve zero-allocation `StaticSmall` and `StaticArc` storage).
- Do NOT implement `IntoWriteBatch for &WriteBatch` directly (must use `From<&WriteBatch>` to prevent `E0119` compiler conflict).
- Do NOT modify COM worker thread request handling or COM interfaces (already verified and audited in H3a/H3b).
- Do NOT modify `opc-cli` crate source files (verified 100% compatible).
- Do NOT implement `From<&str> for TagBatch`. In Rust, `impl From<&str>` covers all lifetimes including `'static`, triggering an `E0119` coherence conflict with the existing `impl From<&'static str> for TagBatch` (retained to preserve zero-allocation `StaticSingle` storage for arbitrary-length static string literals). Dynamic strings use `IntoTags for &str` → `from_str_lenient()` instead.

### Interface Contracts

#### 1. Generalized `IntoTags` Implementations (`src/types/batch.rs`)
```rust
// Replaces `impl IntoTags for &'static str` to unlock 31-byte stack SSO for all lifetimes
impl IntoTags for &str {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        TagBatch::from_str_lenient(self)
    }
}

// Replaces `impl IntoTags for &'static [&'static str]` to support arbitrary borrowed slices
impl IntoTags for &[&str] {
    #[inline]
    fn into_tag_batch(self) -> TagBatch {
        if self.is_empty() {
            TagBatch::empty()
        } else if self.len() == 1 {
            TagBatch::from_str_lenient(self[0])
        } else {
            TagBatch {
                repr: TagBatchRepr::Owned(self.iter().map(|&s| s.to_owned()).collect()),
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

#### 2. Non-Consuming `WriteBatch` Conversions & $O(1)$ Sharing (`src/types/write_batch.rs`)
```rust
// Implemented via `From<&WriteBatch>` to satisfy blanket `impl<T: Into<WriteBatch>> IntoWriteBatch for T`
// without triggering E0119 trait coherence conflicts.
//
// # Performance Note
//
// * If `batch` is `WriteBatch::Shared`, this is an O(1) reference-count increment (zero allocations).
// * If `batch` is `WriteBatch::Owned`, this performs an O(N) deep copy of all tag strings and values.
// * If `batch` is `WriteBatch::Single`, this clones the single tag String and OpcValue.
//
// For repeated operations in loops, call `.into_shareable()` once to ensure subsequent
// reference conversions are O(1).
impl From<&WriteBatch> for WriteBatch {
    #[inline]
    fn from(batch: &WriteBatch) -> Self {
        batch.clone()
    }
}

impl WriteBatch {
    /// Converts this batch into an efficiently cloneable representation,
    /// ensuring repeated clones avoid heap re-allocations via `Arc` sharing.
    #[must_use]
    pub fn into_shareable(self) -> Self {
        match self {
            Self::Single(tag, val) => Self::Shared(std::sync::Arc::from(vec![(tag, val)].into_boxed_slice())),
            Self::Shared(slice) => Self::Shared(slice),
            Self::Owned(vec) => Self::Shared(std::sync::Arc::from(vec.into_boxed_slice())),
        }
    }
}
```

#### 3. Value Reference Conversion (`src/types/value.rs`)
```rust
impl From<&OpcValue> for OpcValue {
    #[inline]
    fn from(val: &OpcValue) -> Self {
        val.clone()
    }
}
```

#### 4. Collection Membership Query Method (`src/types/collection.rs`)
```rust
impl TagValues {
    /// Returns `true` if the collection contains a tag with the specified identifier (case-insensitive).
    ///
    /// Comparison uses ASCII case folding (`A-Z` / `a-z`), matching OPC DA specifications
    /// and the behavior of [`TagValues::get`]. Non-ASCII characters require exact casing.
    ///
    /// # Arguments
    ///
    /// * `tag` - Tag identifier string to look up.
    ///
    /// # Returns
    ///
    /// `true` if a tag with the specified identifier is present in this collection; `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::{OpcQuality, OpcValue, TagValue, TagValues};
    ///
    /// let values = TagValues::new(vec![TagValue::new(
    ///     "Channel1.Device1.Sensor1",
    ///     Some(OpcValue::Int(10)),
    ///     OpcQuality::GOOD,
    ///     None,
    /// )]);
    /// assert!(values.contains("channel1.device1.sensor1"));
    /// assert!(!values.contains("Sensor2"));
    /// ```
    #[inline]
    #[must_use]
    pub fn contains(&self, tag: &str) -> bool {
        self.get(tag).is_some()
    }
}
```

#### 5. Public Subscription Documentation & Doctest (`src/client/subscription.rs`)
```rust
impl<C: ServerBackend + 'static> OpcDaClient<C, Bound> {
    /// Subscribes to a stream of tag value updates polled at the specified interval.
    ///
    /// Spawns a background Tokio task that periodically polls the configured tags on the bound
    /// server and streams updates through a Tokio [`mpsc::Receiver`]. Dropping the receiver
    /// automatically terminates the background polling loop.
    ///
    /// # Arguments
    ///
    /// * `tags` - Tag batch or convertible source to poll. Accepts any type implementing [`IntoTags`]
    ///   (e.g., `&str`, `[&str; N]`, `&[&str]`, `&TagBatch`, or `Vec<String>`).
    /// * `interval` - Polling interval duration. Clamped to a minimum of 10 milliseconds to prevent
    ///   accidental worker starvation.
    ///
    /// # Returns
    ///
    /// A Tokio [`Receiver<TagValues>`] streaming polled tag updates on each tick.
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
    /// if let Some(values) = rx.recv().await {
    ///     if let Some(value) = values.get_value("Sensor.1") {
    ///         let _ = value;
    ///     }
    /// }
    /// // Dropping `rx` shuts down the background polling loop.
    /// drop(rx);
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self, tags))]
    pub fn subscribe(&self, tags: impl IntoTags, interval: Duration) -> Receiver<TagValues> {
        // ... implementation unchanged ...
    }
}
```

### Blast Radius Table
| Symbol | File | Direct Callers | Indirect Deps | Test-Only | Cross-Package? |
|:---|:---|:---:|:---:|:---:|:---:|
| `impl IntoTags for &str` | `src/types/batch.rs` | 6 | 12 | No | Yes (replaces `&'static str`, backward-compatible) |
| `impl IntoTags for &[&str]` | `src/types/batch.rs` | 10 | 24 | No | Yes (replaces `&'static [&'static str]`, backward-compatible) |
| `impl IntoTags for &[String]` | `src/types/batch.rs` | 1 | 8 | No | Yes (Pre-existing at L402, verified) |
| `impl IntoTags for &TagBatch` | `src/types/batch.rs` | 0 (New) | 4 | No | Yes |
| `impl From<&TagBatch> for TagBatch` | `src/types/batch.rs` | 0 (New) | 4 | No | Yes |
| `impl From<&WriteBatch> for WriteBatch` | `src/types/write_batch.rs` | 0 (New) | 4 | No | Yes (satisfies blanket `IntoWriteBatch`) |
| `impl From<&OpcValue> for OpcValue` | `src/types/value.rs` | 0 (New) | 6 | No | Yes |
| `WriteBatch::into_shareable` | `src/types/write_batch.rs` | 1 | 2 | No | Yes |
| `TagValues::contains` | `src/types/collection.rs` | 0 (New) | 0 | No | Yes |
| `OpcDaClient::subscribe` | `src/client/subscription.rs` | 9 | 4 | No | Yes (documentation update) |

### Deprecation Schedule

> [!NOTE]
> **Exemption:** While `Cross-Package?` is marked `Yes` because `opc-da-client` public APIs are consumed across the workspace by `opc-cli`, all changes are 100% additive lifetime widenings (`&'static str` -> `&str`, `&'static [&'static str]` -> `&[&str]`) and new reference conversions. No public symbols, methods, or variants are deprecated or removed; therefore, an active deprecation schedule is not required under `ipr.md § Deprecation Schedule`.

### Security Constraints
- **Multibyte UTF-8 Preservation in SSO:** `TagBatch::from_str_lenient` branches by byte length: strings $\le 31$ bytes are copied in full (byte-for-byte exact replica of valid UTF-8 input), while strings $> 31$ bytes fall back to `OwnedSingle(s.to_string())`. The `valid_up_to()` path in `inline_as_str` is an unreachable defensive fallback under safe Rust memory guarantees.
- **Graceful Fallback:** Dynamic strings $> 31$ bytes fall back cleanly to `OwnedSingle(s.to_string())` without failing or truncating tag names.
- **Resource Bounding:** Slice conversions pre-allocate vector capacity via `Vec::with_capacity(self.len())` without unbounded reallocation cycles.
- **Case Normalization Safety:** `TagValues::contains` delegates strictly to `self.get(tag).is_some()`, utilizing `eq_ignore_ascii_case` over linear memory to prevent homoglyph collisions and TOCTOU divergence.

### Performance Constraints
- **Zero-Allocation Stack SSO:** For dynamic single-tag reads $\le 31$ bytes, `into_tag_batch()` incurs exactly 0 heap allocations (`TagBatchRepr::InlineSingle`).
- **Static Storage Retention:** Compile-time static array literals `[&'static str; N]` retain zero-allocation `StaticSmall` ($N \le 4$, 0 heap bytes) and single-slice `StaticArc` ($N > 4$, 0 string copies).
- **$O(1)$ Single Write Batch Sharing:** Upgrading `WriteBatch::Single` in `into_shareable` turns all subsequent clones across async channels from deep string copies into $O(1)$ atomic refcount increments.
- **Borrowed Slice Lifetime Erasure Budget:** Converting borrowed dynamic slices `&[&str]` with $N > 1$ incurs $1$ `Vec` allocation + $N$ heap `String` allocations into `TagBatchRepr::Owned`. This is unavoidable because `TagBatch` must be `'static + Send` for cross-thread dispatch. Callers with compile-time static strings should prefer fixed-size array literals `[&'static str; N]` to utilize zero-allocation `StaticSmall` ($N \le 4$) or single-allocation `StaticArc` ($N > 4$).

### Test Plan (TDD Red-Green)

> [!NOTE]
> **TDD Red State in Rust:** Steps 1, 3, 5, 7, 9, and 14 test for traits or methods that do not yet exist in the codebase prior to their corresponding `[MODIFY]` steps. In Rust, type-system and API additions exhibit **compile-time rejection** (e.g., `E0277: the trait IntoTags is not implemented for &str` or `E0599: no method named contains`) as the canonical RED baseline. The Builder should verify compile rejection at the `[TEST]` step (`cargo check` fails) before proceeding to the `[MODIFY]` step. Step 12 is an exception — it compiles but panics at runtime (true runtime RED).
1. **SSO Dynamic String Test (`src/types/batch.rs`):** Verify `format!("Tag.{}", i).as_str().into_tag_batch()` constructs `InlineSingle` for $\le 31$ bytes, `OwnedSingle` for $> 31$ bytes, and handles multibyte UTF-8.
2. **Borrowed Slice Test (`src/types/batch.rs`):** Verify non-static `&[&str]` converts to empty, `InlineSingle` (len 1), and `Owned` (len > 1) with capacity pre-allocation.
3. **Batch Ref Test (`src/types/batch.rs`):** Verify `(&batch).into_tag_batch()` and `TagBatch::from(&batch)` clone cleanly without consuming original.
4. **Value Ref Test (`src/types/value.rs`):** Verify `From<&OpcValue> for OpcValue` clones all variants cleanly.
5. **Write Batch Ref Test (`src/types/write_batch.rs`):** Verify `(&batch).into_write_batch()` satisfies blanket `IntoWriteBatch` for all variants.
6. **Write Batch Tuple Slice Test (`src/types/write_batch.rs`):** Verify `&[(&str, &OpcValue)]` converts cleanly into `WriteBatch`.
7. **Write Batch Shareable Single Test (`src/types/write_batch.rs`):** Verify `WriteBatch::Single.into_shareable()` wraps into `Shared` and subsequent clones increment `Arc` refcount with $O(1)$ overhead.
8. **Collection Contains Test (`src/types/collection.rs`):** Verify `TagValues::contains` matches case-insensitively.
9. **Subscription Doctest Test (`src/client/subscription.rs`):** Verify `cargo test --doc` executes successfully without forbidden macros.

### Global Execution Order

Step 1: [TEST] `opc-da-client/src/types/batch.rs` — [+] `test_into_tags_dynamic_str_stack_sso` (Finding #6)
- Pre: ALL
- Target: `src/types/batch.rs:798+`
- Action:
  Add unit test verifying `format!("Sensor.{}", 42).as_str().into_tag_batch()` constructs `InlineSingle` (0 heap bytes), exact 31-byte boundary, 32-byte fallback to `OwnedSingle`, and multibyte UTF-8 safety:
  ```rust
  #[test]
  fn test_into_tags_dynamic_str_stack_sso() {
      let dynamic_tag = format!("Sensor.{}", 42);
      let batch = dynamic_tag.as_str().into_tag_batch();
      assert_eq!(batch.len(), 1);
      assert!(!batch.is_empty());
      assert!(matches!(batch.repr, TagBatchRepr::InlineSingle(_, 9)));
      assert_eq!(batch.iter_str().next(), Some("Sensor.42"));
      assert_eq!(batch.into_vec(), vec!["Sensor.42".to_string()]);

      let dynamic_31 = format!("{:0<31}", "Tag");
      assert_eq!(dynamic_31.len(), 31);
      let batch_31 = dynamic_31.as_str().into_tag_batch();
      assert!(matches!(batch_31.repr, TagBatchRepr::InlineSingle(_, 31)));

      let dynamic_32 = format!("{:0<32}", "Tag");
      assert_eq!(dynamic_32.len(), 32);
      let batch_32 = dynamic_32.as_str().into_tag_batch();
      assert!(matches!(batch_32.repr, TagBatchRepr::OwnedSingle(_)));

      let dynamic_utf8 = format!("圧力_{}", 100);
      let batch_utf8 = dynamic_utf8.as_str().into_tag_batch();
      assert!(matches!(batch_utf8.repr, TagBatchRepr::InlineSingle(_, _)));
  }
  ```
- Post: RED(`test_into_tags_dynamic_str_stack_sso`)

Step 2: [MODIFY] `opc-da-client/src/types/batch.rs` — [~] `impl IntoTags for &str` replaces `&'static str` (Finding #6)
- Pre: RED(`test_into_tags_dynamic_str_stack_sso`)
- Target: `src/types/batch.rs:384-391`
- Action:
  Replace `impl IntoTags for &'static str` with:
  ```rust
  impl IntoTags for &str {
      #[inline]
      fn into_tag_batch(self) -> TagBatch {
          TagBatch::from_str_lenient(self)
      }
  }
  ```
- Post: GREEN(`test_into_tags_dynamic_str_stack_sso`)
  **Builder:** After confirming GREEN, also add the following companion test to verify representation divergence equivalence between `IntoTags for &str` (→ `InlineSingle`) and `From<&'static str>` (→ `StaticSingle`):
  ```rust
  #[test]
  fn test_tag_batch_representation_divergence_equivalence() {
      // IntoTags for &str → from_str_lenient → InlineSingle
      let b_inline = "Sensor.1".into_tag_batch();
      // From<&'static str> → StaticSingle
      let b_static = TagBatch::from("Sensor.1");

      // Different internal representations
      assert!(matches!(b_inline.repr, TagBatchRepr::InlineSingle(_, 8)));
      assert!(matches!(b_static.repr, TagBatchRepr::StaticSingle("Sensor.1")));

      // Logically equivalent across public surface
      assert_eq!(b_inline, b_static);
      assert_eq!(b_inline.len(), b_static.len());
      assert_eq!(b_inline.is_empty(), b_static.is_empty());
      assert_eq!(
          b_inline.iter_str().collect::<Vec<_>>(),
          b_static.iter_str().collect::<Vec<_>>()
      );
      assert_eq!(b_inline.into_vec(), b_static.into_vec());
  }
  ```

Step 3: [TEST] `opc-da-client/src/types/batch.rs` — [+] `test_into_tags_borrowed_slice` (Finding #6)
- Pre: GREEN(`test_into_tags_dynamic_str_stack_sso`)
- Target: `src/types/batch.rs:820+`
- Action:
  Add unit test verifying non-static `&[&str]` slice conversions:
  ```rust
  #[test]
  fn test_into_tags_borrowed_slice() {
      let empty_slice: &[&str] = &[];
      let empty_batch = empty_slice.into_tag_batch();
      assert_eq!(empty_batch.len(), 0);
      assert!(empty_batch.is_empty());

      let d1 = format!("Temp.{}", 1);
      let single_slice: &[&str] = &[d1.as_str()];
      let single_batch = single_slice.into_tag_batch();
      assert_eq!(single_batch.len(), 1);
      assert!(matches!(single_batch.repr, TagBatchRepr::InlineSingle(_, _)));
      assert_eq!(single_batch.iter_str().next(), Some("Temp.1"));

      let d2 = format!("Temp.{}", 2);
      let d3 = format!("Temp.{}", 3);
      let multi_slice: &[&str] = &[d1.as_str(), d2.as_str(), d3.as_str()];
      let multi_batch = multi_slice.into_tag_batch();
      assert_eq!(multi_batch.len(), 3);
      assert!(matches!(multi_batch.repr, TagBatchRepr::Owned(_)));
      assert_eq!(
          multi_batch.iter_str().collect::<Vec<_>>(),
          vec!["Temp.1", "Temp.2", "Temp.3"]
      );

      let batch_survives = {
          let local_a = format!("Local.{}", "A");
          let local_b = format!("Local.{}", "B");
          let local_slice: &[&str] = &[local_a.as_str(), local_b.as_str()];
          local_slice.into_tag_batch()
      };
      assert_eq!(batch_survives.len(), 2);
  }
  ```
- Post: RED(`test_into_tags_borrowed_slice`)

Step 4: [MODIFY] `opc-da-client/src/types/batch.rs` — [~] `impl IntoTags for &[&str]` replaces `&'static [&'static str]` (Finding #6)
- Pre: RED(`test_into_tags_borrowed_slice`)
- Target: `src/types/batch.rs:343-348`
- Action:
  Replace `impl IntoTags for &'static [&'static str]` with:
  ```rust
  impl IntoTags for &[&str] {
      #[inline]
      fn into_tag_batch(self) -> TagBatch {
          if self.is_empty() {
              TagBatch::empty()
          } else if self.len() == 1 {
              TagBatch::from_str_lenient(self[0])
          } else {
              TagBatch {
                  repr: TagBatchRepr::Owned(self.iter().map(|&s| s.to_owned()).collect()),
              }
          }
      }
  }
  ```
- Post: GREEN(`test_into_tags_borrowed_slice`)

Step 5: [TEST] `opc-da-client/src/types/batch.rs` — [+] `test_into_tags_batch_ref` (Finding #7)
- Pre: GREEN(`test_into_tags_borrowed_slice`)
- Target: `src/types/batch.rs:860+`
- Action:
  Add unit test verifying non-consuming `&TagBatch` conversions:
  ```rust
  #[test]
  fn test_into_tags_batch_ref() {
      let original = ["Sensor.A", "Sensor.B"].into_tag_batch();
      let cloned_via_into_tags = (&original).into_tag_batch();
      assert_eq!(cloned_via_into_tags, original);
      assert_eq!(cloned_via_into_tags.len(), 2);

      let cloned_via_from: TagBatch = (&original).into();
      assert_eq!(cloned_via_from, original);

      assert_eq!(original.len(), 2);
      assert_eq!(
          original.iter_str().collect::<Vec<_>>(),
          vec!["Sensor.A", "Sensor.B"]
      );

      let single_orig = TagBatch::from_str_lenient("Sensor.Single");
      let single_cloned = (&single_orig).into_tag_batch();
      assert_eq!(single_cloned, single_orig);

      // Edge case: empty batch reference
      let empty_orig = TagBatch::empty();
      let empty_cloned = (&empty_orig).into_tag_batch();
      assert_eq!(empty_cloned, empty_orig);
      assert_eq!(empty_cloned.len(), 0);
      assert!(empty_cloned.is_empty());
  }
  ```
- Post: RED(`test_into_tags_batch_ref`)

Step 6: [MODIFY] `opc-da-client/src/types/batch.rs` — [+] `impl IntoTags for &TagBatch`, `impl From<&TagBatch> for TagBatch` (Finding #7) 🔒
- Pre: RED(`test_into_tags_batch_ref`)
- Target: `src/types/batch.rs:410-430`
- Action:
  Add implementations for `&TagBatch`:
  ```rust
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
- Post: GREEN(`test_into_tags_batch_ref`) 🔒

Step 7: [TEST] `opc-da-client/src/types/value.rs` — [+] `test_opc_value_from_ref` (Finding #7)
- Pre: CHECK
- Target: `src/types/value.rs:783+`
- Action:
  Add unit test verifying `From<&OpcValue> for OpcValue`:
  ```rust
  #[test]
  fn test_opc_value_from_ref() {
      let cases = vec![
          OpcValue::Int(-12345),
          OpcValue::UInt(67890),
          OpcValue::Float(3.14159),
          OpcValue::Bool(true),
          OpcValue::String("OPC DA Tag".to_string()),
          OpcValue::Empty,
          OpcValue::Null,
      ];

      for val in &cases {
          let cloned = OpcValue::from(val);
          assert_eq!(&cloned, val);
      }
  }
  ```
- Post: RED(`test_opc_value_from_ref`)

Step 8: [MODIFY] `opc-da-client/src/types/value.rs` — [+] `impl From<&OpcValue> for OpcValue` (Finding #7) 🔒
- Pre: RED(`test_opc_value_from_ref`)
- Target: `src/types/value.rs:240-250`
- Action:
  Add `From<&OpcValue>` implementation:
  ```rust
  impl From<&OpcValue> for OpcValue {
      #[inline]
      fn from(val: &OpcValue) -> Self {
          val.clone()
      }
  }
  ```
- Post: GREEN(`test_opc_value_from_ref`) 🔒

Step 9: [TEST] `opc-da-client/src/types/write_batch.rs` — [+] `test_into_write_batch_ref` (Finding #7)
- Pre: CHECK
- Target: `src/types/write_batch.rs:527+`
- Action:
  Add unit test verifying non-consuming `&WriteBatch` conversions:
  ```rust
  #[test]
  fn test_into_write_batch_ref() {
      let single_orig = ("Motor.Speed", OpcValue::Int(1500)).into_write_batch();
      let single_ref_batch = (&single_orig).into_write_batch();
      assert_eq!(single_ref_batch, single_orig);
      let single_from_ref: WriteBatch = (&single_orig).into();
      assert_eq!(single_from_ref, single_orig);
      assert_eq!(single_orig.len(), 1);

      let owned_orig = vec![
          ("V1".to_string(), OpcValue::Bool(true)),
          ("V2".to_string(), OpcValue::Bool(false)),
      ]
      .into_write_batch();
      let owned_ref_batch = (&owned_orig).into_write_batch();
      assert_eq!(owned_ref_batch, owned_orig);
      assert_eq!(owned_orig.len(), 2);
  }
  ```
- Post: RED(`test_into_write_batch_ref`)

Step 10: [MODIFY] `opc-da-client/src/types/write_batch.rs` — [+] `impl From<&WriteBatch> for WriteBatch` (Finding #7)
- Pre: RED(`test_into_write_batch_ref`)
- Target: `src/types/write_batch.rs:370-380`
- Action:
  Add `From<&WriteBatch>` implementation satisfying blanket `IntoWriteBatch`:
  ```rust
  impl From<&WriteBatch> for WriteBatch {
      #[inline]
      fn from(batch: &WriteBatch) -> Self {
          batch.clone()
      }
  }
  ```
- Post: GREEN(`test_into_write_batch_ref`)

Step 11: [TEST] `opc-da-client/src/types/write_batch.rs` — [+] `test_into_write_batch_borrowed_value_tuples` (Finding #7)
- Pre: GREEN(`test_into_write_batch_ref`)
- Target: `src/types/write_batch.rs:550+`
- Action:
  **Integration Verification (no MODIFY step required):** This test verifies the cross-module synergy between Step 8 (`From<&OpcValue> for OpcValue`) and the pre-existing generic slice conversion `impl<S: AsRef<str>, V: Clone + Into<OpcValue>> From<&[(S, V)]> for WriteBatch` at `write_batch.rs:360`. Step 8's `From<&OpcValue>` satisfies the `V: Into<OpcValue>` bound, instantly unlocking `&[(&str, &OpcValue)]` conversions without additional `[MODIFY]` steps.
  Add unit test verifying `&[(&str, &OpcValue)]` slice conversions:
  ```rust
  #[test]
  fn test_into_write_batch_borrowed_value_tuples() {
      let val_int = OpcValue::Int(42);
      let val_float = OpcValue::Float(98.6);
      let val_str = OpcValue::String("Running".to_string());

      let tuple_slice: &[(&str, &OpcValue)] = &[
          ("Device.Motor.RPM", &val_int),
          ("Device.Motor.Temp", &val_float),
          ("Device.Motor.Status", &val_str),
      ];

      let batch = tuple_slice.into_write_batch();
      assert_eq!(batch.len(), 3);
      assert!(!batch.is_empty());

      let items: Vec<(&str, &OpcValue)> = batch.iter().collect();
      assert_eq!(items.len(), 3);
      assert_eq!(items[0], ("Device.Motor.RPM", &val_int));
      assert_eq!(items[1], ("Device.Motor.Temp", &val_float));
      assert_eq!(items[2], ("Device.Motor.Status", &val_str));

      // Edge case: empty borrowed tuple slice
      let empty_tuples: &[(&str, &OpcValue)] = &[];
      let empty_batch = empty_tuples.into_write_batch();
      assert_eq!(empty_batch.len(), 0);
      assert!(empty_batch.is_empty());
  }
  ```
- Post: GREEN(`test_into_write_batch_borrowed_value_tuples`)

Step 12: [TEST] `opc-da-client/src/types/write_batch.rs` — [+] `test_write_batch_into_shareable_lifecycle` (Finding #8)
- Pre: GREEN(`test_into_write_batch_borrowed_value_tuples`)
- Target: `src/types/write_batch.rs:580+`
- Action:
  Add comprehensive unit test covering all `into_shareable` paths — Single→Shared, Owned→Shared, Shared idempotency, empty batch, and drop lifecycle:
  ```rust
  #[test]
  fn test_write_batch_into_shareable_lifecycle() {
      // 1. Single variant → Shared with Arc lifecycle
      let single = WriteBatch::Single("Pump.Pressure".to_string(), OpcValue::Float(101.3));
      let shareable = single.into_shareable();

      match &shareable {
          WriteBatch::Shared(arc_slice) => {
              assert_eq!(arc_slice.len(), 1);
              assert_eq!(arc_slice[0].0, "Pump.Pressure");
              assert_eq!(arc_slice[0].1, OpcValue::Float(101.3));
              assert_eq!(Arc::strong_count(arc_slice), 1);

              let cloned = shareable.clone();
              assert_eq!(Arc::strong_count(arc_slice), 2);
              assert_eq!(cloned, shareable);

              drop(cloned);
              assert_eq!(Arc::strong_count(arc_slice), 1);
          }
          _ => panic!("Expected WriteBatch::Shared representation for shareable single"),
      }

      // 2. Owned variant → Shared
      let owned = vec![
          ("M1.Speed".to_string(), OpcValue::Int(1200)),
          ("M2.Speed".to_string(), OpcValue::Int(1500)),
      ].into_write_batch();
      let shareable_owned = owned.into_shareable();
      match &shareable_owned {
          WriteBatch::Shared(arc_slice) => {
              assert_eq!(arc_slice.len(), 2);
              assert_eq!(Arc::strong_count(arc_slice), 1);
              let cloned = shareable_owned.clone();
              assert_eq!(Arc::strong_count(arc_slice), 2);
              drop(cloned);
          }
          _ => panic!("Expected WriteBatch::Shared for owned batch"),
      }

      // 3. Shared idempotency
      let already_shared = shareable_owned.clone().into_shareable();
      assert_eq!(already_shared, shareable_owned);

      // 4. Empty batch shareable
      let empty_shareable = WriteBatch::empty().into_shareable();
      assert_eq!(empty_shareable.len(), 0);
      assert!(empty_shareable.is_empty());
  }
  ```
- Post: RED(`test_write_batch_into_shareable_lifecycle`)

Step 13: [MODIFY] `opc-da-client/src/types/write_batch.rs` — [~] `WriteBatch::into_shareable` wrap `Single` in `Arc` (Finding #8) 🔒
- Pre: RED(`test_write_batch_into_shareable_lifecycle`)
- Target: `src/types/write_batch.rs:184-190`
- Action:
  Update `into_shareable` to wrap `Single(tag, val)` in `Arc` via boxed slice.
  > **Optimization opportunity (optional):** `Arc::from([(tag, val)])` via `From<[T; N]> for Arc<[T]>` (stable since Rust 1.71) would reduce from 2 heap allocations to 1. The `vec![].into_boxed_slice()` approach below is the safe baseline per Cycle 1 review. The Builder may attempt the direct array form if compilation succeeds with the type-inferred context of `Self::Shared(...)`.

  ```rust
  #[must_use]
  pub fn into_shareable(self) -> Self {
      match self {
          Self::Single(tag, val) => Self::Shared(std::sync::Arc::from(vec![(tag, val)].into_boxed_slice())),
          Self::Shared(slice) => Self::Shared(slice),
          Self::Owned(vec) => Self::Shared(std::sync::Arc::from(vec.into_boxed_slice())),
      }
  }
  ```
- Post: GREEN(`test_write_batch_into_shareable_lifecycle`) 🔒

Step 14: [TEST] `opc-da-client/src/types/collection.rs` — [+] `test_tag_values_contains_case_insensitive` (Finding #9)
- Pre: CHECK
- Target: `src/types/collection.rs:1638+`
- Action:
  Add unit test for `TagValues::contains`:
  ```rust
  #[test]
  fn test_tag_values_contains_case_insensitive() {
      let items = vec![
          TagValue::new(
              "Channel1.Device1.SensorA",
              Some(OpcValue::Int(100)),
              OpcQuality::GOOD,
              None,
          ),
          TagValue::new(
              "CHANNEL2.DEVICE2.SENSORB",
              Some(OpcValue::Float(25.4)),
              OpcQuality::GOOD,
              None,
          ),
      ];
      let values = TagValues::new(items);

      assert!(values.contains("Channel1.Device1.SensorA"));
      assert!(values.contains("channel1.device1.sensora"));
      assert!(values.contains("CHANNEL1.DEVICE1.SENSORA"));
      assert!(values.contains("channel2.device2.sensorb"));
      assert!(values.contains("Channel2.Device2.SensorB"));

      assert!(!values.contains("Channel1.Device1.SensorC"));
      assert!(!values.contains("NonExistentTag"));
      assert!(!values.contains(""));

      let empty_values = TagValues::default();
      assert_eq!(empty_values.len(), 0);
      assert!(!empty_values.contains("Channel1.Device1.SensorA"));
  }
  ```
- Post: RED(`test_tag_values_contains_case_insensitive`)

Step 15: [MODIFY] `opc-da-client/src/types/collection.rs` — [+] `TagValues::contains` (Finding #9) 🔒
- Pre: RED(`test_tag_values_contains_case_insensitive`)
- Target: `src/types/collection.rs:420-435` (Inside existing `impl TagValues` block, immediately after `get`)
- Action:
  Implement `TagValues::contains` with complete rustdoc specifications:
  ```rust
      /// Returns `true` if the collection contains a tag with the specified identifier (case-insensitive).
      ///
      /// Comparison uses ASCII case folding (`A-Z` / `a-z`), matching OPC DA specifications
      /// and the behavior of [`TagValues::get`]. Non-ASCII characters require exact casing.
      ///
      /// # Arguments
      ///
      /// * `tag` - Tag identifier string to look up.
      ///
      /// # Returns
      ///
      /// `true` if a tag with the specified identifier is present in this collection; `false` otherwise.
      ///
      /// # Examples
      ///
      /// ```
      /// use opc_da_client::{OpcQuality, OpcValue, TagValue, TagValues};
      ///
      /// let values = TagValues::new(vec![TagValue::new(
      ///     "Channel1.Device1.Sensor1",
      ///     Some(OpcValue::Int(10)),
      ///     OpcQuality::GOOD,
      ///     None,
      /// )]);
      /// assert!(values.contains("channel1.device1.sensor1"));
      /// assert!(!values.contains("Sensor2"));
      /// ```
      #[inline]
      #[must_use]
      pub fn contains(&self, tag: &str) -> bool {
          self.get(tag).is_some()
      }
  ```
- Post: GREEN(`test_tag_values_contains_case_insensitive`) 🔒

Step 16: [MODIFY] `opc-da-client/src/client/subscription.rs` — [~] `OpcDaClient::subscribe` rustdoc & Gate 7 doctest (Finding #10a)
- Pre: CHECK
- Target: `src/client/subscription.rs:10-20`
- Action:
  Add `# Arguments`, `# Returns`, `# Panics`, and `# Examples` doctest strictly complying with Gate 7 (zero `println!`):
  ```rust
  /// Subscribes to a stream of tag value updates polled at the specified interval.
  ///
  /// Spawns a background Tokio task that periodically polls the configured tags on the bound
  /// server and streams updates through a Tokio [`mpsc::Receiver`]. Dropping the receiver
  /// automatically terminates the background polling loop.
  ///
  /// # Arguments
  ///
  /// * `tags` - Tag batch or convertible source to poll. Accepts any type implementing [`IntoTags`]
  ///   (e.g., `&str`, `[&str; N]`, `&[&str]`, `&TagBatch`, or `Vec<String>`).
  /// * `interval` - Polling interval duration. Clamped to a minimum of 10 milliseconds to prevent
  ///   accidental worker starvation.
  ///
  /// # Returns
  ///
  /// A Tokio [`Receiver<TagValues>`] streaming polled tag updates on each tick.
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
  /// if let Some(values) = rx.recv().await {
  ///     if let Some(value) = values.get_value("Sensor.1") {
  ///         let _ = value;
  ///     }
  /// }
  /// // Dropping `rx` shuts down the background polling loop.
  /// drop(rx);
  /// # }
  /// ```
  ```
- Post: CHECK

Step 17: [TEST] `opc-da-client/src/client/subscription.rs` — [+] `test_subscription_doctest_compiles` (Finding #10a)
- Pre: Step 16
- Target: `src/client/subscription.rs:10-25`
- Action:
  Verify that subscription documentation doctest compiles and runs via `cargo test --doc -p opc-da-client -- subscription`.
- Post: GREEN(`cargo test --doc -p opc-da-client -- subscription`) 🔒

Step 18: [TEST] Workspace verification — `pwsh -File scripts/verify.ps1` 🔒
- Pre: ALL
- Target: Workspace root
- Action:
  Run the full 8-gate verification pipeline (`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --doc`, `cargo test --workspace`, forbidden macro check, AST grep safety).
- Post: VERIFIED 🔒

### Verification Plan
| Type | Command | Expected Result |
|---|---|---|
| Unit Test: Batch SSO & Slices | `cargo test -p opc-da-client --lib types::batch` | All batch SSO, dynamic str, borrowed slices, and batch ref tests pass |
| Unit Test: Value Ref | `cargo test -p opc-da-client --lib types::value` | `test_opc_value_from_ref` passes |
| Unit Test: Write Batch | `cargo test -p opc-da-client --lib types::write_batch` | Write batch ref, tuple slice, and single shareable tests pass |
| Unit Test: Collection Contains | `cargo test -p opc-da-client --lib types::collection` | `test_tag_values_contains_case_insensitive` passes |
| Doctest Suite | `cargo test --doc -p opc-da-client` | All doctests compile and pass, including subscription |
| Workspace Check | `cargo check --workspace --all-targets --all-features` | Zero errors across `opc-da-client` and `opc-cli` |
| Full 8-Gate Pipeline | `pwsh -File scripts/verify.ps1` | All 8 gates pass with exit code 0 |

### Plan Summary
| Metric | Value |
|---|---|
| Tier | M (Feature / Ergonomics) |
| Files Modified | 5 (`src/types/batch.rs`, `src/types/write_batch.rs`, `src/types/value.rs`, `src/types/collection.rs`, `src/client/subscription.rs`) |
| Steps | 18 steps (8 Red-Green TDD cycles) |
| Checkpoints | 6 (Step 6, Step 8, Step 13, Step 15, Step 17, Step 18) |
| Estimated Effort | Medium |
| Review Cycles | 3 (2 plan-reviewer + 1 seven-lens assessment) |
