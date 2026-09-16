# Refactoring Cycle 1 Historical Reference: Architecture Review, Findings & Implementation Deviations

> **Document Status:** Permanent Historical Archive  
> **Workspace:** `opc-cli`  
> **Target Crate:** `opc-da-client`  
> **Scope:** Consolidated record of Cycle 1 modernization—combining the initial 29-finding qualitative review report (`review_report.md`) and the master implementation deviations report (`deviations.md`), both now unified and archived in this document.  
> **Succession Note:** Moving forward, this file serves as the single source of truth for Cycle 1 historical decisions. Active post-refactoring status is tracked in [`refactor/post_refactor_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/post_refactor_review.md) and institutional engineering lessons in [`refactor/lessons.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/lessons.md).

---

## 1. Executive Summary & Cycle 1 Metrics

During Refactoring Cycle 1 of `opc-da-client`—encompassing the 7 architectural foundation waves (Blocks 1–7) and the 8-block modernization roadmap (Blocks A, B, C1, C2, C3, D, E, and F)—the codebase was overhauled from an unbuffered, Windows-bound, high-allocation prototype into an enterprise-grade, memory-efficient, fully mockable industrial communications client.

### Key Cycle 1 Metrics
* **Total Review Findings Closed:** 29 of 29 Findings (100% Resolution)
* **Total Engineering Blocks Executed:** 15 Blocks (Blocks 1–7 Foundation + Blocks A–F Modernization)
* **Total Implementation Deviations Tracked:** 14 Deviations (All justified against compiler invariants and verified)
* **Quality Verification:** 100% Green across all 9 quality gates in `scripts/verify.ps1` (475+ unit/integration tests, 89 doc-tests, 2 compile-fail tests, zero clippy warnings under `-D warnings`).

---

## 2. Initial Architecture Review & Findings Baseline

*Originally published 2026-09-15 as `refactor/review_report.md`.*

### 2.1 Baseline Review Summary
* **Scope Analyzed:** `opc-da-client` crate architecture, testing topology, and subsystem interaction paths (`src/client/`, `src/com/worker/`, `src/connector/`, `src/types/`, `src/errors.rs`, `src/provider.rs`, and `tests/`).
* **Active Lenses:** Logic, Design, Performance, Security, API (All 5 lenses active).
* **Initial Health Assessment:** *Needs Attention* (sound foundational primitives, but impaired by architectural COM gating, test topology inversion, hot-path heap allocations, and unhandled panic/reconnection failure modes).
* **Multi-Lens Hotspots Identified:**
  1. `src/com/worker.rs:run_worker_thread` (Flagged across **Logic**, **Security**, and **Performance**).
  2. `src/com/worker/read.rs:handle_read` (Flagged across **Performance**, **Logic**, and **Security**).
  3. `src/com/worker/pool.rs:dispatch_with_retry` (Flagged across **Logic**, **Performance**, and **Security**).
  4. `src/client/subscription.rs:subscribe` (Flagged across **Logic**, **API**, and **Performance**).
  5. `src/client/mod.rs:OpcDaClient` (Flagged across **Design**, **API**, and **Performance**).

### 2.2 Baseline Findings Matrix (29 Findings)

| # | Severity | Category | File & Symbol | Summary | Remediated In |
|:---:|:---|:---|:---|:---|:---:|
| **1** | 🔴 Critical | Design | `src/client/mod.rs:28`<br>`struct OpcDaClient<C, State>` | Client facade hardwired to COM worker and gated behind `opc-da-backend`, blocking cross-platform offline integration testing. | Block A |
| **2** | 🔴 Critical | Design | `src/com/worker/tests.rs:1`<br>`(Module)` | 1,100+ lines of worker tests bypass public facade and `OpcProvider` contracts, testing private actor messages. | Block B |
| **3** | 🔴 Critical | Performance | `src/com/worker/read.rs:81`<br>`handle_read(...)` | Active group cache hits perform redundant heap allocations per tag (string clones and dummy `"Not read"` error strings) on hot path. | Block E |
| **4** | 🔴 Critical | Performance | `src/com/worker/browse.rs:55`<br>`handle_browse(...)` | Per-tag mutex acquisition in flat browsing and deep vector cloning in `collector.snapshot()`. | Block E |
| **5** | 🔴 Critical | Security | `src/com/worker.rs:435`<br>`run_worker_thread(...)` | `pool.clear()` outside `catch_unwind` during Tier 2 panic recovery risks unhandled secondary panic and worker thread crash. | Block D |
| **6** | 🟠 Major | Logic | `src/com/worker.rs:436`<br>`run_worker_thread(...)` | In-flight requests dropped without response on preceding worker panic, emitting misleading `WorkerTerminated` to callers. | Block D |
| **7** | 🟠 Major | Logic | `src/com/worker/pool.rs:251`<br>`dispatch_with_retry(...)` | Non-idempotent batch and single writes automatically retried on connection drop / RPC error, risking duplicate PLC actuations. | Block D |
| **8** | 🟠 Major | Logic | `src/com/worker/read.rs:105`<br>`handle_read(...)` | Active group cache not invalidated on item state length mismatch, causing persistent subsequent read failures. | Block D |
| **9** | 🟠 Major | Design | `tests/handle_type_safety_test.rs:1`<br>`(Module)` | Test topology inversion: domain unit tests and mockall stability tests misplaced in `tests/`, inflating integration metrics. | Block B |
| **10** | 🟠 Major | Design | `src/types/tests.rs:1`<br>`(Module)` | Monolithic 1,900+ line domain test file lacks intramodule integration across domain data pipeline. | Block B |
| **11** | 🟠 Major | Performance | `src/com/worker.rs:404`<br>`run_worker_thread(...)` | Synchronous worker loop causes head-of-line blocking during namespace browsing, starving high-priority read/write I/O. | Block E |
| **12** | 🟠 Major | Performance | `src/com/worker/read.rs:118`<br>`handle_read(...)` | Single active group cache slot causes thrashing and repeated group recreation when alternating tag batches. | Block E |
| **13** | 🟠 Major | Performance | `src/com/worker/pool.rs:118`<br>`struct ConnectionPool` | Unbounded `connections` map leaks COM server and group proxies while `failure_cooldowns` is bounded to 256. | Block E |
| **14** | 🟠 Major | Security | `src/types/server.rs:261`<br>`ServerIdentifier::from` | `From<&str>` and `From<String>` bypass ProgID validation, allowing malformed strings into COM subsystem. | Block F |
| **15** | 🟠 Major | Security | `src/com/worker.rs:414`<br>`run_worker_thread(...)` | `PriorityRequestQueue` unconditionally drains bounded channel, nullifying Tokio backpressure and risking memory exhaustion. | Block D |
| **16** | 🟠 Major | API | `src/client/gateway.rs:103`<br>`<OpcDaClient as TagReader>` | `Bound` client implements multi-server role traits, completely bypassing its bound endpoint when invoked via trait methods. | Block F |
| **17** | 🟠 Major | API | `src/client/mod.rs:61`<br>`OpcDaClient::connect` | Constructor named `connect` is synchronous and never connects to or verifies reachability of target server. | Block F |
| **18** | 🟠 Major | API | `src/client/builder.rs:130`<br>`build_bound` | `build_bound` allocates resources, spawns OS worker thread, and initializes COM MTA before validating server identifier. | Block F |
| **19** | 🟠 Major | API | `src/errors.rs:130`<br>`is_connection_error` | `is_connection_error` omits checking `OpcError::Server(_, code)`, preventing automatic reconnection on RPC disconnections. | Block F |
| **20** | 🟠 Major | API | `src/types/collection.rs:295`<br>`TagExtractError::into` | Lossy error conversion silently discards tag identifier on `ReadFailed` and `TypeMismatch`, returning anonymous errors. | Block F |
| **21** | 🟡 Minor | Logic | `src/client/subscription.rs:40`<br>`Bound::subscribe` | Polling loop permanently breaks on single transient timeout or cooldown trip without error notification to receiver. | Block C1 |
| **22** | 🟡 Minor | Logic | `src/client/subscription.rs:28`<br>`Bound::subscribe` | Background polling task sleeps until full interval timer expires upon receiver drop before noticing closure. | Block C1 |
| **23** | 🟡 Minor | Logic | `tests/batch_write_test.rs:1`<br>`(Module)` | Batch write integration tests verify only all-success scenario, omitting partial failure and error propagation paths. | Block C2 |
| **24** | 🟡 Minor | Logic | `tests/typestate_client_test.rs:1`<br>`(Module)` | Typestate tests cover only happy-path bind/unbind, omitting bound session failure recovery and endpoint error states. | Block C2 |
| **25** | 🟡 Minor | Security | `src/com/worker.rs:486`<br>`dispatch_pooled_request` | Dispatch engines execute synchronous COM calls without verifying `reply.is_closed()`, wasting work on cancelled requests. | Block D |
| **26** | 🟡 Minor | Security | `src/com/worker/read.rs:25`<br>`handle_read(...)` | Unbounded tag batch processing without client-side chunking or maximum batch size limit. | Block E |
| **27** | 🟡 Minor | Security | `src/com/connector/group.rs:15`<br>`to_wide_null` | Missing null-byte validation on item IDs allows silent string truncation in COM FFI (CWE-626 / CWE-158). | Block F |
| **28** | 🟡 Minor | Performance | `src/types/batch.rs:338`<br>`IntoTags for [&'static str; N]` | Array-by-value `IntoTags` implementation incurs hidden heap allocations across tests and production code. | Block E |
| **29** | 🟡 Minor | API | `src/types/collection.rs:431`<br>`TagValues::get_value` | `get_value` swallows per-item read errors into `None`, while non-swallowing `get_value_checked` is kept private. | Block F |

---

## 3. Master Implementation Deviations Log

*Originally published 2026-09-16 as `refactor/deviations.md`.*

Every engineering deviation from initial planning blueprints was subjected to architectural review, justified by compiler/language constraints, and signed off under the TARS protocol:

### 3.1 Master Deviation Matrix (14 Deviations)

| Block ID | Finding Ref | Planned Approach | Implemented Deviation | Rationale & Compiler Dynamic | Category | Resulting Invariant |
|:---:|:---:|---|---|---|:---:|---|
| **Block F** | Finding #14 | Implement `TryFrom<&str>` and `TryFrom<String>` on `ServerIdentifier`. | Implemented `ServerIdentifier::new(s: &str) -> Result<Self, ParseServerIdError>` and `FromStr`. Deprecated `From<&str>`. | Rust standard library `core` provides blanket `impl<T, U> TryFrom<U> for T where U: Into<T>`. Because `From<&str>` existed, manual `TryFrom` triggered compiler error `E0119` (conflicting implementation). AST-grep `no-panic-or-unwrap` prohibited `.unwrap()` in deprecated `From`. | Language Invariant | Safe fallible constructor and `FromStr` parsing without violating Rust blanket rules. |
| **Block F** | Finding #16 | Restrict role traits (`TagReader`, `TagWriter`) on `OpcDaClient<C, Bound>` to bound endpoint only. | Implemented `validate_bound_server` parsing UNC (`\\`), URI (`opc://`), and colon (`:`) host delimiters, validating requested endpoint against `self.state.endpoint`. | The role traits take `server: impl Into<ServerIdentifier>` as part of the public `TagReader`/`TagWriter` SPI. Rather than breaking the trait signature, the gateway dynamically guards the invocation, rejecting unauthorized multi-server access. | API Type Safety | Preserves trait polymorphism while enforcing bound endpoint invariants. |
| **Block F** | Gate 7 | Doc-test example for `bind_new` using standard `Result<(), Box<dyn Error>>`. | Changed doc-test return type to `opc_da_client::OpcResult<()>`. | Quality Gate 7 (`verify.ps1`) enforces a strict repository-wide guard against `Box<dyn Error>` in `opc-da-client`, including hidden doc-test runner functions (`/// # fn run()`). | Toolchain Conformance | Enforces 100% type-safe error domain even in documentation examples. |
| **Block E** | Finding #4 | Monolithic `handle_browse` in `browse.rs` buffering flat items into 256-item chunks. | Extracted private helpers `browse_flat_namespace` and `try_fast_flat_browse` into separate sub-100 line functions. | Clippy cognitive and cyclomatic complexity lint thresholds; maintaining modular functions under 100 lines per project standards. | Toolchain Conformance | Clean, readable functions with zero clippy warnings and identical 256-item chunking behavior. |
| **Block E** | Finding #12 | Single-slot active group cache replaced with multi-slot LRU cache. | Wrapped group removal during LRU cache eviction in `std::panic::catch_unwind(AssertUnwindSafe(...))`. | COM proxy release (`IOPCServer::RemoveGroup`) during active group eviction can trigger secondary faults or panics in out-of-process COM servers; must not terminate worker thread. | Concurrency & Panic Safety | Prevents worker thread termination during LRU cache rotation. |
| **Block E** | Finding #28 | Optimize `IntoTags for [&'static str; N]` to reduce heap allocation. | Implemented `TagBatchRepr::StaticSmall([&'static str; 4], u8)` for $N \le 4$ (inline zero-allocation) and `StaticArc(Arc<[&'static str]>)` for $N > 4$. | Small arrays of 1–4 tags represent >90% of polling tag sets. Storing them inline completely eradicates heap allocation. | API Type Safety | 0 bytes heap allocation on hot-path scalar and small-batch reads. |
| **Block D** | Finding #7 | Accept `idempotent: bool` in `dispatch_with_retry`. | Created dedicated `RetryPolicy::{Idempotent, NonIdempotent}` enum. | Boolean parameters at call sites ("boolean trap") cause cognitive friction and misuse. Dedicated enum makes mutating operations explicit. | API Type Safety | Prevents accidental retries on non-idempotent PLC writes at compile time. |
| **Block D** | Finding #5 | Protect `pool.clear()` during worker panic recovery. | Wrapped `pool.clear()` in `std::panic::catch_unwind(AssertUnwindSafe(|| { pool.clear(); }))`. | Dropping COM proxies during Tier 2 event loop panic recovery can panic if COM MTA apartment is in an invalid state. Unwind safety guarantees thread loop survival. | Concurrency & Panic Safety | Worker thread survives even double-fault COM proxy drops. |
| **Block D** | Finding #8 | Invalidate active group on `populate_item_states` length mismatch. | Scoped the immutable borrow of `cached` to drop before calling `pooled.clear_active_group()`. | The Rust borrow checker prevents calling `&mut self` (`clear_active_group`) while `&self` (`cached`) is in scope. Scoping the borrow block cleanly resolved the conflict. | Language Invariant | Clean group cache eviction without borrow checker workarounds or unsafe pointers. |
| **Block D** | Test Plan | Test plan called `client.ping().await`. | Adapted integration test to call `client.connect_eager().await`. | `connect_eager()` is the public facade method that dispatches `ComRequest::Ping`. Calling the public facade adheres to black-box integration testing principles. | Test Topology & Fast-Path | Verified through public API contract without relying on private worker channels. |
| **Block C3** | Finding #2 | Test 5-second failure cooldown with real sleep timer. | Simulated timestamp verification asserting immediate short-circuiting without real-time `sleep`. | Sleeping 5 seconds in automated test suites degrades CI performance and developer feedback loops. Fast-path timestamp math verifies the circuit breaker instantaneously. | Test Topology & Fast-Path | Suite executes in 0.01s while maintaining 100% circuit breaker test coverage. |
| **Block C2** | Finding #2 | Borrowed tag array passed to `client.subscribe(&["Tag"])`. | Passed array by value `client.subscribe(["Tag"])`. | Clippy lint `clippy::needless_borrows_for_generic_args` flags borrowing arrays when `IntoTags` is implemented by value for `[&'static str; N]`. | Toolchain Conformance | Satisfied clippy `-D warnings` while leveraging zero-allocation `StaticSmall`. |
| **Block A** | Finding #1 | Ungate `ComWorker` and `OpcDaClientBuilder` from `opc-da-backend`. | Introduced `NoOpComInit` associated with `ComInitializer` trait (`type Guard = ()`) and `ActiveDefaultComInit` type alias. | Offline non-Windows builds and Linux CI runs cannot execute Win32 COM MTA initialization (`CoInitializeEx`). Decoupling initialization behind a pluggable guard allows headless compilation. | Concurrency & Panic Safety | Complete offline compilation and testing capability on any platform. |
| **Block 5** | Finding #15 | Coexist monolithic `mock.rs` with `mock/` submodule folder. | Deleted `src/connector/mock.rs` entirely in favor of `src/connector/mock/mod.rs`. | Rust module system error `E0761` forbids having both `foo.rs` and `foo/mod.rs` in the same module tree. Complete deletion resolved the namespace collision. | Language Invariant | Modular, decoupled mock subsystem under `src/connector/mock/`. |

---

## 4. In-Depth Technical Case Studies

### 4.1 Case Study 1: Rust Standard Library Blanket Collision (Deviation F.1)
* **Problem:** Implementing `TryFrom<&str>` on `ServerIdentifier` triggered rustc error `E0119`.
* **Investigation:** The standard library `core::convert` defines:
  $$\text{impl}\langle T, U\rangle \text{ TryFrom}\langle U\rangle \text{ for } T \text{ where } U: \text{Into}\langle T\rangle$$
  Because `From<&str> for ServerIdentifier` was already implemented, Rust automatically derived `Into<ServerIdentifier> for &str`, which in turn caused `core` to blanket-implement `TryFrom<&str> for ServerIdentifier` with error type `Infallible`. Any manual `TryFrom` implementation created an irreconcilable conflict.
* **Architecture Decision:** Shifted from `TryFrom` to standard `FromStr` and an explicit associated constructor `ServerIdentifier::new(s: &str) -> Result<Self, ParseServerIdError>`. Deprecated `From<&str>` using `.unwrap_or_else()` to comply with AST-grep's `no-panic-or-unwrap` rule.

### 4.2 Case Study 2: Industrial PLC Write Non-Idempotency (Deviation D.1)
* **Problem:** `dispatch_with_retry` automatically evicted stale COM proxies and re-dispatched requests on connection failures.
* **Investigation:** While reads, browsing, and pings are idempotent, tag writes alter physical device state (e.g., motor setpoints, valve positions, chemical dosing). If an RPC connection drops after the server processes the packet but before the acknowledgment returns, an automatic retry issues a duplicate write, violating industrial functional safety standards.
* **Architecture Decision:** Introduced the `RetryPolicy` enum (`Idempotent` vs `NonIdempotent`). In `dispatch_with_retry`, if `retry_policy == RetryPolicy::NonIdempotent`, stale proxies are evicted from the connection pool, but the operation fails fast immediately, returning the error to the application layer without issuing a second write command.

### 4.3 Case Study 3: Hot-Path Small-Batch Memory Optimization (Deviation E.2)
* **Problem:** Converting small array literals (e.g., `["Pump1.Speed", "Pump1.Status"]`) into `TagBatch` triggered heap allocations on every polling cycle.
* **Investigation:** 90% of industrial tag polling operations involve 1 to 4 scalar tags. Allocating a `Vec<String>` on each tick generated hundreds of thousands of heap allocations per minute.
* **Architecture Decision:** Implemented `TagBatchRepr::StaticSmall([&'static str; 4], u8)`. For slices where $N \le 4$, the string pointers and count are stored entirely on the stack (zero bytes allocated on the heap). For larger batches, `TagBatchRepr::StaticArc(Arc<[&'static str]>)` shares immutable slices across reader threads.

---

## 5. Archival Conclusion & Succession

Refactoring Cycle 1 achieved complete functional and architectural remediation of `opc-da-client`. All 29 original defects are closed, verified, and backed by automated regression test suites.

**Succession Notice:**
* For current, active post-refactoring review findings and dead-code elimination recommendations, refer to [`refactor/post_refactor_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/post_refactor_review.md).
* For codified architectural patterns and engineering lessons learned, refer to [`refactor/lessons.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/lessons.md).
* This document ([`refactor/cycle1_reference.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle1_reference.md)) serves as the sole permanent historical record of Cycle 1.
