---
title: "Architectural Lessons Learned: opc-da-client Modernization & ICS Systems"
description: "Master catalog of 44 architectural patterns, compiler invariants (Rust 2024 / MSRV 1.93.1), concurrency dynamics, zero-allocation memory optimizations, industrial control safety invariants, Windows COM/DCOM FFI caveats, and quality gate governance."
category: "development"
tags:
  - rust
  - opc-da
  - industrial-automation
  - concurrency
  - memory-safety
  - zero-allocation
  - windows-com
  - dcom
  - clippy
  - testing
  - ast-grep
date: "2026-09-18"
version: "Cycle 2"
msrv: "1.93.1"
total_lessons: 44
status: "Certified & Verified against Rust 2024 (MSRV 1.93.1), Windows COM/DCOM, and Tokio Runtime"
---

# Architectural Lessons Learned: `opc-da-client` Modernization & ICS Systems

**Scope:** Master reference catalog of 44 architectural patterns, compiler dynamics, concurrency rules, industrial control safety invariants, and Windows COM/FFI caveats discovered during the modernization of `opc-da-client`.  
**Target Destination:** Prepared for indexing into `knowledge-rag` MCP (`development` category).  
**Author:** Architect  
**Date:** 2026-09-18 (Cycle 2 Modernization Archive)  
**Status:** Certified & Verified against Rust 2024 (MSRV 1.93.1), Windows COM/DCOM, and Tokio Runtime. All 9 Quality Gates passing (629 tests, 0 warnings).

---

## Master Diagnostic, Error Code & Quick Lookup Index

| Diagnostic / Keyword | Domain | Section | Core Rule / Finding Summary |
|:---|:---|:---|:---|
| `E0119` (Coherence) | Rust Traits | [§1.1](#11-the-blanket-tryfrom-vs-from-orphan-collision) | `From<&str>` blanket-implements `TryFrom<&str>`; use inherent `.new()` + `FromStr`. |
| Boolean Trap | API Design | [§1.2](#12-eliminating-the-boolean-trap-with-domain-enums) | Replace boolean parameters with explicit two-variant domain enums (`RetryPolicy`). |
| Typestate Invariant | Domain Model | [§1.3](#13-compile-time-typestate-encapsulation--role-trait-boundaries) | Bound facade role traits must validate requested server against session endpoint. |
| AFIT | Async Trait | [§1.4](#14-native-rust-2024-afit-async-fn-in-trait) | Native Rust 2024 async traits eliminate `#[async_trait]` heap boxing allocations. |
| `E0283` (Default Generic) | Type Solver | [§1.5](#15-associated-function-type-inference-ambiguity-e0283-with-default-generic-parameters) | Default generic parameters on structs are ignored by associated functions in expression position. |
| `E0283` (Intermediate) | Type Solver | [§1.6](#16-intermediate-type-inference-collisions-on-generic-collection-conversions-e0283) | Trait solver cannot resolve intermediate type in chained `.into()` on string literals. |
| `E0658` (Anonymous Lifetime) | Lifetime Rules | [§1.7](#17-rust-2024-anonymous-lifetimes-in-associated-type-bounds-e0658) | Anonymous lifetimes in `impl Trait` associated type bounds are unstable in Rust 2024. |
| Representation Equality | Equivalence | [§1.8](#18-semantic-sequence-equality-vs-derived-representation-equality) | Collections with multiple storage representations require manual element-wise `PartialEq`. |
| Object Safety | SPI Architecture | [§1.9](#19-trait-object-safety-preservation-across-facade-and-role-layers) | Keep SPI traits strictly concrete; put generic ergonomic wrappers on inherent facades. |
| Channel Backpressure | Concurrency | [§2.1](#21-preserving-channel-backpressure-across-asyncsync-boundaries) | Unbounded `try_recv()` loop destroys Tokio backpressure; cap queue depth at `MAX_QUEUE_DEPTH`. |
| Panic Provenance | Concurrency | [§2.2](#22-transparent-panic-recovery-with-explicit-in-flight-request-drainage) | Worker thread panic must drain queue rejecting awaiting callers with `WorkerError::Panic`. |
| Cancellation Check | Performance | [§2.3](#23-early-cancellation-check-avoiding-wasted-ffi--rpc) | Check `reply.is_closed()` before dispatching synchronous COM RPC round-trips. |
| Mutex Contention | Concurrency | [§2.4](#24-amortizing-mutex-contention-via-chunked-accumulation) | Buffer leaf nodes in local chunk and push in 256-item batches to cut lock contention by 99.6%. |
| `RwLock` Decoupling | Concurrency | [§2.5](#25-reader-writer-concurrency-decoupling-for-streaming-accumulators-rwlock-vs-mutex) | Use `RwLock` + atomics to allow concurrent observer reads while worker streams writes. |
| Poison Recovery | Lock Safety | [§2.6](#26-symmetrical-poison-recovery-across-lock-acquisition-sites) | Use `match guard { Ok(g) => g, Err(p) => p.into_inner() }` symmetrically across all lock sites. |
| `PushBatchGuard` | Unwind Safety | [§2.7](#27-raii-unwind-safety-in-streaming-iterators-pushbatchguard) | RAII drop guard resynchronizes atomic counters if third-party iterator panics midway. |
| Cold Panic Allocation | Hot Path | [§2.8](#28-zero-allocation-hot-path-request-dispatch-across-catch_unwind) | Borrow endpoint via `AssertUnwindSafe`; allocate error string strictly in cold panic branch. |
| `StaticSmall` | Zero Allocation | [§3.1](#31-static-small-array-optimization-staticsmall) | Inline array `[&'static str; 4]` for $N \le 4$ avoids heap allocation on polling loops. |
| UTF-8 Boundary | Memory Safety | [§3.2](#32-utf-8-multibyte-slicing-safety-in-inline-sso-buffers) | Use `Utf8Error::valid_up_to()` when slicing fixed-size inline byte buffers. |
| Double-Panic Abort | Unwind Safety | [§3.3](#33-double-panic-containment-in-raii-drops) | Wrap COM cleanup inside `Drop` in `std::panic::catch_unwind` to prevent process abort. |
| Sentinel Allocations | Hot Path | [§3.4](#34-eliminating-sentinel-allocations-on-hot-paths) | Pre-allocate exact vector capacity; avoid initializing vectors with placeholder error strings. |
| `chunk.drain(..)` | Buffer Reuse | [§3.5](#35-in-place-vector-buffer-draining-chunkdrain-vs-reallocation) | Reuse physical vector memory via `chunk.drain(..)` instead of replacing with throwaway vectors. |
| Lazy Slot Buffer | Allocator Optimization | [§3.6](#36-lazy-slot-allocation-buffers-vs-eager-dummy-records) | Pre-allocate `Vec<Option<T>>` with `None` slots; populate lazily to eliminate dead store strings. |
| 31-Byte SSO | Cache Layout | [§3.7](#37-31-byte-stack-small-string-optimization-sso-with-zero-padding-72-byte-layout) | Inline 31-byte stack buffer + tag value fits into 72 bytes with zero internal padding. |
| `std::mem::take` | Move Semantics | [§3.8](#38-zero-copy-terminal-data-handoff-via-stdmemtake) | Terminal `harvest()` swaps accumulator vector in $O(1)$ time with 0 allocations. |
| Non-Idempotent Write | ICS Safety | [§4.1](#41-write-non-idempotency--duplicate-actuation-hazards) | Writes must fail fast on connection drop; never auto-retry mutating PLC commands. |
| Group Poisoning | State Machine | [§4.2](#42-active-group-poisoning-on-array-length-mismatch) | Evict and destroy cached COM group on server state length mismatch before returning error. |
| Precondition Guard | Resource Safety | [§4.3](#43-precondition-validation-prior-to-os-thread--apartment-allocation) | Validate builder configuration parameters prior to allocating OS thread and COM MTA. |
| CWE-626 / CWE-400 | ICS Availability | [§4.4](#44-granular-industrial-input-quarantine-vs-cascading-batch-abort-cwe-626--cwe-400) | Quarantine invalid tag into result vector; do not abort valid peer commands in batch. |
| Positional Attribution | Data Integrity | [§4.5](#45-two-stage-positional-index-mapping-under-granular-defensive-screening) | Maintain two-stage index vectors with `.zip()` to map results back to original batch positions. |
| Clean Slate Browse | State Machine | [§4.6](#46-reconnection-retry-accumulator-clean-slate-on-idempotent-browse) | Clear accumulator on browse retry entry to prevent duplicate items and premature capacity limit. |
| CWE-626 Null Byte | FFI Security | [§5.1](#51-cwe-626-null-byte-injection-in-com-wide-strings) | Reject strings containing interior `\0` before converting to Win32 wide string pointers. |
| RPC Disconnect | Error Mapping | [§5.2](#52-cryptic-win32-rpc-disconnect-classification) | Bitwise classification of `0x800706BA` and `0x800706BE` across both `Com` and `Server` error types. |
| `NoOpComInit` | Portability | [§5.3](#53-decoupling-windows-com-via-pluggable-initializers-noopcominit) | Pure-Rust `ComInitializer` SPI trait allows running tests on Linux and headless CI. |
| Pure-Rust CLSID | Type Safety | [§5.4](#54-pure-rust-128-bit-clsid-domain-type) | 128-bit RFC-4122 `Clsid([u8; 16])` decouples domain layers from `windows::core::GUID`. |
| KB5004442 / Blanket | Windows DCOM | [§5.5](#55-direct-proxy-blanketing-vs-queryinterface-ephemeral-allocation-windows-kb5004442) | Re-borrow concrete proxy pointer as `&IUnknown`; avoid calling `.cast()` / `QueryInterface`. |
| ISP on COM Graphs | Network Efficiency | [§5.6](#56-interface-segregation-principle-isp-on-com-interface-graphs) | Retain only `IOPCServer`, `IOPCItemMgt`, `IOPCSyncIO`; saves 12 redundant DCOM round-trips. |
| `cast_possible_wrap` | Static Analysis | [§5.7](#57-canonical-win32-hresult-modeling-vs-raw-integer-casts-clippycast_possible_wrap) | Construct signed `HRESULT` using `0x8000_4005_u32.cast_signed()` to satisfy Clippy. |
| Sub-Second Cooldown | Test Speed | [§6.1](#61-sub-second-failure-cooldown-verification-no-sleep) | Test circuit breakers instantaneously via back-to-back calls; eliminate real-time `sleep`. |
| `Box<dyn Error>` Guard | Standards | [§6.2](#62-strict-elimination-of-boxdyn-error-and-anyhow-in-libraries) | Enforce strongly-typed `thiserror` variants; CI scanner rejects dynamic errors in libraries. |
| Co-located Tests | Architecture | [§6.3](#63-test-topology-normalization-co-located-unit-tests-vs-integrated-suites) | Unit tests reside in `mod tests` within source files; `tests/` reserved for multi-subsystem suites. |
| PowerShell 5.1 | Portability | [§6.4](#64-multi-shell-powershell-automation-portability-join-path-positional-constraints) | Standardize `Join-Path` on 2-parameter relative paths for Windows PowerShell 5.1 compatibility. |
| AST-Grep Multi-Line | Static Analysis | [§6.5](#65-ast-grep-multi-line-comment-sibling-token-matching) | Repeat `// SAFETY:` prefix on every comment line to satisfy AST-Grep sibling node matching. |
| Mock Isolation | Mock Fidelity | [§6.6](#66-mock-server-state-isolation-in-recursive-tree-walks) | Reset simulated child branches (`.with_branch_tags(Vec::new())`) in leaf traversal tests. |

---

## 1. Rust Type System & Trait Invariants

### 1.1 The Blanket `TryFrom` vs `From` Orphan Collision
- **Keywords / Search Tokens:** `E0119`, `TryFrom`, `From`, `core::convert`, `coherence`, `orphan rule`, `conflicting implementations`, `ServerIdentifier`
- **Category:** Rust Language Invariants & Trait Coherence
- **Problem Statement:** Attempting to implement `impl TryFrom<&str> for MyType` when `impl From<&str> for MyType` already exists causes compiler error `E0119` (conflicting implementations of trait `TryFrom`).
- **Root Cause & Technical Dynamics:** The Rust standard library (`core::convert`) contains the blanket implementation:
  ```rust
  impl<T, U> TryFrom<U> for T where U: Into<T> {
      type Error = Infallible;
      fn try_from(value: U) -> Result<Self, Self::Error> {
          Ok(U::into(value))
      }
  }
  ```
  Because `From<&str>` automatically implies `Into<MyType>`, `TryFrom<&str>` is already blanket-implemented by `core`. Manual implementation is forbidden by coherence rules.
- **Anti-Pattern (What NOT to do):**
  ```rust
  // FAILS TO COMPILE (E0119) if `From<&str>` exists:
  impl TryFrom<&str> for ServerIdentifier {
      type Error = ParseServerIdError;
      fn try_from(s: &str) -> Result<Self, Self::Error> { ... }
  }
  ```
- **Architectural Solution (What to DO):**
  1. Provide a dedicated fallible inherent constructor: `pub fn new(s: &str) -> Result<Self, ParseError>`.
  2. Implement `FromStr for MyType` delegating to `MyType::new`.
  3. Deprecate infallible `From` implementations, delegating to `s.parse().unwrap_or_else(...)` to satisfy zero-panic linters while preserving backward compatibility.
  ```rust
  impl ServerIdentifier {
      pub fn new(s: &str) -> Result<Self, ParseServerIdError> {
          // Validation logic here...
      }
  }

  impl FromStr for ServerIdentifier {
      type Err = ParseServerIdError;
      fn from_str(s: &str) -> Result<Self, Self::Err> {
          Self::new(s)
      }
  }

  #[deprecated(since = "0.2.1", note = "Use `ServerIdentifier::new` or `.parse::<ServerIdentifier>()`")]
  impl From<&str> for ServerIdentifier {
      fn from(s: &str) -> Self {
          s.parse().unwrap_or_else(|_| Self::ProgId(s.to_string()))
      }
  }
  ```
- **Enforcement & Invariant:** Verified under `cargo check` on MSRV 1.93.1. Zero-panic linter validates fallback branch.

---

### 1.2 Eliminating the "Boolean Trap" with Domain Enums
- **Keywords / Search Tokens:** `Boolean trap`, `RetryPolicy`, `Idempotent`, `NonIdempotent`, `API ergonomics`, `code review`, `call-site clarity`
- **Category:** API Ergonomics & Call-Site Clarity
- **Problem Statement:** Functions accepting multiple boolean flags (e.g. `dispatch_with_retry(pool, connector, ep, true, op)`) suffer from the "boolean trap"—callers cannot understand what `true` signifies without inspecting function signatures, and inverted boolean flags easily slip past code review.
- **Root Cause & Technical Dynamics:** Primitive booleans convey only 1 bit of information without semantic context. Call sites become opaque and error-prone when multiple flags are passed.
- **Anti-Pattern (What NOT to do):**
  ```rust
  // Ambiguous call site: Does `true` mean retry? Is it idempotent? Is it recursive?
  dispatch_with_retry(&mut pool, &connector, &endpoint, true, |_| Ok(()));
  ```
- **Architectural Solution (What to DO):** Replace primitive boolean parameters with dedicated two-variant domain enums:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub(crate) enum RetryPolicy {
      Idempotent,
      NonIdempotent,
  }

  // Self-documenting call site:
  dispatch_with_retry(&mut pool, &connector, &endpoint, RetryPolicy::NonIdempotent, |_| {
      // Mutating PLC write...
  })?;
  ```
- **Enforcement & Invariant:** Mandated by API review checklist; enforced in `opc-da-client/src/client/resilience.rs`.

---

### 1.3 Compile-Time Typestate Encapsulation & Role Trait Boundaries
- **Keywords / Search Tokens:** `Typestate pattern`, `Bound`, `Unbound`, `TagReader`, `validate_bound_server`, `OpcError::InvalidState`, `endpoint isolation`
- **Category:** Domain Modeling & Type Safety
- **Problem Statement:** A multi-role client facade implementing generic role traits (e.g. `TagReader`, `TagWriter`, `TagBrowser`) can accidentally allow a session bound to `ServerA` to execute queries against `ServerB` if trait signatures accept an arbitrary server identifier.
- **Root Cause & Technical Dynamics:** Standard role traits take `server: impl Into<ServerIdentifier>` to permit multi-server querying from unbound clients. When implemented on a `Bound` client instance, accepting an external server parameter creates an invariant hazard.
- **Anti-Pattern (What NOT to do):** Blindly dispatching queries using the caller-supplied server parameter on a bound client instance.
- **Architectural Solution (What to DO):**
  1. Separate client typestates at compile time: `OpcDaClient<C, Unbound>` (gateway) vs `OpcDaClient<C, Bound>` (dedicated session).
  2. In role trait implementations on `Bound`, execute an explicit endpoint verification check (`validate_bound_server`) comparing requested hosts and ProgIDs against `self.state.endpoint`, returning `OpcError::InvalidState` on mismatch.
  ```rust
  impl<C: ServerBackend + 'static> TagReader for OpcDaClient<C, Bound> {
      async fn read_tag_values(
          &self,
          server: impl Into<ServerIdentifier> + Send,
          tags: impl IntoTags + Send,
      ) -> OpcResult<TagValues> {
          self.validate_bound_server(server.into())?;
          self.read_tags(tags).await
      }
  }
  ```
- **Enforcement & Invariant:** Verified in `tests/tag_io_integration_test.rs` via `test_bound_client_server_mismatch_rejected`.

---

### 1.4 Native Rust 2024 AFIT (Async Fn in Trait)
- **Keywords / Search Tokens:** `AFIT`, `async fn in trait`, `Rust 2024`, `MSRV 1.93.1`, `async_trait`, `zero allocation`, `vtable`, `monomorphization`
- **Category:** Async Performance & Zero Allocation
- **Problem Statement:** Pre-Rust 2024 codebases rely on `#[async_trait]`, which desugars every async method into `Pin<Box<dyn Future<Output = ...> + Send + '_>>`. On high-frequency polling loops (e.g. reading 100 tags every 10ms), this incurs continuous heap allocation and vtable indirection.
- **Root Cause & Technical Dynamics:** `#[async_trait]` boxes futures on the heap to achieve dyn-compatibility under older compiler editions.
- **Anti-Pattern (What NOT to do):** Using `#[async_trait]` on performance-critical internal SPI traits under Rust 2024.
- **Architectural Solution (What to DO):** Migrate to native Rust 2024 async traits returning `impl Future<Output = ...> + Send`:
  ```rust
  pub trait TagReader: Send + Sync {
      fn read_tag_values(
          &self,
          server: impl Into<ServerIdentifier> + Send,
          tags: impl IntoTags + Send,
      ) -> impl Future<Output = OpcResult<TagValues>> + Send;
  }
  ```
  This guarantees zero heap allocation for the returned future and allows the compiler to monomorphize the entire async call graph.
- **Enforcement & Invariant:** Verified under `cargo clippy -- -D warnings` on Rust 2024 edition.

---

### 1.5 Associated Function Type Inference Ambiguity (`E0283`) with Default Generic Parameters
- **Keywords / Search Tokens:** `E0283`, `type annotations needed`, `Default generic parameter`, `OpcDaClientBuilder`, `associated function`, `turbofish`
- **Category:** Rust Language Invariants & Generic Constructors
- **Problem Statement:** When a generic struct `OpcDaClientBuilder<C = DefaultBackendConnector>` implements `impl<C: ServerBackend + Default> OpcDaClientBuilder<C> { pub fn new() -> Self }`, calling `OpcDaClientBuilder::new()` fails with compiler error `E0283` ("type annotations needed for `C`").
- **Root Cause & Technical Dynamics:** Default generic type parameters on struct declarations (`struct Foo<C = DefaultType>`) apply only when referring to the type without parameters in type position. In expression position, calling an associated function `Foo::new()` defined on a generic `impl<C>` block does not automatically constrain `C` to the default type parameter if multiple types implement `ServerBackend + Default`.
- **Anti-Pattern (What NOT to do):**
  ```rust
  // FAILS TO COMPILE (E0283) at call site `OpcDaClientBuilder::new()`:
  impl<C: ServerBackend + Default> OpcDaClientBuilder<C> {
      pub fn new() -> Self {
          Self { connector: C::default(), ... }
      }
  }
  ```
- **Architectural Solution (What to DO):** Implement `new()` directly on the concrete default type parameter, while providing generic construction via `Default` or explicit constructor arguments:
  ```rust
  // Unambiguous: no turbofish required at call sites
  impl OpcDaClientBuilder<DefaultBackendConnector> {
      pub fn new() -> Self {
          Self { connector: DefaultBackendConnector::default(), ... }
      }
  }

  // Generic constructor for custom backends:
  impl<C: ServerBackend> OpcDaClientBuilder<C> {
      pub fn new_with_connector(connector: C) -> Self {
          Self { connector, ... }
      }
  }
  ```
- **Enforcement & Invariant:** Verified in `tests/builder_smoke_test.rs` and across all doc-tests without turbofish annotations.

---

### 1.6 Intermediate Type Inference Collisions on Generic Collection Conversions (`E0283`)
- **Keywords / Search Tokens:** `E0283`, `IntoWriteBatch`, `intermediate type inference`, `Into<String>`, `collection conversions`, `trait solver`
- **Category:** Rust Type Solver & Trait Ergonomics
- **Problem Statement:** Implementing generic collection conversions such as `impl<S, V> IntoWriteBatch for Vec<(S, V)> where S: Into<String> + Send, V: Into<OpcValue> + Send` causes callers chaining `.into()` on string literals (e.g. `vec![("Tag.1".into(), val)].into_write_batch()`) to fail with `E0283` ("type annotations needed").
- **Root Cause & Technical Dynamics:** When `"Tag.1".into()` is invoked, the compiler must resolve an intermediate type `S` such that `&str: Into<S>` and `S: Into<String>`. Because multiple standard types satisfy this relationship (`String`, `Cow<'_, str>`, `Box<str>`, `&str`), the trait solver cannot infer a unique type for `S`.
- **Anti-Pattern (What NOT to do):** Requiring callers to write cumbersome turbofish expressions `("Tag.1".to_string(), val)`.
- **Architectural Solution (What to DO):** Provide concrete types directly in collection fixtures (e.g. string slices `("Tag.1", val)` or owned `("Tag.1".to_string(), val)`). Because `&str` satisfies `Into<String> + Send` directly, the compiler immediately infers `S = &'static str` without type ambiguity or intermediate allocations.
- **Enforcement & Invariant:** Tested in `tests/tag_io_integration_test.rs` across slice and owned collection inputs.

---

### 1.7 Rust 2024 Anonymous Lifetimes in Associated Type Bounds (`E0658`)
- **Keywords / Search Tokens:** `E0658`, `Rust 2024`, `MSRV 1.93.1`, `anonymous lifetime`, `impl Trait`, `associated type bound`, `compiler feature gate`
- **Category:** Rust 2024 / MSRV 1.93.1 Lifetime Rules
- **Problem Statement:** In Rust 2024, declaring a function parameter using `impl Trait` with an elided reference lifetime in an associated type bound (e.g. `tags: impl ExactSizeIterator<Item = &str>`) triggers compiler error `E0658` ("anonymous lifetimes in `impl Trait` are unstable").
- **Root Cause & Technical Dynamics:** In Rust 2024, lifetime elision in complex associated type bounds within existential parameter positions is not yet stabilized and requires explicit lifetime declarations.
- **Anti-Pattern (What NOT to do):**
  ```rust
  // FAILS TO COMPILE (E0658) in Rust 2024:
  pub(crate) fn assemble_tag_values(
      tags: impl ExactSizeIterator<Item = &str>,
      results: Vec<GroupItemResult>,
  ) -> OpcResult<TagValues>
  ```
- **Architectural Solution (What to DO):** Introduce an explicit named lifetime parameter in the function signature:
  ```rust
  // Stable in Rust 2024 / MSRV 1.93.1:
  pub(crate) fn assemble_tag_values<'a>(
      tags: impl ExactSizeIterator<Item = &'a str>,
      results: Vec<GroupItemResult>,
  ) -> OpcResult<TagValues> { ... }
  ```
- **Enforcement & Invariant:** Enforced across all internal collection assembly utilities in `opc-da-client/src/types/`.

---

### 1.8 Semantic Sequence Equality vs Derived Representation Equality
- **Keywords / Search Tokens:** `PartialEq`, `WriteBatch`, `SSO`, `InlineSingle`, `StaticSingle`, `Owned`, `representation equality`, `sequence equivalence`
- **Category:** Domain Model Design & Mathematical Equivalence
- **Problem Statement:** Encapsulated collection types that support multiple storage representations (e.g. 31-byte stack SSO `InlineSingle`, static literals `StaticSingle`, owned strings `OwnedSingle`, and slices `Owned`) produce false negatives under derived `PartialEq`. An inline single write does not match an owned single write under discriminant comparison, even though their tag names and values are identical.
- **Root Cause & Technical Dynamics:** `#[derive(PartialEq)]` requires identical enum variants and structural fields. Two collections with equivalent elements stored under different enum variants fail derived equality.
- **Anti-Pattern (What NOT to do):** Deriving `PartialEq` on multi-variant optimized representation enums.
- **Architectural Solution (What to DO):** Implement manual sequence `PartialEq` comparing sequence length and borrowed iterator items:
  ```rust
  impl PartialEq for WriteBatch {
      fn eq(&self, other: &Self) -> bool {
          self.len() == other.len() && self.iter().eq(other.iter())
      }
  }
  ```
  This guarantees representation-independent equality across all $5 \times 5 = 25$ cross-variant permutations.
- **Enforcement & Invariant:** Verified in `tests/write_batch_test.rs` via cross-variant comparison matrix tests.

---

### 1.9 Trait Object Safety Preservation Across Facade and Role Layers
- **Keywords / Search Tokens:** `Object safety`, `dyn TagReader`, `mockall`, `generic method`, `SPI trait`, `IntoWriteBatch`, `facade decoupling`
- **Category:** API Architecture & Object Safety
- **Problem Statement:** Adding generic conversion parameters (e.g. `tags: impl IntoTags`, `writes: impl IntoWriteBatch`) directly to public SPI traits (`TagReader`, `TagWriter`, `OpcProvider`) permanently destroys trait object safety (`dyn TagReader`), making dynamic dispatch and mock generation (`mockall`) impossible.
- **Root Cause & Technical Dynamics:** In Rust, methods with generic type parameters cannot be called via trait objects because the compiler cannot construct a finite vtable for an infinite set of potential monomorphized types.
- **Anti-Pattern (What NOT to do):** Putting `impl Into<T>` or `impl IntoWriteBatch` parameters directly into core SPI trait definitions.
- **Architectural Solution (What to DO):**
  1. Keep SPI role traits strictly concrete: `fn write_tag_batch(&self, server: &ServerIdentifier, batch: WriteBatch)`.
  2. Implement ergonomic generic polymorphism (`impl IntoWriteBatch`) on inherent facade methods on `OpcDaClient`:
     ```rust
     impl<C: ServerBackend, State> OpcDaClient<C, State> {
         pub async fn write_tags(
             &self,
             server: impl TryInto<ServerIdentifier>,
             writes: impl IntoWriteBatch,
         ) -> OpcResult<Vec<WriteResult>> {
             let batch = writes.into_write_batch();
             // dispatch concrete batch to SPI role trait...
         }
     }
     ```
- **Enforcement & Invariant:** Enforced across `TagReader`, `TagWriter`, and `TagBrowser` SPI traits in `opc-da-client/src/api/`.

---

## 2. Concurrency, Backpressure & Background Worker Resilience

### 2.1 Preserving Channel Backpressure Across Async/Sync Boundaries
- **Keywords / Search Tokens:** `Backpressure`, `tokio::sync::mpsc`, `try_recv`, `priority queue`, `MAX_QUEUE_DEPTH`, `OOM prevention`, `buffer overflow`
- **Category:** Concurrency Safety & Memory Bounding
- **Problem Statement:** An async Tokio task sends requests to a dedicated background OS worker thread via a bounded channel (`tokio::sync::mpsc::channel(32)`). To optimize throughput, the worker thread event loop drains incoming requests using `while let Ok(req) = rx.try_recv()`. Under sustained traffic bursts, this completely nullifies Tokio's backpressure, allowing the worker's internal priority queue to grow without bound until out-of-memory (OOM) occurs.
- **Root Cause & Technical Dynamics:** Unbounded `try_recv()` drains the bounded channel faster than requests are processed, moving all pending requests into an unbounded heap vector and bypassing the Tokio channel's capacity limits.
- **Anti-Pattern (What NOT to do):**
  ```rust
  // DANGEROUS: Unbounded drain bypasses channel capacity:
  while let Ok(pending) = rx.try_recv() {
      queue.push(pending);
  }
  ```
- **Architectural Solution (What to DO):** Enforce a strict compile-time queue depth ceiling (`MAX_QUEUE_DEPTH = 64`):
  ```rust
  pub(crate) const MAX_QUEUE_DEPTH: usize = 64;

  while queue.len() < MAX_QUEUE_DEPTH {
      if let Ok(pending) = rx.try_recv() {
          queue.push(pending);
      } else {
          break;
      }
  }
  ```
  Once the queue reaches capacity, `rx.try_recv()` stops, leaving items in the Tokio channel buffer. When the 32-slot channel fills, async callers awaiting `tx.send(req).await` naturally suspend, propagating backpressure upstream.
- **Enforcement & Invariant:** Verified in `opc-da-client/src/worker/event_loop.rs`.

---

### 2.2 Transparent Panic Recovery with Explicit In-Flight Request Drainage
- **Keywords / Search Tokens:** `catch_unwind`, `WorkerError::Panic`, `oneshot::error::RecvError`, `drain_and_reject`, `error provenance`, `panic teardown`
- **Category:** Concurrency Diagnostics & Error Provenance
- **Problem Statement:** When a synchronous COM call panics inside a background worker thread, `std::panic::catch_unwind` recovers the event loop. However, calling `queue.clear()` drops all pending requests. Awaiting Tokio tasks receive `oneshot::error::RecvError`, which maps to a misleading `WorkerTerminated` error even though the worker survived and is ready for traffic.
- **Root Cause & Technical Dynamics:** Dropping a oneshot sender without sending a value causes the receiver to error with `RecvError`, losing the causal link to the panic that cleared the queue.
- **Anti-Pattern (What NOT to do):** Simply clearing the request queue on panic (`queue.clear()`).
- **Architectural Solution (What to DO):** Implement `drain_and_reject` on the priority queue to reply to all in-flight oneshot senders with an explicit `WorkerError::Panic`:
  ```rust
  if let Err(payload) = std::panic::catch_unwind(AssertUnwindSafe(|| handle_request(...))) {
      let msg = panic_message(&payload);
      let panic_err = OpcError::Worker(WorkerError::Panic(format!(
          "Worker thread recovered from panic on preceding request: {msg}"
      )));
      
      // Explicitly reject all awaiting callers:
      queue.drain_and_reject(&panic_err);
  }
  ```
- **Enforcement & Invariant:** Tested in `tests/worker_panic_recovery_test.rs`.

---

### 2.3 Early Cancellation Check (Avoiding Wasted FFI / RPC)
- **Keywords / Search Tokens:** `Cancellation check`, `is_closed`, `oneshot`, `timeout`, `RPC optimization`, `resource leak`, `Tokio timeout`
- **Category:** Performance & Resource Conservation
- **Problem Statement:** In high-concurrency systems, callers frequently set timeouts via `tokio::time::timeout`. If a caller times out or drops its receiver while a request is sitting in the worker queue, dispatching expensive synchronous COM RPC roundtrips wastes CPU, network, and COM proxy resources.
- **Root Cause & Technical Dynamics:** Once a request enters the worker queue, it is processed sequentially regardless of whether the initiating caller is still waiting.
- **Anti-Pattern (What NOT to do):** Executing foreign COM RPC calls without checking receiver status.
- **Architectural Solution (What to DO):** Check `reply.is_closed()` immediately before initiating synchronous dispatch:
  ```rust
  if reply.is_closed() {
      tracing::debug!(op, server = %endpoint, "Caller cancelled request; skipping dispatch");
      return;
  }
  ```
- **Enforcement & Invariant:** Verified in `opc-da-client/src/worker/dispatcher.rs`.

---

### 2.4 Amortizing Mutex Contention via Chunked Accumulation
- **Keywords / Search Tokens:** `Mutex contention`, `TagCollector`, `push_batch`, `chunking`, `lock overhead`, `namespace browse`, `throughput`
- **Category:** Concurrency Performance
- **Problem Statement:** When browsing large industrial server namespaces (e.g. 50,000 tags), acquiring a shared accumulator mutex (`Arc<Mutex<TagCollector>>`) per individual leaf tag causes severe lock contention, degrading throughput to <500 tags/sec.
- **Root Cause & Technical Dynamics:** Mutex acquisition incurs atomic compare-and-swap operations and cacheline invalidation on every invocation. Calling it 50,000 times in a tight loop creates high contention between background ingestion and observer threads.
- **Anti-Pattern (What NOT to do):** Calling `collector.push(tag)` in a tight loop for every single leaf item.
- **Architectural Solution (What to DO):** Buffer items in a local stack/thread-local buffer and flush in 256-item batches via `collector.push_batch(std::mem::replace(&mut chunk, Vec::with_capacity(256)))`:
  - Reduces mutex acquisitions by **99.6%**.
  - Provides a natural checkpoint for cooperative cancellation (`if collector.is_cancelled() || collector.is_full() { break; }`).
- **Enforcement & Invariant:** Verified in `opc-da-client/src/types/collector.rs` and browse handlers.

---

### 2.5 Reader-Writer Concurrency Decoupling for Streaming Accumulators (`RwLock` vs `Mutex`)
- **Keywords / Search Tokens:** `RwLock`, `Mutex`, `TagCollector`, `snapshot`, `concurrency decoupling`, `AtomicUsize`, `AtomicBool`, `read scalability`
- **Category:** Concurrency Safety & Read Scalability
- **Problem Statement:** Using `std::sync::Mutex<Vec<String>>` to protect a shared tag accumulator forces exclusive lock acquisition even for read-only observer queries (e.g. `collector.snapshot()`). Holding an exclusive lock while deep-cloning 10,000 strings serializes concurrent readers (TUI rendering, CLI telemetry, monitoring threads) and completely stalls the background MTA ingestion worker.
- **Root Cause & Technical Dynamics:** A mutual exclusion lock grants single-thread access. Slow read operations starve write operations, stalling the streaming pipeline.
- **Anti-Pattern (What NOT to do):** Protecting read-heavy shared data structures with standard exclusive `Mutex`.
- **Architectural Solution (What to DO):**
  1. Upgrade storage to `std::sync::RwLock<Vec<String>>`.
  2. Implement atomic lock-free queries (`AtomicUsize` for `len()`, `AtomicBool` for `is_cancelled()`, `is_full()`).
  3. `snapshot()` acquires a shared read lock (`read()`), allowing unlimited parallel observer reads while background ingestion acquires write locks (`write()`) only for brief chunk flushes.
- **Enforcement & Invariant:** Verified in `opc-da-client/src/types/collector.rs`.

---

### 2.6 Symmetrical Poison Recovery Across Lock Acquisition Sites
- **Keywords / Search Tokens:** `Lock poisoning`, `into_inner`, `RwLock`, `Mutex`, `cascade panic prevention`, `atomic resynchronization`
- **Category:** Concurrency Resilience & Lock Recovery
- **Problem Statement:** When a worker or observer thread panics while holding a standard lock, Rust marks the lock as poisoned. Calling `.unwrap()` or `.expect()` on poisoned locks cascades panics to healthy observer threads, causing entire applications to crash.
- **Root Cause & Technical Dynamics:** In standard Rust synchronization primitives, acquiring a poisoned lock returns an `Err(PoisonError<T>)`.
- **Anti-Pattern (What NOT to do):** Writing `lock.write().unwrap()` or `lock.read().expect("lock poisoned")`.
- **Architectural Solution (What to DO):** Implement uniform symmetrical poison recovery across all lock acquisition sites (`clear`, `snapshot`, `harvest`, `push`, `push_batch`):
  ```rust
  let mut guard = match self.inner.tags.write() {
      Ok(g) => g,
      Err(poisoned) => poisoned.into_inner(),
  };
  // Resynchronize atomic count with physical buffer length:
  self.inner.count.store(guard.len(), Ordering::Release);
  ```
- **Enforcement & Invariant:** Tested in `tests/collector_test.rs` under simulated thread panic conditions.

---

### 2.7 RAII Unwind Safety in Streaming Iterators (`PushBatchGuard`)
- **Keywords / Search Tokens:** `RAII guard`, `PushBatchGuard`, `Drop`, `unwind safety`, `streaming iterator`, `atomic count integrity`
- **Category:** Concurrency Invariants & Unwind Protection
- **Problem Statement:** In streaming methods accepting arbitrary iterators (`push_batch(iter: impl IntoIterator<Item = String>)`), if an untrusted third-party iterator panics midway through iteration, items already appended to the internal buffer remain uncounted in atomic telemetry (`count`), corrupting subsequent size and capacity checks.
- **Root Cause & Technical Dynamics:** Unwinding skips manual trailing update statements (e.g. `self.count.store(...)`).
- **Anti-Pattern (What NOT to do):** Updating atomic counters only after iterator completion.
- **Architectural Solution (What to DO):** Wrap streaming ingestion in an RAII drop guard:
  ```rust
  struct PushBatchGuard<'a> {
      count: &'a AtomicUsize,
      tags: &'a mut Vec<String>,
  }

  impl Drop for PushBatchGuard<'_> {
      fn drop(&mut self) {
          self.count.store(self.tags.len(), Ordering::Release);
      }
  }
  ```
  Whether iteration completes successfully or unwinds due to a panic, the guard's `Drop` execution guarantees that atomic `count` matches `tags.len()`.
- **Enforcement & Invariant:** Verified in `opc-da-client/src/types/collector.rs`.

---

### 2.8 Zero-Allocation Hot-Path Request Dispatch Across `catch_unwind`
- **Keywords / Search Tokens:** `catch_unwind`, `AssertUnwindSafe`, `zero allocation`, `hot path`, `borrowing closure`, `string formatting`
- **Category:** Hot-Path Allocation Optimization
- **Problem Statement:** Dispatching background requests through `std::panic::catch_unwind(AssertUnwindSafe(...))` often tempts engineers to pre-clone endpoints or pre-format host strings so they are accessible in the panic recovery block, creating heap allocations on 100% of happy-path dispatches.
- **Root Cause & Technical Dynamics:** Closures passed to `catch_unwind` require `UnwindSafe`. Moving cloned data into closures satisfies the typechecker easily but incurs unnecessary allocations.
- **Anti-Pattern (What NOT to do):** Cloning strings or structs into the closure purely for error reporting.
- **Architectural Solution (What to DO):** Borrow the endpoint reference `&OpcServerEndpoint` across the `AssertUnwindSafe` closure boundary. Restrict all string formatting and heap allocations strictly to the cold panic branch:
  ```rust
  let endpoint_ref = AssertUnwindSafe(&endpoint);
  let res = std::panic::catch_unwind(AssertUnwindSafe(|| {
      self.dispatch_request(endpoint_ref.0, ...)
  }));
  if let Err(payload) = res {
      // String allocation occurs ONLY on worker thread panic:
      let msg = format!("Worker panicked while dispatching to {}", endpoint.host_or_local());
      ...
  }
  ```
- **Enforcement & Invariant:** Validated via zero-allocation benchmarks and code review in `opc-da-client/src/worker/dispatcher.rs`.

---

## 3. Memory Safety, Allocation Budgets & Zero-Allocation Patterns

### 3.1 Static Small Array Optimization (`StaticSmall`)
- **Keywords / Search Tokens:** `TagBatchRepr`, `StaticSmall`, `zero allocation`, `polling loops`, `inline array`, `memory layout`
- **Category:** Zero-Allocation Hot Paths
- **Problem Statement:** Polling loops frequently read small, fixed sets of tags (e.g. `["Sensor1", "Sensor2"]`). Standard implementations convert array literals into heap-allocated `Vec<String>`, causing millions of short-lived allocations per minute.
- **Root Cause & Technical Dynamics:** Using heap vectors for small arrays incurs allocator overhead, heap fragmentation, and cacheline misses.
- **Anti-Pattern (What NOT to do):** Converting `[&'static str]` to `Vec<String>` on every polling tick.
- **Architectural Solution (What to DO):** Define a multi-tier `TagBatchRepr` containing an inline array variant for $N \le 4$:
  ```rust
  pub(crate) enum TagBatchRepr {
      Empty,
      StaticSingle(&'static str),
      StaticSmall([&'static str; 4], u8),
      StaticArc(Arc<[&'static str]>),
      OwnedSingle(String),
      Owned(Arc<[String]>),
  }
  ```
  - For $N = 1$: `StaticSingle` (0 bytes heap).
  - For $N \in [2, 4]$: `StaticSmall` with inline length (0 bytes heap).
  - For $N > 4$: `StaticArc` allocating a single contiguous slice of string pointers (0 string copies).
- **Enforcement & Invariant:** Verified in `opc-da-client/src/types/batch.rs`.

---

### 3.2 UTF-8 Multibyte Slicing Safety in Inline SSO Buffers
- **Keywords / Search Tokens:** `UTF-8 slicing`, `multibyte`, `Utf8Error::valid_up_to`, `SSO`, `panic prevention`, `codepoint tearing`
- **Category:** Memory Safety & Panic Prevention
- **Problem Statement:** When storing small strings inline in fixed-size byte buffers (e.g. 31 bytes), truncating strings that contain multibyte UTF-8 characters (e.g. Cyrillic, kanji, accents) at a fixed byte length will slice through a code point, causing `std::str::from_utf8` to fail or panic.
- **Root Cause & Technical Dynamics:** UTF-8 code points can span 1 to 4 bytes. Slicing at an arbitrary byte index can split a multibyte sequence.
- **Anti-Pattern (What NOT to do):** Slicing directly with `&bytes[..31]`.
- **Architectural Solution (What to DO):** Use `std::str::from_utf8` combined with `Utf8Error::valid_up_to` to safely fallback to the largest valid character boundary:
  ```rust
  match std::str::from_utf8(&self.bytes[..len]) {
      Ok(s) => s,
      Err(e) => {
          let valid_len = e.valid_up_to();
          std::str::from_utf8(&self.bytes[..valid_len]).unwrap_or("")
      }
  }
  ```
- **Enforcement & Invariant:** Tested with Cyrillic, Japanese, and accented UTF-8 inputs in `tests/write_batch_test.rs`.

---

### 3.3 Double-Panic Containment in RAII Drops
- **Keywords / Search Tokens:** `Double panic`, `Drop`, `catch_unwind`, `GroupGuard`, `COM RPC fault`, `abort prevention`, `unwind safety`
- **Category:** Crash Prevention & Unwind Safety
- **Problem Statement:** Rust aborts the entire process if a panic occurs while another panic is unwinding the stack. If a resource drop guard (e.g. `GroupGuard`, `BrowsePositionGuard`, or `ConnectionPool::clear`) encounters a COM RPC fault and panics during an active unwind, the application dies instantly.
- **Root Cause & Technical Dynamics:** When a thread is already unwinding, a second panic cannot be handled and triggers `std::process::abort()`.
- **Anti-Pattern (What NOT to do):** Allowing fallible foreign COM calls inside `Drop::drop` to panic.
- **Architectural Solution (What to DO):** Wrap all foreign drop operations in `std::panic::catch_unwind`:
  ```rust
  impl<'a, S: ConnectedServer> Drop for GroupGuard<'a, S> {
      fn drop(&mut self) {
          let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
              if let Err(e) = self.server.remove_group(&self.handle) {
                  tracing::warn!(?e, "Failed to remove group during drop");
              }
          }));
      }
  }
  ```
- **Enforcement & Invariant:** Implemented across all RAII guards in `opc-da-client/src/client/`.

---

### 3.4 Eliminating Sentinel Allocations on Hot Paths
- **Keywords / Search Tokens:** `Sentinel values`, `pre-allocation`, `TagValue`, `GroupItemResult`, `allocation optimization`, `hot path`
- **Category:** Performance Optimization
- **Problem Statement:** In read operations, initializing results by pre-populating a vector with placeholder `"Not read"` error strings forces unnecessary string allocations on every single tag read cycle, even when reads succeed immediately.
- **Root Cause & Technical Dynamics:** Eager initialization with dummy heap structures incurs allocator overhead before the actual values are known.
- **Anti-Pattern (What NOT to do):** Pre-allocating vectors filled with dummy error strings.
- **Architectural Solution (What to DO):** Pre-allocate exact vector capacity and use single-pass construction mapping COM item state records directly to `TagValue` without intermediate sentinel values.
- **Enforcement & Invariant:** Verified in `opc-da-client/src/client/tag_io.rs`.

---

### 3.5 In-Place Vector Buffer Draining (`chunk.drain(..)`) vs Reallocation
- **Keywords / Search Tokens:** `chunk.drain`, `into_iter`, `clippy::iter_with_drain`, `buffer reuse`, `allocator churn`, `memory efficiency`
- **Category:** Memory Churn Elimination & High-Throughput Buffering
- **Problem Statement:** In chunked streaming operations (e.g. flat namespace browsing), replacing the local accumulator at batch boundaries via `std::mem::replace(&mut chunk, Vec::with_capacity(256))` allocates and deallocates a new 6 KiB buffer on every single flush. Over 10,000 tags, this creates ~40 throwaway vectors (~240 KB of allocator churn).
- **Root Cause & Technical Dynamics:** Replacing vectors discards allocated heap capacity on each flush, forcing the allocator to repeatedly allocate and free blocks.
- **Anti-Pattern (What NOT to do):** Replacing the vector with a new one at every flush boundary.
- **Architectural Solution (What to DO):** Use `chunk.drain(..)` into receiver traits accepting `impl IntoIterator<Item = T>`:
  ```rust
  // Reuses the same physical buffer across thousands of flushes:
  collector.push_batch(chunk.drain(..));
  ```
  *Clippy Note:* The nursery lint `clippy::iter_with_drain` recommends `.into_iter()`, but `.into_iter()` deallocates the vector buffer. A scoped `#[allow(clippy::iter_with_drain)]` is appropriate when retaining vector buffer capacity across hot loops.
- **Enforcement & Invariant:** Verified in browse handlers in `opc-da-client/src/client/browse.rs`.

---

### 3.6 Lazy Slot Allocation Buffers vs Eager Dummy Records
- **Keywords / Search Tokens:** `Lazy slot buffer`, `WriteResult`, `Vec<Option<T>>`, `dead store elimination`, `allocator reduction`, `batch write`
- **Category:** Allocator Efficiency & Industrial Actuation Optimization
- **Problem Statement:** In batch write operations, pre-populating a result vector with eager dummy failure records (`WriteResult::failure(*tag_id, OpcError::InvalidState("..."))`) allocates $2N$ throwaway heap strings ($N$ cloned tag names $+ N$ error strings). On a 10,000-tag batch where 100% of writes succeed, this allocates 20,000 strings only to overwrite and drop them immediately, generating 50,005 allocator calls.
- **Root Cause & Technical Dynamics:** Eager placeholder population creates dead store allocations that are overwritten upon successful operation.
- **Anti-Pattern (What NOT to do):** Eagerly creating `WriteResult::failure` placeholders for all items before write execution.
- **Architectural Solution (What to DO):** Initialize a slot buffer of `Vec<Option<WriteResult>> = vec![None; items.len()]`. Successful or quarantined write results populate slots lazily upon completion, reducing allocator calls by **80.0%** (from 50,005 to 10,004) and completely eliminating dead store string allocations.
- **Enforcement & Invariant:** Implemented in `opc-da-client/src/client/tag_io.rs` write batch handlers.

---

### 3.7 31-Byte Stack Small String Optimization (SSO) with Zero-Padding 72-Byte Layout
- **Keywords / Search Tokens:** `SSO`, `WriteBatchRepr`, `InlineSingle`, `72-byte layout`, `zero padding`, `cache alignment`, `PLC setpoints`, `hot loop`
- **Category:** Cache Budgeting & Zero-Allocation Primitives
- **Problem Statement:** High-frequency control loops (e.g. 50 Hz PLC setpoints) commanding scalar tag writes generate thousands of heap allocations per minute if the domain primitive encapsulates an owned `String`.
- **Root Cause & Technical Dynamics:** Standard `String` allocates 24 bytes on the stack plus a separate heap buffer.
- **Anti-Pattern (What NOT to do):** Requiring heap-allocated `String` for short tag names in high-frequency control loops.
- **Architectural Solution (What to DO):** Implement an opaque `WriteBatch` backed by a 5-variant `WriteBatchRepr` featuring `InlineSingle([u8; 31], u8, OpcValue)`:
  - Tags $\le 31$ bytes reside entirely in a 32-byte inline stack buffer (31 data bytes $+ 1$ length byte).
  - Memory layout on `x86_64` is exactly 72 bytes with zero internal padding bytes.
  - Safe UTF-8 extraction via `inline_as_str()` is backed by constructor validation, eliminating codepoint tearing hazards (`CWE-20/787`).
  - Completely eliminates 3,000 heap allocations per minute in 50 Hz control loops.
- **Enforcement & Invariant:** Struct size verified via `std::mem::size_of::<WriteBatchRepr>() == 72` in `tests/write_batch_test.rs`.

---

### 3.8 Zero-Copy Terminal Data Handoff via `std::mem::take`
- **Keywords / Search Tokens:** `harvest`, `std::mem::take`, `zero copy`, `O(1) transfer`, `TagCollector`, `memory reset`, `move semantics`
- **Category:** Move Semantics & Terminal Lifecycle Optimization
- **Problem Statement:** Long-running accumulation tasks (such as namespace browsing) that return collected tags via `collector.snapshot()` perform a deep copy of the entire collection (10,001 heap allocations, ~860 KB memory churn for 10k tags), leaving duplicate zombie strings pinned inside the collector instance.
- **Root Cause & Technical Dynamics:** `snapshot()` clones the buffer to leave the original intact. If the collector is dropped or reset immediately after, cloning was unnecessary.
- **Anti-Pattern (What NOT to do):** Relying solely on deep-cloning `snapshot()` for terminal collection consumption.
- **Architectural Solution (What to DO):** Provide a dedicated `harvest()` method utilizing `std::mem::take(&mut *guard)`:
  ```rust
  pub fn harvest(&self) -> Vec<String> {
      let mut guard = match self.inner.tags.write() {
          Ok(g) => g,
          Err(p) => p.into_inner(),
      };
      self.inner.count.store(0, Ordering::Release);
      std::mem::take(&mut *guard)
  }
  ```
  This transfers vector ownership to the caller in $O(1)$ time via pointer swap (0 allocations), leaving the collector reset and memory-neutral.
- **Enforcement & Invariant:** Verified in `tests/collector_test.rs`.

---

## 4. Industrial Control Systems (ICS) & PLC Actuation Safety

### 4.1 Write Non-Idempotency & Duplicate Actuation Hazards
- **Keywords / Search Tokens:** `Non-idempotent write`, `duplicate actuation`, `PLC safety`, `RetryPolicy`, `fail fast`, `valve pulse`, `motor control`
- **Category:** Industrial Functional Safety
- **Problem Statement:** In traditional HTTP/REST services, connection drops are often transparently retried. In industrial automation, retrying a write operation (`IOPCSyncIO::Write`) across a disconnected or timed-out COM connection can cause **duplicate physical actuations** on PLCs (e.g. dispensing double raw material, toggling a motor twice, or pulsing a valve).
- **Root Cause & Technical Dynamics:** Network drops or timeouts do not confirm whether the physical PLC executed the write command before the communication link severed.
- **Anti-Pattern (What NOT to do):** Retrying timed-out or dropped write operations automatically.
- **Architectural Invariant:**
  - **Reads, Browsing, Pings (`RetryPolicy::Idempotent`):** Stale connection is evicted; a fresh connection is established transparently and the read is re-executed.
  - **Single & Batch Writes (`RetryPolicy::NonIdempotent`):** Stale connection is evicted from the pool to protect subsequent calls, but the write operation **MUST FAIL FAST** and return the error to the caller. The application layer must decide whether physical state permits a re-write.
- **Enforcement & Invariant:** Tested in `tests/resilience_and_pool_integration_test.rs` asserting no retry on write failures.

---

### 4.2 Active Group Poisoning on Array Length Mismatch
- **Keywords / Search Tokens:** `Group poisoning`, `array length mismatch`, `clear_active_group`, `COM server bug`, `outage recovery`, `cached group`
- **Category:** State Machine Resilience
- **Problem Statement:** OPC DA servers return arrays of item states corresponding to registered item handles. If a faulty or misconfigured server returns an item state array whose length does not match the registered tags, propagating the error via `?` leaves the active group cached in the client connection pool.
- **Root Cause & Technical Dynamics:** Returning early via `?` bypasses group cleanup, leaving an invalid group handle in the connection pool cache.
- **Consequence:** Every subsequent read hitting that cached group will continuously fail with array index out-of-bounds or length mismatches, causing persistent outage until application restart.
- **Architectural Solution (What to DO):** Before returning an error on state length mismatch, explicitly evict and destroy the cached group (`pooled.clear_active_group()`).
- **Enforcement & Invariant:** Enforced in `opc-da-client/src/client/tag_io.rs`.

---

### 4.3 Precondition Validation Prior to OS Thread & Apartment Allocation
- **Keywords / Search Tokens:** `Precondition validation`, `OpcDaClientBuilder`, `MTA thread`, `resource leak prevention`, `COM initialization`
- **Category:** Resource Allocation Safety
- **Problem Statement:** A client builder (`OpcDaClientBuilder::build_bound`) that initializes the COM worker thread before checking whether a server endpoint was configured spawns a dedicated OS thread and initializes a Windows COM MTA apartment, only to fail immediately afterwards.
- **Root Cause & Technical Dynamics:** Executing resource-heavy initializations before verifying basic input preconditions wastes system resources and leaks background worker threads on failure.
- **Anti-Pattern (What NOT to do):** Spawning threads or initializing runtimes before parameter validation.
- **Architectural Solution (What to DO):** Validate all required configuration parameters (server identity, timeout constraints, batch limits) *prior* to allocating OS resources:
  ```rust
  pub fn build_bound(self) -> OpcResult<OpcDaClient<C, Bound>> {
      if self.server.is_none() {
          return Err(OpcError::InvalidState(
              "Cannot build bound client: server endpoint was not specified".into(),
          ));
      }
      let endpoint = self.server.clone().unwrap();
      let client = self.build()?;
      Ok(client.bind(endpoint))
  }
  ```
- **Enforcement & Invariant:** Verified in `tests/builder_smoke_test.rs`.

---

### 4.4 Granular Industrial Input Quarantine vs Cascading Batch Abort (`CWE-626` / `CWE-400`)
- **Keywords / Search Tokens:** `Granular quarantine`, `batch write`, `CWE-626`, `CWE-400`, `fail-partial`, `industrial safety`, `starvation prevention`
- **Category:** Industrial Functional Safety & Availability
- **Problem Statement:** If a batch write operation validates tag strings by failing fast via `?` on the first invalid tag (e.g. containing an interior null byte `\0`), the entire batch is immediately aborted. In an industrial control system commanding multiple physical actuators (e.g. tank valves, safety interlocks, motor setpoints), a single contaminated tag would starve all valid commands in the batch, causing industrial process halts or safety hazards.
- **Root Cause & Technical Dynamics:** Fail-fast validation on aggregated batches treats all batch members as a monolithic unit, creating availability starvation (`CWE-400`).
- **Anti-Pattern (What NOT to do):** Aborting an entire batch of physical actuation setpoints because one tag is malformed.
- **Architectural Solution (What to DO):** Implement granular item-level quarantine at Entry Gate 1:
  1. Inspect each tag string individually during initial partitioning.
  2. If a tag contains `\0`, isolate it directly into `write_results[orig_idx] = Some(WriteResult::failure(...))` with structured warning telemetry (`tracing::warn!(..., tag = %tag_id.escape_debug())`).
  3. Valid tags in the batch proceed without interruption to COM item registration and write execution.
  4. If and only if 100% of tags in the batch are quarantined, short-circuit immediately before allocating an ephemeral COM group.
- **Enforcement & Invariant:** Verified in `tests/tag_io_integration_test.rs` via `test_write_tags_batch_partial_quarantine`.

---

### 4.5 Two-Stage Positional Index Mapping Under Granular Defensive Screening
- **Keywords / Search Tokens:** `Positional index mapping`, `attribution integrity`, `defensive screening`, `zip pairing`, `data integrity`
- **Category:** Data Integrity & State Attribution
- **Problem Statement:** When defensive filtering isolates invalid items prior to COM registration, array sizes diverge (`registered_items.len() < requested_items.len()`). Using direct indexing (`results[idx]`) corrupts result attribution, assigning the write status of `Tag3` to `Tag2`.
- **Root Cause & Technical Dynamics:** Filtering alters array offsets. Index-based array indexing after filtering causes index displacement and silent misattribution.
- **Anti-Pattern (What NOT to do):** Assuming output result array indices match input array indices when items are screened.
- **Architectural Solution (What to DO):** Establish two-stage positional index mapping:
  - **Stage 1 (`valid_orig_indices: Vec<usize>`):** Maps valid screened inputs to their original batch positions.
  - **Stage 2 (`valid_write_orig_indices: Vec<usize>`):** Maps successfully registered COM item handles to their original batch positions.
  Using `.zip()` pairing and array parity validation ensures strict 1:1 positional correspondence between requested tag order and returned result vectors, regardless of arbitrary partial rejections.
- **Enforcement & Invariant:** Verified in `tests/tag_io_integration_test.rs`.

---

### 4.6 Reconnection Retry Accumulator Clean Slate on Idempotent Browse
- **Keywords / Search Tokens:** `Clean slate`, `accumulator reset`, `reconnection retry`, `TagCollector::clear`, `duplicate elimination`, `browse`
- **Category:** State Machine Resilience & Data Integrity
- **Problem Statement:** When an idempotent namespace browse operation (`ComRequest::BrowseTags`) encounters a transient DCOM disconnection (e.g. `RPC_S_SERVER_UNAVAILABLE`), the connection pool automatically reconnects and re-dispatches the request with the *same* `TagCollector` accumulator instance. If the handler does not clear the accumulator on entry, tags accumulated prior to the drop remain in the buffer, appending duplicate entries from the root and prematurely hitting `max_tags` capacity limits.
- **Root Cause & Technical Dynamics:** Accumulator instances passed across retry boundaries retain previously collected state unless explicitly reset.
- **Anti-Pattern (What NOT to do):** Reusing accumulators across retry attempts without clearing.
- **Architectural Solution (What to DO):** Enforce a clean slate entry guard in idempotent browse handlers:
  ```rust
  if !collector.is_empty() {
      tracing::debug!("Clearing partial browse collector accumulator prior to retry traversal");
      collector.clear();
  }
  ```
  This guarantees that retries start with a pristine accumulator and zero duplicate items.
- **Enforcement & Invariant:** Tested in `tests/resilience_and_pool_integration_test.rs`.

---

## 5. Windows COM / DCOM FFI Boundaries & Win32 Peculiarities

### 5.1 CWE-626 Null-Byte Injection in COM Wide Strings
- **Keywords / Search Tokens:** `CWE-626`, `null byte injection`, `to_wide_null`, `PWSTR`, `PCWSTR`, `FFI boundary`, `security hardening`
- **Category:** Application Security & FFI Integrity
- **Problem Statement:** Win32 COM APIs consume null-terminated wide string pointers (`PWSTR`, `PCWSTR`). In Rust, a `&str` or `String` can contain embedded interior null bytes (`\0`). When converting a Rust string with interior nulls into a wide string via pointer casting, the COM FFI function silently truncates the string at the first `\0`.
- **Root Cause & Technical Dynamics:** C/Win32 APIs treat `\0` as a sentinel terminator, whereas Rust strings track explicit length and allow arbitrary byte values.
- **Exploitation / Hazard:** An attacker or misconfigured tag name (`"Sensors.Temperature\0.PrivilegedValve"`) can bypass tag namespace authorization checks or cause buffer miscalculations.
- **Architectural Solution (What to DO):** Hardened string conversion rejecting interior null bytes:
  ```rust
  pub(crate) fn to_wide_null(s: &str) -> OpcResult<Vec<u16>> {
      if s.contains('\0') {
          return Err(OpcError::InvalidState(format!(
              "String contains invalid interior null byte: {s:?}"
          )));
      }
      Ok(s.encode_utf16().chain(std::iter::once(0)).collect())
  }
  ```
- **Enforcement & Invariant:** Enforced across all Win32 COM string conversions in `opc-da-client/src/com/strings.rs`.

---

### 5.2 Cryptic Win32 RPC Disconnect Classification
- **Keywords / Search Tokens:** `RPC_S_SERVER_UNAVAILABLE`, `RPC_S_CALL_FAILED`, `0x800706BA`, `0x800706BE`, `is_connection_error`, `HRESULT`, `DCOM disconnect`
- **Category:** COM Error Handling & Diagnostic Mapping
- **Problem Statement:** When an out-of-process or remote OPC DA server crashes or network connectivity drops, Windows COM calls return raw HRESULT codes:
  - `0x800706BA` (`RPC_S_SERVER_UNAVAILABLE`): Server died or host unreachable.
  - `0x800706BE` (`RPC_S_CALL_FAILED`): RPC call was executed but communication link broken during transmission.
- **Diagnostic Trap:** These errors often originate from internal `IUnknown` or `IOPCServer` interface calls and are encapsulated within `OpcError::Server(msg, code)`. If `OpcError::is_connection_error` only checks `OpcError::Com`, automatic reconnection logic will fail to detect RPC disconnects.
- **Anti-Pattern (What NOT to do):** Checking only `OpcError::Com` variants when determining whether an error is reconnectable.
- **Architectural Solution (What to DO):** Inspect `OpcError::Server` codes using Win32 HRESULT bitwise classification:
  ```rust
  impl OpcError {
      #[must_use]
      pub fn is_connection_error(&self) -> bool {
          match self {
              Self::Com { source } => self::hresult::is_connection_hresult(source.code()),
              Self::Server(_, code) => {
                  self::hresult::is_connection_hresult(windows_core::HRESULT((*code).cast_signed()))
              }
              Self::Timeout(_) => true,
              Self::Worker(w) => w.is_connection_error(),
              _ => false,
          }
      }
  }
  ```
- **Enforcement & Invariant:** Tested in `tests/error_test.rs` asserting true for `0x800706BA` and `0x800706BE`.

---

### 5.3 Decoupling Windows COM via Pluggable Initializers (`NoOpComInit`)
- **Keywords / Search Tokens:** `ComInitializer`, `NoOpComInit`, `DefaultComInit`, `cross-platform`, `mocking`, `headless CI`, `CoInitializeEx`
- **Category:** Cross-Platform Portability & Mockability
- **Problem Statement:** Code interacting with Windows COM requires `CoInitializeEx(None, COINIT_MULTITHREADED)`. Calling this on non-Windows platforms or in headless Linux CI environments causes compilation failure or runtime crashes, preventing automated testing of domain logic.
- **Root Cause & Technical Dynamics:** Direct calls to platform-specific runtime initialization APIs permanently bind compilation and execution to that platform.
- **Anti-Pattern (What NOT to do):** Hardcoding `CoInitializeEx` directly into worker thread startup functions.
- **Architectural Solution (What to DO):** Abstract initialization behind a pure-Rust SPI trait with an associated RAII guard:
  ```rust
  pub trait ComInitializer: Send + Sync + 'static {
      type Guard: 'static;
      fn initialize(&self) -> OpcResult<Self::Guard>;
  }

  #[cfg(feature = "opc-da-backend")]
  pub struct DefaultComInit; // Calls CoInitializeEx, returns ComGuard

  pub struct NoOpComInit; // Returns (), compiles unconditionally
  impl ComInitializer for NoOpComInit {
      type Guard = ();
      fn initialize(&self) -> OpcResult<()> { Ok(()) }
  }
  ```
- **Enforcement & Invariant:** Enables `--no-default-features` compilation on all targets (Gate 5).

---

### 5.4 Pure-Rust 128-bit CLSID Domain Type
- **Keywords / Search Tokens:** `Clsid`, `GUID`, `RFC-4122`, `pure rust domain`, `--no-default-features`, `Windows SDK decoupling`
- **Category:** Boundary Hygiene & Platform Isolation
- **Problem Statement:** Using `windows::core::GUID` throughout high-level domain types leaks Windows SDK dependencies into pure-Rust domain layers, breaking `--no-default-features` compilation.
- **Root Cause & Technical Dynamics:** Direct coupling of domain types to external vendor SDK types forces all downstream crates and features to depend on those SDKs.
- **Anti-Pattern (What NOT to do):** Using `windows::core::GUID` in core domain models.
- **Architectural Solution (What to DO):** Define a pure-Rust 128-bit `Clsid` (`#[repr(C)] struct Clsid([u8; 16])`) with RFC-4122 parsing, formatting, and unconditional bidirectional conversion to `windows::core::GUID`.
- **Enforcement & Invariant:** Tested in `tests/clsid_test.rs` under `--no-default-features`.

---

### 5.5 Direct Proxy Blanketing vs `QueryInterface` Ephemeral Allocation (Windows KB5004442)
- **Keywords / Search Tokens:** `KB5004442`, `CVE-2021-26414`, `CoSetProxyBlanket`, `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY`, `QueryInterface`, `E_ACCESSDENIED`, `0x80070005`
- **Category:** Windows DCOM Security & FFI Proxy Topology
- **Problem Statement:** To satisfy modern Windows DCOM hardening (KB5004442 / CVE-2021-26414), remote COM calls require `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY`. When implementing `apply_proxy_blanket`, invoking `.cast::<windows::core::IUnknown>()` triggers an underlying `QueryInterface(&IUnknown::IID)` call. This produces a brand new, independently allocated proxy pointer. Calling `CoSetProxyBlanket` on that temporary pointer configures security on an ephemeral interface that is dropped immediately upon function return. The caller's actual dispatch interface proxy (`IOPCServer`, `IOPCItemMgt`, `IOPCSyncIO`) remains completely unblanketed, causing remote calls to fail with `0x80070005` (`E_ACCESSDENIED`).
- **Root Cause & Technical Dynamics:** In DCOM proxy topology, each `QueryInterface` across apartment/machine boundaries can return a distinct proxy manager or interface proxy instance with its own security blanket settings.
- **Anti-Pattern (What NOT to do):** Calling `proxy.cast::<IUnknown>()` before applying `CoSetProxyBlanket`.
- **Architectural Solution (What to DO):** Re-borrow the concrete proxy's vtable pointer directly as `&windows::core::IUnknown` using raw pointer casting without `QueryInterface`:
  ```rust
  // Re-borrows the EXACT proxy instance without allocating an ephemeral duplicate:
  let unk = unsafe { &*std::ptr::from_ref(proxy).cast::<windows::core::IUnknown>() };
  apply_proxy_blanket(unk)?;
  ```
  This guarantees that DCOM security blankets apply directly to the dispatch proxy used for physical IPC round-trips.
- **Enforcement & Invariant:** Implemented in `opc-da-client/src/com/security.rs`.

---

### 5.6 Interface Segregation Principle (ISP) on COM Interface Graphs
- **Keywords / Search Tokens:** `ISP`, `Interface Segregation Principle`, `IOPCServer`, `IOPCItemMgt`, `IOPCSyncIO`, `E_NOINTERFACE`, `0x80004002`, `DCOM round trips`
- **Category:** COM Resource Optimization & Legacy Compatibility
- **Problem Statement:** Querying and caching secondary COM interfaces (such as `IOPCGroupStateMgt`, `IOPCAsyncIO2`, `IConnectionPointContainer`, `IOPCCommon`, `IOPCItemProperties`) that are never consumed by client logic introduces two severe architectural flaws:
  1. Mandatory fallible casts (`?`) cause connection or group creation to abort with `E_NOINTERFACE` (`0x80004002`) on standard minimal OPC DA 2.05a servers.
  2. Each unread interface query requires a synchronous DCOM `QueryInterface` RPC round-trip over the network, adding 12 redundant network round-trips per session.
- **Root Cause & Technical Dynamics:** Violating ISP by querying all theoretically possible interfaces creates brittleness against minimal server implementations and generates excessive network latency.
- **Anti-Pattern (What NOT to do):** Querying every interface in the OPC DA specification during connection initialization.
- **Architectural Solution (What to DO):** Strictly adhere to the Interface Segregation Principle by querying and retaining only the minimal interfaces required for synchronous telemetry:
  - `ComServer`: Retains strictly `server: IOPCServer` and optional `browse_server_address_space`.
  - `ComGroup`: Retains strictly `item_mgt: IOPCItemMgt` and `sync_io: IOPCSyncIO`.
  This guarantees maximum compatibility with industrial servers while saving ~12 network round-trips per connection.
- **Enforcement & Invariant:** Enforced in `opc-da-client/src/com/server.rs` and `opc-da-client/src/com/group.rs`.

---

### 5.7 Canonical Win32 HRESULT Modeling vs Raw Integer Casts (`clippy::cast_possible_wrap`)
- **Keywords / Search Tokens:** `HRESULT`, `cast_signed`, `clippy::cast_possible_wrap`, `E_FAIL`, `E_ACCESSDENIED`, `0x80004005`, `0x80070005`
- **Category:** Type Safety & Static Analysis Compliance
- **Problem Statement:** In Windows SDK bindings (`windows-core`), `HRESULT` wraps a signed 32-bit integer (`i32`). Win32 error codes with the high error bit set (e.g. `0x8000_4005` for `E_FAIL` or `0x8007_0005` for `E_ACCESSDENIED`) represent positive numbers in unsigned representation. Writing `0x8000_4005u32 as i32` triggers Clippy's `clippy::cast_possible_wrap` lint under `-D warnings`.
- **Root Cause & Technical Dynamics:** Casting large `u32` values to `i32` wraps the value into negative range. While intentional for HRESULT bit patterns, standard `as` casts violate strict lint policies.
- **Anti-Pattern (What NOT to do):** Writing ad-hoc `0x8000_4005u32 as i32` across multiple source files.
- **Architectural Solution (What to DO):** Define canonical constants using `.cast_signed()` in a centralized `hresult` module:
  ```rust
  pub const E_FAIL: HRESULT = HRESULT(0x8000_4005_u32.cast_signed());
  pub const E_ACCESSDENIED: HRESULT = HRESULT(0x8007_0005_u32.cast_signed());
  ```
  Reference these canonical constants workspace-wide instead of ad-hoc integer casting.
- **Enforcement & Invariant:** Verified under `cargo clippy --all-targets -- -D warnings` (Gate 2).

---

## 6. Testing Topology, Determinism & Quality Gate Governance

### 6.1 Sub-Second Failure Cooldown Verification (No `sleep`)
- **Keywords / Search Tokens:** `Circuit breaker test`, `no sleep`, `deterministic test`, `cooldown window`, `instant assertion`, `CI flakiness`
- **Category:** Test Performance & Determinism
- **Problem Statement:** Testing circuit breakers or cooldown windows (e.g. 5-second failure backoff) by introducing real-time `tokio::time::sleep(Duration::from_millis(5100))` drastically slows down test suites, leading to flaky CI builds and degraded developer feedback loops.
- **Root Cause & Technical Dynamics:** Real-time sleeps couple unit tests to wall-clock time, causing tests to fail on slow or overloaded CI runners.
- **Anti-Pattern (What NOT to do):** Using real-time sleeps in automated tests to wait out cooldown timers.
- **Architectural Solution (What to DO):** Test circuit breakers instantaneously by asserting immediate rejection on back-to-back requests executed within the active window without sleeping.
- **Enforcement & Invariant:** Executed in `tests/resilience_and_pool_integration_test.rs` with execution time <5ms.

---

### 6.2 Strict Elimination of `Box<dyn Error>` and `anyhow` in Libraries
- **Keywords / Search Tokens:** `Box<dyn Error>`, `anyhow`, `thiserror`, `OpcResult`, `library standards`, `doctest quality`, `forbidden patterns`
- **Category:** Library Engineering Standards
- **Problem Statement:** Using `Box<dyn std::error::Error>` or `anyhow::Result` in library code or doc-test helpers obscures concrete error variants from consumers and defeats static pattern matching.
- **Root Cause & Technical Dynamics:** Trait objects erase type information, preventing consumers from matching on specific domain error conditions.
- **Anti-Pattern (What NOT to do):** Using `Box<dyn Error>` or `anyhow` anywhere in library crates or doc-test boilerplate.
- **Architectural Solution (What to DO):**
  1. Enforce `thiserror` for all library errors and expose strongly-typed `OpcResult<T>`.
  2. Use structural pattern scanners in CI to reject `Box<dyn Error>` even in hidden doc-test runner functions (`/// # fn run() -> opc_da_client::OpcResult<()>`).
- **Enforcement & Invariant:** Enforced via Gate 8 (`scripts/verify.ps1`) AST scan rejecting `Box<dyn Error>` and `anyhow`.

---

### 6.3 Test Topology Normalization: Co-located Unit Tests vs Integrated Suites
- **Keywords / Search Tokens:** `Test topology`, `unit tests`, `integration tests`, `mod tests`, `tests/`, `maintainability`
- **Category:** Maintainability & Clean Testing Architecture
- **Problem Statement:** Placing domain unit tests and mock contract checks into `tests/` inflates integration test counts and splits unit tests away from the code they verify.
- **Root Cause & Technical Dynamics:** Putting private or component-level tests into `tests/` forces unnecessary visibility expansion (`pub`) and obscures which tests belong to which source files.
- **Anti-Pattern (What NOT to do):** Placing micro unit tests for internal types into the root `tests/` directory.
- **Architectural Rule:**
  - **Unit Tests:** Must reside in co-located `mod tests` blocks within their respective source files (e.g. `src/types/handles.rs`, `src/types/batch.rs`).
  - **Integration Tests:** `tests/` is strictly reserved for true multi-subsystem integration suites (`domain_pipeline_test.rs`, `tag_io_integration_test.rs`, `resilience_and_pool_integration_test.rs`) exercising public facade contracts without private access.
- **Enforcement & Invariant:** Audited under Gate 3 and verified during architecture review.

---

### 6.4 Multi-Shell PowerShell Automation Portability (`Join-Path` Positional Constraints)
- **Keywords / Search Tokens:** `Join-Path`, `PowerShell 5.1`, `PowerShell Core 7`, `PositionalParameterNotFound`, `automation portability`, `scripts`
- **Category:** Toolchain Integrity & Windows Shell Compatibility
- **Problem Statement:** In PowerShell Core (`pwsh` 7+), `Join-Path` dynamically accepts three or more positional arguments (`Join-Path $PSScriptRoot ".." "compat"`). However, in Windows PowerShell 5.1 (the default built-in shell on Windows systems), `Join-Path` only defines two positional parameters (`-Path` and `-ChildPath`). Executing a script with 3 positional arguments under Windows PowerShell 5.1 immediately fails with `PositionalParameterNotFound`.
- **Root Cause & Technical Dynamics:** PowerShell Core added an `AdditionalChildPath` parameter set that does not exist in PowerShell 5.1.
- **Anti-Pattern (What NOT to do):** Calling `Join-Path` with 3 or more positional parameters.
- **Architectural Solution (What to DO):** Standardize all path joins to 2-parameter relative paths:
  ```powershell
  $compatDir = Join-Path $PSScriptRoot "..\compat"
  ```
  This guarantees 100% portability across Windows PowerShell 5.1 and PowerShell Core 7+.
- **Enforcement & Invariant:** Validated across all 6 scripts via Gate 9 (`scripts/verify.ps1`).

---

### 6.5 AST-Grep Multi-Line Comment Sibling Token Matching
- **Keywords / Search Tokens:** `ast-grep`, `require-safety-comment`, `line_comment`, `SAFETY:`, `tree-sitter`, `false positive`, `lint rule`
- **Category:** Static Analysis & AST Linter Architecture
- **Problem Statement:** In tree-sitter AST parsers, consecutive comment lines starting with `//` are parsed as individual, distinct `line_comment` nodes. An AST-Grep rule enforcing `require-safety-comment` using a `follows: { kind: line_comment, regex: "SAFETY:" }` selector inspects only the *immediate preceding sibling* comment node. If an unsafe block is preceded by a multi-line explanation where only the first line contains `SAFETY:`, the immediate sibling comment node fails the regex match, triggering a false-positive violation under Gate 6.
- **Root Cause & Technical Dynamics:** AST-Grep sibling selectors evaluate the immediate sibling token. When multiple single-line comments precede a block, only the final line is the immediate sibling.
- **Anti-Pattern (What NOT to do):** Placing `// SAFETY:` only on the first line of a multi-line unsafe block comment.
- **Architectural Solution (What to DO):** Repeat the `// SAFETY:` prefix on every single line of a multi-line safety justification comment:
  ```rust
  // SAFETY: Pointer is non-null and points to an active DCOM proxy.
  // SAFETY: Casting to IUnknown conforms to standard Win32 COM ABI rules.
  let unk = unsafe { &*std::ptr::from_ref(proxy).cast::<windows::core::IUnknown>() };
  ```
- **Enforcement & Invariant:** Verified under Gate 6 AST-Grep scan in `scripts/verify.ps1`.

---

### 6.6 Mock Server State Isolation in Recursive Tree Walks
- **Keywords / Search Tokens:** `MockConnectedServer`, `with_branch_tags`, `state isolation`, `recursive tree walk`, `flaky test`, `test determinism`
- **Category:** Mock Fidelity & Test Suite Determinism
- **Problem Statement:** A mock server double (`MockConnectedServer::default()`) configured with simulated default child branches (e.g. `["Random", "Simulation"]`) will cause recursive walk tests targeting leaf chunking at depth 0 to traverse simulated child branches. If child branches re-enumerate the same leaf nodes, tests encounter unexpected item counts and premature capacity exhaustion.
- **Root Cause & Technical Dynamics:** Default mock fixtures configured with hierarchical branches cause recursive algorithms to explore child nodes unless child branches are explicitly cleared.
- **Anti-Pattern (What NOT to do):** Running leaf-traversal unit tests against mock servers with uncleared default branch fixtures.
- **Architectural Solution (What to DO):** In unit and integration tests verifying leaf batching and traversal mechanics, explicitly chain builder methods to reset simulated child branch tags:
  ```rust
  let server = MockConnectedServer::default()
      .with_tags(leaf_tags)
      .with_branch_tags(Vec::new()) // Isolates depth-0 walk from mock branch re-traversal
      .with_organization(ServerOrganization::Hierarchical);
  ```
- **Enforcement & Invariant:** Verified in `tests/browse_integration_test.rs`.
