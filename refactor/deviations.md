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
* **Total Cycle 2 Deviations:** 16
* **Block G1 (Clean Slate API Excision & Struct Deduplication):** 3 deviations (0 violations, all justified and verified)
* **Block G2 (Ergonomic Symmetry & Comprehensive Public Documentation):** 4 deviations (0 violations, all justified and verified)
* **Block H1 (Domain Invariants & CWE-626 Hardening):** 2 deviations (0 violations, all justified and verified)
* **Block H2 (COM Resource & Dead Code Pruning):** 3 deviations (0 violations, all justified and verified)
* **Sub-Block H3a (Server Identity & Host Canonicalization):** 2 deviations (0 violations, all justified and verified)
* **Sub-Block H3b (Worker Active Group Caching & Batch Defense):** 2 deviations (0 violations, all justified and verified)
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
