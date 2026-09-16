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
* **Total Cycle 2 Deviations:** 7
* **Block G1 (Clean Slate API Excision & Struct Deduplication):** 3 deviations (0 violations, all justified and verified)
* **Block G2 (Ergonomic Symmetry & Comprehensive Public Documentation):** 4 deviations (0 violations, all justified and verified)
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


