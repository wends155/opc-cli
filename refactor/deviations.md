# Refactoring Cycle 2 Implementation Deviations Log

> **Document Status:** Active Engineering Artifact (Cycle 2)  
> **Workspace:** `opc-cli`  
> **Target Crate:** `opc-da-client`  
> **Scope:** Formal record and justification of implementation deviations encountered during Cycle 2 refactoring blocks. Evaluated and approved under the TARS protocol (Fidelity Matrix per `audit-rules.md §4` and `builder-rules.md §1`).  
> **Historical Precedent:** For Cycle 1 deviations (Blocks 1–7 and A–F), see [`refactor/cycle1_reference.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle1_reference.md).

---

## 1. Cycle 2 Deviations Overview

During Cycle 2, each deviation from the approved implementation plan is recorded with its compiler/language dynamic, architectural justification, and resulting invariant.

### Summary Metrics
* **Total Cycle 2 Deviations:** 26
* **Block G1 (Clean Slate API Excision & Struct Deduplication):** 3 deviations (0 violations, all justified and verified)
* **Block G2 (Ergonomic Symmetry & Comprehensive Public Documentation):** 4 deviations (0 violations, all justified and verified)
* **Block H1 (Domain Invariants & CWE-626 Hardening):** 2 deviations (0 violations, all justified and verified)
* **Block H2 (COM Resource & Dead Code Pruning):** 3 deviations (0 violations, all justified and verified)
* **Sub-Block H3a (Server Identity & Host Canonicalization):** 2 deviations (0 violations, all justified and verified)
* **Sub-Block H3b (Worker Active Group Caching & Batch Defense):** 2 deviations (0 violations, all justified and verified)
* **Sub-Block H3c (Batch Ergonomics & Public Conversions):** 2 deviations (0 violations, all justified and verified)
* **Sub-Block I1 (COM Worker Hygiene & Buffer Reuse):** 4 deviations (0 violations, all justified and verified)
* **Sub-Block I2 (Batch Write Allocation & Defensive Hardening):** 1 deviation (0 violations, all justified and verified)
* **Sub-Block I3 (Collector Concurrency, Zero-Copy Handoff & Traversal Parity):** 1 deviation (0 violations, all justified and verified)
* **Sub-Block I4 (WriteBatch Encapsulation & Small String Optimization):** 2 deviations (0 violations, all justified and verified)
* **Quality Gate Verification:** 100% Green across all 9 quality gates in `scripts/verify.ps1`.

---

## 2. Master Implementation Deviations Matrix

| Block ID | Finding / Step | Planned Approach | Implemented Deviation | Rationale & Compiler Dynamic | Category | Resulting Invariant |
|:---:|:---:|---|---|---|:---:|---|
| **Block G1** | Step 3<br>Finding #16 | `impl<C: ServerBackend + Default> OpcDaClientBuilder<C> { pub fn new() -> Self }` | `impl OpcDaClientBuilder<DefaultBackendConnector> { pub fn new() -> Self }` | Rust compiler error `E0283` ("type annotations needed for `T`"). A parameterless generic `new()` method cannot infer `C` at call sites like `OpcDaClientBuilder::new()` without turbofish annotations. Implementing `new()` directly on `OpcDaClientBuilder<DefaultBackendConnector>` resolves type inference unambiguously. Generic connector construction remains fully supported via `Default` (`OpcDaClientBuilder::<C>::default()`) and `new_with_connector(C::default())`. | Language Invariant & API Ergonomics | Downstream callers and doctests can write concise `OpcDaClientBuilder::new()` targeting the default backend without turbofish syntax. |
| **Block G1** | Step 4<br>Finding #1 | Provider test calling `crate::types::WriteBatch::Borrowed(&[...])` | Provider test calling `crate::types::WriteBatch::Owned(vec![("Tag.Fail".into(), OpcValue::Int(1)), ("Tag.Pass".into(), OpcValue::Int(2))])` | The `WriteBatch` enum defines `Single`, `Shared`, and `Owned` variants; no `Borrowed` variant exists in the domain type hierarchy. Using `WriteBatch::Owned` with owned strings accurately exercises the batch write failure path without fabricating non-existent enum variants. | Type Correctness | Test suite validates against actual `WriteBatch` domain variants without invalid enum constructions. |
| **Block G1** | Step 4<br>Finding #1 | Purge `#[allow(deprecated)]` scoped only to `provider.rs` and `gateway.rs` | Removed stale `#[allow(deprecated)]` and renamed `test_endpoint_deprecated_from_str_behavior` to `test_endpoint_from_str_behavior` in `src/types/server.rs:1180` | Plan Objective O4 mandated clean removal of all deprecations. Ripgrep audit discovered an obsolete `#[allow(deprecated)]` on a test in `types/server.rs` whose target (`From<&str> for OpcServerEndpoint`) was no longer deprecated. Purging this stale lint suppression achieved a 100% deprecation-free crate (0 matches for `allow(deprecated)` across `opc-da-client/src/`). Pre-approved under `builder-rules.md §4.2`. | Code Quality & Governance | Exactly zero `#[allow(deprecated)]` suppressions exist anywhere across `opc-da-client/src/`. |
| **Block G2** | Step 9<br>Finding #15 | Bare `#[must_use]` on `TagValues::get_value_checked` | `#[must_use = "handling the Result distinguishes unrequested tags from server read failures"]` on `TagValues::get_value_checked` in `src/types/collection.rs:476` | In Rust, `Result` is already marked `#[must_use]`. Applying a bare `#[must_use]` to a function returning `Result<&OpcValue, TagExtractError>` triggers `clippy::double-must-use`. Adding an explanatory message resolves the lint error under `-D warnings` while providing actionable compiler diagnostics to callers distinguishing unrequested tags from server read errors. | Lint Rule Invariant (`clippy::double-must-use`) | `TagValues::get_value_checked` carries an informative must-use diagnostic while passing zero-warning linter checks. |
| **Block G2** | Step 14<br>Finding #11 | `client.write_tag_batch(server, writes.into()).await` in `tests/batch_write_test.rs:46` | `client.write_tag_batch(server, writes).await` | `OpcDaClient<C, Unbound>::write_tag_batch` was upgraded to accept `impl IntoWriteBatch` directly. Chaining `.into()` became redundant and caused a compiler type-inference ambiguity error (`E0282`) under `-D warnings`. Passing `writes` directly aligns with the new generic signature. | Type Inference & API Ergonomics | Integration test directly exercises generic parameter polymorphism without extraneous manual conversions. |
| **Block G2** | Steps 10, 11<br>Finding #5 | Doc comment examples using `println!("{:?}", ...);` across `session.rs` and `gateway.rs` | Doc comment examples using `assert!(...);` or `let _ = ...;` bindings | Gate 7 (Forbidden Pattern Scanner in `scripts/verify.ps1`) enforces a strict zero-tolerance scan (`rg "\b(println!|dbg!|todo!)" opc-da-client/src/`) for production and doc code in library crates. Replacing `println!` with testable assertions and discarded bindings respects the forbidden pattern guard while keeping doctests compilable and runnable. | Governance & Code Standard (`coding-standard.md §4.8`) | 100% zero-forbidden-macro compliance across all source and doc comments in `opc-da-client`. |
| **Block G2** | Step 2<br>Finding #11 | Existing unit test `test_write_batch_vec_and_into_iter` with `("Tag1".into(), ...)` | Adjusted tuple to `("Tag1".to_string(), ...)` in `src/types/write_batch.rs:438` | Generalizing `From<Vec<(S, V)>>` over `S: Into<String>` and `V: Into<OpcValue>` introduced an inference ambiguity for `vec![("Tag1".into(), ...)]`, as `&str::into()` cannot infer the intermediate target type `S` when collecting into a polymorphic container. Specifying `.to_string()` provides unambiguous type information. | Type Inference Invariant | Unit tests compile cleanly without type annotation ambiguity. |
| **Block H1** | Step 6<br>Finding #5 | Retain `impl From<S> for LocalPointer` alongside `TryFrom<&str>` | Deleted unchecked `impl From<S> for LocalPointer` from `src/raw/memory.rs` | Rust's blanket `impl<T, U> TryFrom<U> for T where U: Into<T>` caused compiler error `E0119` (conflicting trait implementations) when implementing fallible `TryFrom<&str>` while keeping infallible `From<S>`. Excising unchecked `From<S>` cleanly resolved the conflict, strictly enforced the Clean Slate paradigm, and guaranteed at compile time that all UTF-16 COM conversions reject interior null bytes (`\0`). | Language Invariant & Security Hardening (`E0119`) | `LocalPointer` cannot be constructed infallibly from arbitrary string types; all callers must use fallible `try_from_str` or `try_into()`, eliminating CWE-626 null-byte truncation at compile time. |
| **Block H1** | Step 9<br>Gate 4b | `#[cfg(feature = "opc-da-backend")] use crate::types::ServerIdentifier;` in `src/client/mod.rs` | Unconditional `use crate::types::{OpcServerEndpoint, ServerIdentifier};` in `src/client/mod.rs` | `OpcDaClient::bind_new_remote` accepts `impl TryInto<ServerIdentifier>` on `impl OpcDaClient<DefaultBackendConnector, Unbound>`. In headless builds (`--no-default-features`), `DefaultBackendConnector` resolves to `NoopServerBackend`. Gating the import behind `opc-da-backend` caused `E0425: cannot find type ServerIdentifier in this scope` during Gate 4b verification. Unconditionally importing the pure domain type resolved the issue. | Feature Independence & Cross-Target Compilation | Client constructors and domain type bindings compile cleanly across all feature combinations including `--no-default-features`. |
| **Block H2** | Step 1<br>Finding #1 | `&*(proxy as *const T as *const windows::core::IUnknown)` | `&*std::ptr::from_ref(proxy).cast::<windows::core::IUnknown>()` | Clippy `-D warnings` triggered `clippy::ptr_as_ptr` and `clippy::ref_as_ptr` on raw pointer `as` casting. Using standard library `std::ptr::from_ref(proxy).cast()` satisfies all compiler safety lints in Rust 2024 / MSRV 1.93.1 while achieving exact proxy pointer re-borrowing without `QueryInterface` copies. | Lint Rule Invariant (`clippy::ptr_as_ptr`, `clippy::ref_as_ptr`) | Exact proxy pointer re-borrowing passes strict linting without compiler warnings or ephemeral COM proxy drops. |
| **Block H2** | Steps 1, 2<br>Gate 6 | `// SAFETY: ...` on first line of multi-line comment | `// SAFETY:` on every line of multi-line comment | In tree-sitter-rust and AST-Grep `require-safety-comment`, each `//` is an individual `line_comment` node. The AST-Grep `follows` selector inspects the immediate preceding sibling token; if subsequent lines in a multi-line comment lack `SAFETY:`, the immediate preceding comment node fails the regex match. Repeating `// SAFETY:` on each line ensures 100% compliance with Gate 6 structural safety scans. | AST Linting Invariant (`require-safety-comment`) | Unsafe blocks in production code pass AST-Grep structural validation without missing-rationale false positives. |
| **Block H2** | Step 10<br>Finding #10 | `let _ = unsafe { self.server.RemoveGroup(raw_server_handle, true) };` | `let remove_res = unsafe { self.server.RemoveGroup(...) }; if let Err(rm_err) = remove_res { tracing::warn!(...); }` | Directly binding the unsafe call ensures the `// SAFETY:` rationale directly precedes the `unsafe` block for clippy (`undocumented_unsafe_blocks`). Logging cleanup failures via `tracing::warn!` upholds `coding-standard.md §4.8` (no silent failures), preventing unobservable group handle leakage. | Error Handling & Governance (`coding-standard.md §4.8`) | Server-side group cleanup failures are logged with full error and handle context rather than silently swallowed. |
| **Sub-Block H3a** | Step 13<br>Finding #8 | Doctest for `OpcServerEndpoint::local` using string literal `local("...")` | Doctest using `ServerIdentifier::new("...").unwrap()` passed to `OpcServerEndpoint::local(id)` | In Block H1, infallible `From<&str>` for `ServerIdentifier` was deleted under the Clean Slate paradigm. Passing a `&str` literal directly to `local(identifier: impl Into<ServerIdentifier>)` fails compilation because `&str` no longer implements `Into<ServerIdentifier>`. Pre-constructing via `ServerIdentifier::new("...").unwrap()` ensures doctests compile and pass under Gate 3 (`cargo test --doc`). | Type Correctness & Doctest Governance | Public doctests reflect actual fallible domain constructor requirements. |
| **Sub-Block H3a** | Step 8<br>TDD Protocol | Direct addition of `test_endpoint_matches` expecting RED | Added minimal stub `pub fn matches(&self, _other: &Self) -> bool { false }` before test addition | Adding a test referencing a non-existent method causes `E0599` (compiler error), which halts compilation before assertions can execute. To uphold the strict TDD contract where RED signifies a compilable, executing test that fails an assertion, a `false`-returning stub was introduced so `cargo test` compiled and reported a clean runtime assertion failure, followed by GREEN implementation in Step 9. | TDD Methodology & Compilable RED Phase | All TDD RED phases compile cleanly and verify runtime assertion failures. |
| **Sub-Block H3b** | Step 9<br>Finding #2 | `tags: impl ExactSizeIterator<Item = &str>` in `assemble_tag_values` | `tags: impl ExactSizeIterator<Item = &'a str>` with named lifetime `<'a>` in `assemble_tag_values` | Rust compiler error `E0658` ("anonymous lifetimes in `impl Trait` are unstable"). In Rust 2024 / MSRV 1.93.1, elided/anonymous lifetimes inside associated type trait bounds in function argument position require a named lifetime parameter. Introducing `<'a>` resolves `E0658` on stable Rust without nightly `#![feature(...)]` gates. | Language Invariant (`E0658`) / Compiler Stability | Clean compilation on stable Rust toolchain without nightly feature dependencies. |
| **Sub-Block H3b** | Step 12<br>Finding #5 | `(format!("Tag.{i}"), OpcValue::Int(i as i32))` in batch write tests | `(format!("Tag.{i}"), OpcValue::Int(i as i64))` in batch write tests | Rust compiler error `E0308` ("mismatched types: expected `i64`, found `i32`"). The domain model defines `OpcValue::Int(i64)` (`src/types/value.rs:24`), not `i32`. Casting loop indices to `i64` accurately conforms to the domain type variant definition. | Type Correctness (`E0308`) | Test fixtures construct `OpcValue::Int` variants with strictly valid 64-bit integer payloads without type errors. |
| **Sub-Block H3c** | Step 7<br>Finding #7 | `OpcValue::Float(3.14159)` in test cases | `OpcValue::Float(std::f64::consts::PI)` in `src/types/value.rs:795` | Rust clippy `-D warnings` triggered `clippy::approx_constant` on the literal `3.14159`. Replacing with `std::f64::consts::PI` eliminates the linter failure while accurately testing the float variant. | Lint Rule Invariant (`clippy::approx_constant`) | Float variants in test suites conform to standard library constant precision rules. |
| **Sub-Block H3c** | Step 13<br>Finding #8 | `Arc::from(vec![(tag, val)].into_boxed_slice())` | `Arc::from([(tag, val)])` in `src/types/write_batch.rs:186` | Standard library `From<[T; N]> for Arc<[T]>` directly constructs a boxed slice Arc from a 1-element array without intermediate `Vec` heap allocation or re-allocation. Pre-approved during interview. | Performance & Zero-Intermediate Allocation Optimization | Single write batch sharing wraps into Arc with zero intermediate vector overhead. |
| **Sub-Block I1** | Step 6<br>Finding #3 | `GroupItemResult { server_handle: ..., error: ... }` in test fixture | `GroupItemResult { server_handle: ..., canonical_type: VarType::EMPTY, error: ... }` in `tests/connection_pool_test.rs` | Rust compiler error `E0063`: missing field `canonical_type` on struct `GroupItemResult`. The domain struct definition (`src/connector/traits.rs:83`) requires `canonical_type: VarType`. Providing `VarType::EMPTY` satisfies struct construction invariants for mock test doubles without invalidating server error assertions. | Type Correctness & Test Fixture Conformance (`E0063`) | Test fixtures constructing `GroupItemResult` explicitly supply `canonical_type` conforming to the domain struct contract. |
| **Sub-Block I1** | Step 7<br>Finding #4 | `Arc::new(MockServerConnector::with_state(state.clone()))` in connection pool test | `Arc::new(MockServerConnector::with_state(state))` in `tests/connection_pool_test.rs:188` | Clippy `-D warnings` triggered `clippy::redundant_clone` because the local binding `state` was dropped immediately after without further reads or references. Passing `state` by move avoids unnecessary atomic reference count increments. | Lint Rule Invariant (`clippy::redundant_clone`) | Test setup transfers ownership directly without redundant clones when bindings are not subsequently accessed. |
| **Sub-Block I1** | Step 10<br>Finding #1 | In-place buffer reuse via `chunk.drain(..)` in `browse_flat_namespace` and `try_fast_flat_browse` | Scoped `#[allow(clippy::iter_with_drain)]` on `browse_flat_namespace` and `try_fast_flat_browse` in `src/com/worker/browse.rs` | Clippy nursery lint `iter_with_drain` denies calling `drain(..)` under `-D warnings` and suggests `into_iter()`. However, calling `into_iter()` drops and deallocates the underlying `Vec` buffer, directly violating Plan Objective O1 (reusing a single pre-allocated 256-item vector buffer across intermediate and terminal flushes to eliminate ~39 heap allocations per 10k tags). Scoped suppression preserves the high-performance buffer-reuse invariant while passing zero-warning linter checks. | Performance Optimization & Nursery Lint Exemption | In-place vector buffer reuse is preserved across browse batch pushes without buffer re-allocation or lint failure. |
| **Sub-Block I1** | Step 12<br>Finding #2 | Action 1: "Remove unused import `ConnectedGroup` from line 12" in `src/com/worker.rs` | Retained `use crate::connector::ConnectedGroup;` in `src/com/worker.rs` | In Rust, calling trait methods requires the trait to be in scope. `src/com/worker.rs:94` calls `group.add_items(&item_defs)` where `group: S::Group` and `add_items` is defined on trait `ConnectedGroup`. Removing the import triggered compiler error `E0599: no method named add_items found for associated type <S as traits::ConnectedServer>::Group in the current scope`. Retaining the trait import satisfies Rust's trait visibility requirements. | Language Invariant & Trait Visibility (`E0599`) | `ConnectedGroup` trait remains in scope in `worker.rs` to allow dispatch of group management trait methods. |
| **Sub-Block I2** | Step 7 / Step 10<br>Finding #1 | `let server_write_results = Some(vec![Ok(()), Err(OpcError::Com(0x8000_4005))]);` or raw cast `0x8000_4005u32 as i32` | `let server_write_results = Some(vec![Ok(()), Err(OpcError::Com { source: windows_core::Error::from_hresult(crate::errors::hresult::E_FAIL) })]);` in `src/com/worker/write.rs:732` | Clippy `-D warnings` triggered `clippy::cast_possible_wrap` on high-bit raw integer cast `0x8000_4005u32 as i32` for Win32 HRESULT in test fixtures, and `OpcError::Com` is a struct variant `OpcError::Com { source: windows_core::Error }`. Using canonical constant `crate::errors::hresult::E_FAIL` (`HRESULT(0x8000_4005_u32.cast_signed())`) guarantees zero-warning compiler compliance and single-source-of-truth HRESULT error modeling. | Lint Rule Invariant (`clippy::cast_possible_wrap`) & Error Struct Conformance | Test fixtures construct Win32 COM error variants via canonical `crate::errors::hresult` constants rather than ad-hoc inline casts or obsolete tuple syntax. |
| **Sub-Block I3** | Step 3 / Step 4<br>Finding #3 | `let server = MockConnectedServer::default().with_tags(...).with_organization(...)` | `let server = MockConnectedServer::default().with_tags(...).with_branch_tags(Vec::new()).with_organization(...)` in `src/com/worker/browse.rs:414` | In `MockConnectedServer::default()`, simulated branch tags default to `["Random", "Simulation"]`. In a recursive walk test validating 256-chunking for 300 leaves (`supports_flat_browse = false`), traversing simulated child branches causes the mock server to re-enumerate the same 300 leaf tags at depth 1, saturating collector capacity to 500 (`300 + 200`). Chaining `.with_branch_tags(Vec::new())` eliminates simulated branch traversal and cleanly isolates the 300 leaf chunking test. | Test Fixture Isolation & Mock State Precision | Recursive browse tests targeting pure leaf chunking explicitly clear simulated mock branch tags to ensure exact item count verification. |
| **Sub-Block I4** | Step 7<br>Finding #1 | `vec![("Tag.1".into(), OpcValue::Int(10)), ...].into_write_batch()` | `vec![("Tag.1", OpcValue::Int(10)), ...].into_write_batch()` in `src/provider.rs:518, 568` | Rust compiler error `E0283` ("type annotations needed"). Because `Vec<(S, V)>::into_write_batch()` is generic over `S: Into<String>`, chaining `.into()` on string literals left type inference unconstrained. Providing concrete `&str` literals eliminated type ambiguity while maintaining zero-reallocation generic collection conversion. | Type Inference Invariant (`E0283`) | Test fixtures use concrete string slices that infer unambiguously into `IntoWriteBatch`. |
| **Sub-Block I4** | Step 9<br>Gate Pipeline | `Join-Path $PSScriptRoot ".." "compat"` in `scripts/verify.ps1` | Standardized to 2-parameter `Join-Path $PSScriptRoot "..\compat"` across all paths in `scripts/verify.ps1` | Windows PowerShell 5.1 (`powershell.exe`) `Join-Path` only accepts 2 positional arguments (`-Path` and `-ChildPath`). Supplying 3 positional arguments triggers `PositionalParameterNotFound`. Using `Join-Path $PSScriptRoot "..\..."` ensures strict multi-shell compatibility across both Windows PowerShell 5.1 and PowerShell 7+ (`pwsh`). | Toolchain & Shell Compatibility | Verification script executes reliably across all supported Windows PowerShell and PowerShell Core runtimes. |

---

## 3. Technical Analysis & Case Studies

### 3.1 Case Study G1.1: Associated Function Type Inference Ambiguity (`E0283`)
* **Context:** The plan called for consolidating triplicate `OpcDaClientBuilder` struct definitions into a single generic struct with `C = DefaultBackendConnector` as the default type parameter, and providing a generic constructor `impl<C: ServerBackend + Default> OpcDaClientBuilder<C> { pub fn new() -> Self }`.
* **Compiler Obstacle:** When Rust compiles `let builder = OpcDaClientBuilder::new();`, the default type parameter `C = DefaultBackendConnector` on the struct declaration does **not** automatically constrain associated functions that are generically defined over an `impl<C>` block. Because multiple types implement `ServerBackend + Default` (e.g. `ComConnector`, `MockServerConnector`, `NoopServerBackend`), the compiler emits:
  ```text
  error[E0283]: type annotations needed for `T`
     --> opc-da-client\src\lib.rs:82:13
      |
   82 |         let builder = OpcDaClientBuilder::new()
      |             ^^^^^^^   ------------------------- type must be known at this point
      |
      = note: the type must implement `traits::ServerBackend`
  ```
* **Architectural Resolution:** Implementing `new()` specifically for `OpcDaClientBuilder<DefaultBackendConnector>` binds the constructor return type directly to the default backend. Generic backend construction is decoupled and served via:
  1. `OpcDaClientBuilder::<CustomConnector>::default()`
  2. `OpcDaClientBuilder::new_with_connector(connector)`
  3. `OpcDaClient::builder_with_connector(connector)`

This preserves clean ergonomics for 99% of consumers while providing full parametric flexibility for custom test doubles and alternative backends.

---

### 3.2 Case Study G1.2: Complete Deprecation Elimination
* **Context:** Prior to Block G1, the crate accumulated deprecation suppressions across `src/provider.rs`, `src/client/gateway.rs`, and `src/types/server.rs`.
* **Resolution:** Rather than merely excising the methods targeted in the plan (`write_tag_values`, `connect`, `connect_remote`), a comprehensive workspace ripgrep sweep was performed. Stale lint suppressions in `mod mock`, `OpcProvider` trait, and `types/server.rs` were eliminated, establishing an uncompromised zero-deprecation baseline for semver 0.3.0.

---

### 3.3 Case Study G2.1: Clippy Double-Must-Use Lint on Result Accessors
* **Context:** The plan specified adding `#[must_use]` to `TagValues::get_value_checked` to prevent silent discarding of extraction results.
* **Compiler Obstacle:** In Rust, `core::result::Result` is already marked `#[must_use]`. Clippy flags bare `#[must_use]` on functions returning `Result` with `clippy::double-must-use`, because discarding the `Result` is already warned on by rustc.
* **Resolution:** Clippy allows `#[must_use = "..."]` if a distinct, helpful message explains *why* ignoring the value is problematic. Supplying `#[must_use = "handling the Result distinguishes unrequested tags from server read failures"]` clarified the semantic rationale and eliminated the lint warning under `-D warnings`.

---

### 3.4 Case Study G2.2: Redundant `.into()` and Ambiguity with Generic `impl IntoWriteBatch`
* **Context:** In `tests/batch_write_test.rs:46`, the test invoked `client.write_tag_batch(server, writes.into()).await`.
* **Compiler Obstacle:** Once `write_tag_batch` was modernized to accept `impl IntoWriteBatch` directly instead of a concrete `WriteBatch`, passing `writes.into()` forced Rust's trait solver to resolve both `Into` and `IntoWriteBatch` concurrently. Under `-D warnings`, this triggered type inference ambiguity `E0282`.
* **Resolution:** Passing `writes` directly to `write_tag_batch(server, writes)` eliminated the redundant conversion and allowed the generic parameter bound to resolve cleanly.

---

### 3.5 Case Study G2.3: Forbidden Pattern Guard in Rustdoc Comments (Gate 7)
* **Context:** Documenting inherent methods on `Bound` and `Unbound` clients included illustrative code snippets printing retrieved tag values (e.g. `println!("{:?}", values)`).
* **Toolchain Obstacle:** Workspace Gate 7 executes `rg --color=never -n -g "*.rs" "\b(println!|dbg!|todo!)" opc-da-client/src/`. This scanner checks all `.rs` files indiscriminately, flagging `println!` even inside `///` documentation blocks.
* **Resolution:** All doc examples were updated to use standard assertions (`assert!(tv.quality.is_good())`, `assert_eq!(values.len(), 2)`) or explicit variable bindings (`let _values = ...`). This preserved clear, runnable documentation while ensuring 100% compliance with Gate 7.

---

### 3.6 Case Study G2.4: Generic Tuple String Invariants in `From<Vec<(S, V)>>`
* **Context:** In `src/types/write_batch.rs`, `From<Vec<(String, OpcValue)>>` was replaced with `From<Vec<(S, V)>>` generic over `S: Into<String>` and `V: Into<OpcValue>`.
* **Compiler Obstacle:** In existing unit test `test_write_batch_vec_and_into_iter`, the vector was instantiated as `vec![("Tag1".into(), OpcValue::Int(1))]`. Previously, Rust inferred that `.into()` targeted `String` because the concrete parameter was `Vec<(String, OpcValue)>`. With `Vec<(S, V)>`, `&str::into()` cannot deduce what intermediate type `S` should be before converting to `WriteBatch`, emitting type inference error `E0282`.
* **Resolution:** The literal was updated to `"Tag1".to_string()`, providing an unambiguous type declaration while keeping the test's intent and assertions intact.

---

### 3.7 Case Study H1.1: Blanket `TryFrom` Conflict and the Clean Slate Deletion of `From<S>` (`E0119`)
* **Context:** In `src/raw/memory.rs`, the original plan contemplated retaining `impl<S: AsRef<str>> From<S> for LocalPointer<Vec<u16>>` while adding `TryFrom<&str>` to reject interior null bytes (`\0`).
* **Compiler Obstacle:** In standard Rust core library, there is a blanket implementation:
  ```rust
  impl<T, U> TryFrom<U> for T where U: Into<T> {
      type Error = Infallible;
      fn try_from(value: U) -> Result<Self, Self::Error> {
          Ok(U::into(value))
      }
  }
  ```
  Because `LocalPointer<Vec<u16>>` implemented `From<S>` for `S: AsRef<str>`, every `&str` already implemented `Into<LocalPointer<Vec<u16>>>`. Consequently, the compiler automatically synthesized `impl TryFrom<&str> for LocalPointer<Vec<u16>>` with `type Error = Infallible`. Attempting to implement an explicit `impl TryFrom<&str> for LocalPointer<Vec<u16>>` with `type Error = OpcError` resulted in:
  ```text
  error[E0119]: conflicting implementations of trait `TryFrom<&str>` for type `raw::memory::LocalPointer<Vec<u16>>`
  ```
* **Architectural Resolution:** Rather than trying to preserve infallible string conversions that silently bypass null-byte validation, `impl From<S> for LocalPointer` was completely deleted. This eliminated the conflicting blanket implementation, allowed `TryFrom<&str>` (with `Error = OpcError`) to be implemented cleanly, and forced all 6 COM call sites in `com/connector/server.rs` and `com/security.rs` to explicitly handle fallible conversion via `LocalPointer::try_from_str(s)?`. This perfectly aligned with the Clean Slate security mandate for Block H1.

---

### 3.8 Case Study H1.2: Feature Independence in Typestate Constructors under Zero-Default Features (`Gate 4b`)
* **Context:** `OpcDaClient::bind_new` and `bind_new_remote` were updated to accept generic endpoints and server identifiers anchored on `impl OpcDaClient<DefaultBackendConnector, Unbound>`.
* **Toolchain Obstacle:** Workspace Gate 4b executes `cargo check -p opc-da-client --no-default-features` to verify that the crate builds cleanly on non-Windows platforms or headless environments where `opc-da-backend` is disabled. Under `--no-default-features`, `DefaultBackendConnector` defaults to `crate::connector::NoopServerBackend`. However, `ServerIdentifier` was previously imported in `client/mod.rs` with `#[cfg(feature = "opc-da-backend")]`. When `bind_new_remote` was compiled under `--no-default-features`, rustc failed with:
  ```text
  error[E0425]: cannot find type `ServerIdentifier` in this scope
     --> opc-da-client\src\client\mod.rs:129:30
      |
  129 |         server: impl TryInto<ServerIdentifier, Error: Into<OpcError>>,
      |                              ^^^^^^^^^^^^^^^^ not found in this scope
  ```
* **Architectural Resolution:** `ServerIdentifier` is a pure domain type defined in `crate::types::server`, which is completely agnostic of Win32 COM APIs and available unconditionally under all feature configurations. Removing the `#[cfg(feature = "opc-da-backend")]` gate on the `ServerIdentifier` import in `src/client/mod.rs` ensured `bind_new_remote` compiles cleanly regardless of whether `opc-da-backend` is active, achieving 100% compliance with Gate 4b.

---

### 3.9 Case Study H2.1: Clippy Pointer Casting Lints on COM Proxy Re-Borrowing
* **Context:** To avoid creating ephemeral COM proxy copies via `proxy.cast::<IUnknown>()`, `apply_proxy_blanket` was planned to re-borrow the underlying proxy pointer using `&*(proxy as *const T as *const windows::core::IUnknown)`.
* **Compiler Obstacle:** In Rust 2024 / MSRV 1.93.1 under `-D warnings`, clippy denies `clippy::ref_as_ptr` (casting `&T` using `as *const T`) and `clippy::ptr_as_ptr` (casting between raw pointer types using `as`).
* **Architectural Resolution:** Using standard library pointer methods:
  ```rust
  let unk: &windows::core::IUnknown = &*std::ptr::from_ref(proxy).cast::<windows::core::IUnknown>();
  ```
  satisfies both lints, preserves sound raw pointer re-borrowing without `QueryInterface` calls, and prevents dropping transient COM proxies under KB5004442 DCOM authentication requirements.

---

### 3.10 Case Study H2.2: AST-Grep Preceding Sibling Semantics on Multi-Line Safety Comments
* **Context:** Production unsafe blocks must be annotated with `// SAFETY:` rationale to satisfy the AST-Grep `require-safety-comment` structural rule (Gate 6).
* **Toolchain Obstacle:** Tree-sitter parses each line starting with `//` as a distinct `line_comment` syntax node. AST-Grep's `follows` selector checks the immediate preceding sibling token before the `unsafe_block` (or its enclosing `let_declaration`). When multi-line explanations were written with `// SAFETY:` only on the first line, the token immediately preceding the unsafe block was the final line (e.g. `// CoSetProxyBlanket is safe...`), causing the regex `SAFETY:` match to fail and triggering AST-Grep Gate 6 errors.
* **Architectural Resolution:** Standardizing multi-line safety comments so that *every* line begins with `// SAFETY:` ensures that whichever comment line is selected as the immediate preceding sibling satisfies the AST pattern match, guaranteeing reliable compliance across all AST structural tools.

---

### 3.11 Case Study H2.3: Elimination of Silent Cleanup Failures during Group Initialization Rollback
* **Context:** In `ComServer::add_group`, when converting the newly created group `IUnknown` pointer into `ComGroup` or applying member proxy blanketing, failure must trigger server-side `RemoveGroup` cleanup to avoid leaving orphaned server handles.
* **Architectural Obstacle:** The original code snippet proposed `let _ = unsafe { self.server.RemoveGroup(raw_server_handle, true) };`. This silently swallowed potential RPC or COM errors on group cleanup, violating `coding-standard.md §4.8` ("No silent failures"). Furthermore, inline assignment without dedicated binding separated the `// SAFETY:` comment from the `unsafe` block, triggering clippy's `undocumented_unsafe_blocks` lint.
* **Architectural Resolution:** Binding the removal result directly with an adjacent `// SAFETY:` comment and logging failures via `tracing::warn!(error = ?rm_err, handle = raw_server_handle, "Failed to remove orphaned group...")` provided full diagnostic observability while enforcing strict error transparency.

---

### 3.12 Case Study H3a.1: Doctest Type Invariants under Clean Slate Fallible Conversions
* **Context:** In `src/types/server.rs`, `OpcServerEndpoint::local` provides an ergonomic constructor for local server endpoints. When reattaching its rustdoc block, the example doctest was updated to demonstrate local endpoint construction.
* **Compiler Obstacle:** In Block H1, infallible `From<&str>` for `ServerIdentifier` was completely excised to uphold the Clean Slate validation mandate (preventing silent `unwrap_or_else` syntax swallowing into invalid `ProgId` variants). Writing `OpcServerEndpoint::local("Matrikon.OPC.Simulation.1")` fails compilation under Gate 3 (`cargo test --doc`) because string slices no longer implement `Into<ServerIdentifier>`.
* **Architectural Resolution:** The doctest was written to explicitly construct the identifier via `let id = ServerIdentifier::new("Matrikon.OPC.Simulation.1").unwrap();` before passing it to `OpcServerEndpoint::local(id)`. This accurately communicates to public API consumers that server identifiers require explicit fallible validation, ensuring 100% runnable doctest compliance.

---

### 3.13 Case Study H3a.2: Compilable RED-Phase Stubs for New Inherent Methods
* **Context:** Sub-Block H3a adopted strict TDD Red-Green discipline for introducing `ServerIdentifier::matches` and `OpcServerEndpoint::matches`.
* **Methodology Obstacle:** Adding a test case like `assert!(ep1.matches(&ep2))` when `.matches()` has not yet been declared on the struct causes rustc compiler error `E0599: no method named matches found for struct OpcServerEndpoint`. In strict TDD methodology, a compile error is NOT a valid RED state — RED requires the test suite to compile and link successfully, execute the target code path, and fail on a runtime assertion.
* **Architectural Resolution:** Before executing the test assertion, a minimal stub `pub fn matches(&self, _other: &Self) -> bool { false }` was introduced into the type definition. This allowed `cargo test` to compile cleanly, execute, and report an unambiguous assertion failure:
  ```text
  assertion `left == right` failed: ep1.matches(&ep2)
    left: false
   right: true
  ```
  Step 9 then replaced the stub with the full semantic comparison logic, transitioning the test to GREEN.

---

### 3.14 Case Study H3b.1: Anonymous Lifetime Instability in Trait Position (`E0658`)
* **Context:** In `src/com/worker/read.rs`, `assemble_tag_values` was upgraded to stream tag string slices via `tags: impl ExactSizeIterator<Item = &str>` directly from `TagBatch::iter_str()`, eliminating throwaway vector allocations and ensuring caller tag casing is preserved deterministically.
* **Compiler Obstacle:** In Rust 2024 (MSRV 1.93.1), eliding lifetime parameters on reference types nested within trait bounds in function argument position triggers compiler error `E0658`:
  ```text
  error[E0658]: anonymous lifetimes in `impl Trait` are unstable
     --> opc-da-client\src\com\worker\read.rs:216:42
      |
  216 |     tags: impl ExactSizeIterator<Item = &str>,
      |                                          ^ expected named lifetime parameter
      |
  help: consider introducing a named lifetime parameter
      |
  215 ~ pub(crate) fn assemble_tag_values<'a>(
  216 ~     tags: impl ExactSizeIterator<Item = &'a str>,
      |
  ```
* **Architectural Resolution:** Declaring an explicit named lifetime parameter `pub(crate) fn assemble_tag_values<'a>(tags: impl ExactSizeIterator<Item = &'a str>, ...)` resolved `E0658` cleanly on stable Rust. This maintains full zero-allocation streaming for both `TagBatchIter<'a>` and array iterator `[&'a str; N]::into_iter()` across tests and production code paths without requiring nightly compiler features.

---

### 3.15 Case Study H3b.2: Domain Type Conformance for `OpcValue::Int` in Test Fixtures (`E0308`)
* **Context:** In `src/com/worker/write.rs`, unit tests `test_handle_write_batch_exceeds_max_limit` and `test_handle_write_batch_at_max_limit` generate synthetic batches of size $N = 10{,}001$ and $N = 10{,}000$ to verify the `MAX_TAG_BATCH_SIZE` upper-bound admission guard.
* **Compiler Obstacle:** The test skeleton originally used `(format!("Tag.{i}"), OpcValue::Int(i as i32))`. In `opc-da-client`, `OpcValue::Int(i64)` stores a 64-bit integer (`src/types/value.rs:24`), not `i32` (`OpcValue` has no 32-bit integer variant; `VT_I4` COM values are widened to `i64`). Rust emitted:
  ```text
  error[E0308]: mismatched types
     --> opc-da-client\src\com\worker\write.rs:268:52
      |
  268 |                 (format!("Tag.{i}"), OpcValue::Int(i as i32))
      |                                      ------------- ^^^^^^^^ expected `i64`, found `i32`
  ```
* **Architectural Resolution:** Casting loop index `i` via `i as i64` accurately conforms to the domain type specification, allowing test fixtures to compile cleanly and assert boundary conditions without type mismatch diagnostics.

---

### 3.16 Case Study H3c.1: Linter Precision Guard via `std::f64::consts::PI` (`clippy::approx_constant`)
* **Context:** In `src/types/value.rs`, unit test `test_opc_value_from_ref` tests `From<&OpcValue> for OpcValue` across all domain variants, including `OpcValue::Float(3.14159)`.
* **Compiler Obstacle:** Under `-D warnings`, Clippy triggered `clippy::approx_constant`:
  ```text
  error: approximate value of `f{32, 64}::consts::PI` found
     --> opc-da-client\src\types\value.rs:795:29
      |
  795 |             OpcValue::Float(3.14159),
      |                             ^^^^^^^
      |
      = help: consider using the constant directly
  ```
* **Architectural Resolution:** Using `std::f64::consts::PI` eliminates the linter diagnostic under `-D warnings` while accurately testing float equality across clones.

---

### 3.17 Case Study H3c.2: Direct Fixed-Size Array Conversion for Zero-Intermediate-Allocation Arc Sharing
* **Context:** In `WriteBatch::into_shareable` (`src/types/write_batch.rs:186`), `Single(tag, val)` is converted into a shareable `WriteBatch::Shared(Arc<[(String, OpcValue)]>)` to allow $O(1)$ cloning across async worker channels.
* **Optimization Decision:** The initial baseline used `Arc::from(vec![(tag, val)].into_boxed_slice())`, which requires 2 heap allocations (allocating the `Vec`, then allocating the `Arc` header and buffer) and 1 heap deallocation (`into_boxed_slice()`). Rust standard library provides `impl<T, const N: usize> From<[T; N]> for Arc<[T]>`. Implementing `Self::Single(tag, val) => Self::Shared(Arc::from([(tag, val)]))` directly constructs the `Arc` slice from a 1-element fixed-size array in a single heap allocation with zero intermediate buffers.
* **Architectural Resolution:** Direct array construction reduces heap allocation overhead by 50% on single-write sharing, verified by `test_write_batch_into_shareable_lifecycle`.

---

### 3.18 Case Study I1.1: Mandatory Domain Struct Fields in Test Fixtures (`E0063`)
* **Context:** In Step 6 of Sub-Block I1, `tests/connection_pool_test.rs` was updated to test mock read error handling when `MockServerGroup::add_items` returns simulated item-level rejection errors.
* **Compiler Obstacle:** The plan's test snippet constructed `GroupItemResult` with only `server_handle` and `error`:
  ```rust
  GroupItemResult {
      server_handle: 0,
      error: Some(OpcError::Custom("Item rejected".into())),
  }
  ```
  However, in `src/connector/traits.rs:83`, `GroupItemResult` defines three mandatory fields:
  ```rust
  pub struct GroupItemResult {
      pub server_handle: u32,
      pub canonical_type: VarType,
      pub error: Option<OpcError>,
  }
  ```
  Rust compiler emitted `E0063`:
  ```text
  error[E0063]: missing field `canonical_type` in initializer of `GroupItemResult`
     --> tests/connection_pool_test.rs:245:21
      |
  245 |                     GroupItemResult {
      |                     ^^^^^^^^^^^^^^^ missing `canonical_type`
  ```
* **Architectural Resolution:** Test fixtures must strictly adhere to domain type invariants. Adding `canonical_type: VarType::EMPTY` satisfied struct construction while preserving the test's intent to verify item-level error extraction during group registration.

---

### 3.19 Case Study I1.2: Clippy Redundant Clone on Trait Mock Ownership Transfer (`clippy::redundant_clone`)
* **Context:** In Step 7 of Sub-Block I1, `test_connection_pool_server_down_eviction` was added to `tests/connection_pool_test.rs` to verify pool reconnection lifecycle behavior.
* **Compiler Obstacle:** The test initialized the mock connector using:
  ```rust
  let state = Arc::new(AtomicBool::new(true));
  let connector = Arc::new(MockServerConnector::with_state(state.clone()));
  ```
  Because `state` was never read, cloned, or referenced again within the scope of the test function, rustc/clippy under `-D warnings` triggered:
  ```text
  error: redundant clone
     --> tests/connection_pool_test.rs:188:73
      |
  188 |         let connector = Arc::new(MockServerConnector::with_state(state.clone()));
      |                                                                       ^^^^^^^^ help: remove this
  ```
* **Architectural Resolution:** Moving `state` directly into `MockServerConnector::with_state(state)` transferred ownership without an extraneous atomic reference increment, satisfying Clippy's zero-warning constraint.

---

### 3.20 Case Study I1.3: Nursery Lint `iter_with_drain` Conflict with In-Place Buffer Reuse
* **Context:** Plan Objective O1 and Step 10 mandated replacing `std::mem::replace(&mut chunk, Vec::with_capacity(BATCH_CHUNK_SIZE))` with `chunk.drain(..)` in `browse_flat_namespace` and `try_fast_flat_browse` (`src/com/worker/browse.rs`). The purpose was in-place vector buffer reuse: allocating a single 256-element vector once and streaming drained batches into `collector.push_batch(chunk.drain(..))`, eliminating ~39 intermediate heap allocations for large 10,000-tag namespaces.
* **Compiler Obstacle:** Under `-D warnings`, Clippy triggered nursery lint `clippy::iter_with_drain`:
  ```text
  error: `drain(..)` used on a `Vec` where `into_iter()` could be used
     --> opc-da-client/src/com/worker/browse.rs:114:41
      |
  114 |                 let _ = collector.push_batch(chunk.drain(..));
      |                                         ^^^^^^^^^^^^ help: use `into_iter()` instead
  ```
  However, following Clippy's recommendation to use `into_iter()` would consume and deallocate the vector buffer on every 256-tag chunk boundary. This would force re-allocating a new `Vec::with_capacity` on the subsequent iteration, directly defeating the performance objective of zero-allocation buffer reuse.
* **Architectural Resolution:** Applied scoped attribute `#[allow(clippy::iter_with_drain)]` to both `browse_flat_namespace` and `try_fast_flat_browse`. Documented inline rationale explaining that `drain(..)` is intentionally used to retain the backing capacity across loop iterations, preventing repeated heap re-allocations while satisfying `-D warnings`.

---

### 3.21 Case Study I1.4: In-Scope Trait Requirements for Associated Type Method Dispatch (`E0599`)
* **Context:** Step 12 Action 1 recommended: "Remove unused import `ConnectedGroup` from line 12" in `src/com/worker.rs`, assuming it was an unreferenced symbol.
* **Compiler Obstacle:** In `src/com/worker.rs:94`, the worker dispatches item registration against the server's active group:
  ```rust
  let item_results = group.add_items(&item_defs);
  ```
  Here, `group` has associated type `<S as ConnectedServer>::Group`. The method `add_items` is declared on trait `crate::connector::ConnectedGroup`. In Rust, invoking a trait method requires that trait to be in scope. Removing `use crate::connector::ConnectedGroup;` caused rustc to fail with:
  ```text
  error[E0599]: no method named `add_items` found for associated type `<S as traits::ConnectedServer>::Group` in the current scope
    --> opc-da-client/src/com/worker.rs:94:32
     |
  94 |         let item_results = group.add_items(&item_defs);
     |                                  ^^^^^^^^^ method not found in `<S as traits::ConnectedServer>::Group`
     |
     = help: items from traits can only be used if the trait is in scope
  ```
* **Architectural Resolution:** Retained `use crate::connector::ConnectedGroup;` in `src/com/worker.rs`. While the symbol name `ConnectedGroup` is not mentioned literally as a type annotation in the function body, its presence in scope is strictly required for dynamic trait method dispatch on associated types.

---

### 3.22 Case Study I2.1: Clippy Cast-Possible-Wrap on Win32 HRESULT Test Fixture Construction (`clippy::cast_possible_wrap`)
* **Context:** In Step 7 of Sub-Block I2, unit test `test_assemble_write_results_partial_failure` was added to `src/com/worker/write.rs` to verify result assembly when the COM server returns an `Err(OpcError::Com { ... })` for a specific item write.
* **Compiler Obstacle:** Initial plan snippets constructed the COM error using either an obsolete tuple variant `OpcError::Com(0x8000_4005)` or a raw high-bit cast:
  ```rust
  let server_write_results = Some(vec![
      Ok(()),
      Err(OpcError::Com {
          source: windows_core::Error::from_hresult(windows_core::HRESULT(0x8000_4005u32 as i32)),
      }),
  ]);
  ```
  This triggered two compiler/linter issues under `-D warnings`:
  1. `OpcError::Com` is a struct variant with field `source: windows_core::Error` rather than a tuple variant.
  2. In `windows_core`, `HRESULT` wraps signed `i32`. Casting high-bit Win32 constants like `0x8000_4005u32 as i32` triggers Clippy's `clippy::cast_possible_wrap` lint, which halts verification under `-D warnings`:
  ```text
  error: casting `u32` to `i32` may wrap around the value
     --> opc-da-client/src/com/worker/write.rs:732:73
      |
  732 |     source: windows_core::Error::from_hresult(windows_core::HRESULT(0x8000_4005u32 as i32))
      |                                                                     ^^^^^^^^^^^^^^^^^^^^^^ help: use: `0x8000_4005u32.cast_signed()`
  ```
* **Architectural Resolution:** Rather than using inline casts or silencing the lint, the test fixture was updated to reference the repository's canonical HRESULT constant `crate::errors::hresult::E_FAIL` (`HRESULT(0x8000_4005_u32.cast_signed())`). This conforms strictly to the project's single-source-of-truth error modeling, avoids raw integer casts, and passes all 9 quality verification gates cleanly with zero warnings.

---

### 3.23 Case Study I3.1: Mock Server Branch Traversal Isolation in Recursive Chunking Test
* **Context:** In Step 3 / Step 4 of Sub-Block I3, unit test `test_handle_browse_recursive_chunked_batch_push` was added to `src/com/worker/browse.rs` to verify that when hierarchical browsing walks leaves, it batches leaf items into `BROWSE_CHUNK_SIZE = 256` chunks via `chunk.drain(..)` rather than accumulating an unbounded single vector.
* **Mock Traversal Dynamic:** The test constructed a `MockConnectedServer::default()` with 300 generated leaves, disabled `supports_flat_browse` to force `browse_recursive`, and provided a collector with capacity 500. Under default mock initialization, `MockConnectedServer::default()` initializes `branch_tags` with two simulated branches: `vec!["Random".to_string(), "Simulation".to_string()]`.
* **Behavioral Obstacle:** During execution, `browse_recursive` at depth 0 successfully ingested the 300 leaf items (256 chunked + 44 residual). However, `browse_recursive` then proceeded to branch enumeration and recursed into simulated branch `"Random"`. Because `MockConnectedServer` returned the same 300 leaf tags within child branches, `browse_recursive` collected an additional 200 items before hitting the collector's capacity cap ($300 + 200 = 500$). The test assertion `assert_eq!(tags.len(), 300)` failed with `left: 500, right: 300`.
* **Architectural Resolution:** In accordance with `builder-rules.md §5.1` (Allowed Micro-Decisions on test setup) and Fidelity Hierarchy Priority 1 (Tests define correctness), the mock server setup was chained with `.with_branch_tags(Vec::new())`. This explicitly cleared simulated child branches, ensuring that `browse_recursive` focused strictly on depth-0 leaf chunking without unintended mock branch re-traversal. The test passed with exactly 300 tags, verifying both 256-chunking on the happy path and post-browse zero-allocation harvest semantics.

