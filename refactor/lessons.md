# Architectural Lessons Learned: `opc-da-client` Modernization & ICS Systems

**Scope:** Comprehensive catalog of architectural patterns, compiler dynamics, concurrency rules, industrial control safety invariants, and Windows COM/FFI caveats discovered during the modernization of `opc-da-client`.  
**Target Destination:** Intermediate staging repository for indexing into `knowledge-rag` MCP.  
**Author:** Architect  
**Date:** 2026-09-18 (Updated for Cycle 2 Modernization)  
**Status:** Certified & Verified against Rust 2024 (MSRV 1.93.1), Windows COM/DCOM, and Tokio Runtime.

---

## Table of Contents
1. [Rust Type System & Trait Invariants](#1-rust-type-system--trait-invariants)
   - 1.1 [The Blanket `TryFrom` vs `From` Orphan Collision](#11-the-blanket-tryfrom-vs-from-orphan-collision)
   - 1.2 [Eliminating the "Boolean Trap" with Domain Enums](#12-eliminating-the-boolean-trap-with-domain-enums)
   - 1.3 [Compile-Time Typestate Encapsulation & Role Trait Boundaries](#13-compile-time-typestate-encapsulation--role-trait-boundaries)
   - 1.4 [Native Rust 2024 AFIT (Async Fn in Trait)](#14-native-rust-2024-afit-async-fn-in-trait)
   - 1.5 [Associated Function Type Inference Ambiguity (`E0283`) with Default Generic Parameters](#15-associated-function-type-inference-ambiguity-e0283-with-default-generic-parameters)
   - 1.6 [Intermediate Type Inference Collisions on Generic Collection Conversions (`E0283`)](#16-intermediate-type-inference-collisions-on-generic-collection-conversions-e0283)
   - 1.7 [Rust 2024 Anonymous Lifetimes in Associated Type Bounds (`E0658`)](#17-rust-2024-anonymous-lifetimes-in-associated-type-bounds-e0658)
   - 1.8 [Semantic Sequence Equality vs Derived Representation Equality](#18-semantic-sequence-equality-vs-derived-representation-equality)
   - 1.9 [Trait Object Safety Preservation Across Facade and Role Layers](#19-trait-object-safety-preservation-across-facade-and-role-layers)
2. [Concurrency, Backpressure & Background Worker Resilience](#2-concurrency-backpressure--background-worker-resilience)
   - 2.1 [Preserving Channel Backpressure Across Async/Sync Boundaries](#21-preserving-channel-backpressure-across-asyncsync-boundaries)
   - 2.2 [Transparent Panic Recovery with Explicit In-Flight Request Drainage](#22-transparent-panic-recovery-with-explicit-in-flight-request-drainage)
   - 2.3 [Early Cancellation Check (Avoiding Wasted FFI / RPC)](#23-early-cancellation-check-avoiding-wasted-ffi--rpc)
   - 2.4 [Amortizing Mutex Contention via Chunked Accumulation](#24-amortizing-mutex-contention-via-chunked-accumulation)
   - 2.5 [Reader-Writer Concurrency Decoupling for Streaming Accumulators (`RwLock` vs `Mutex`)](#25-reader-writer-concurrency-decoupling-for-streaming-accumulators-rwlock-vs-mutex)
   - 2.6 [Symmetrical Poison Recovery Across Lock Acquisition Sites](#26-symmetrical-poison-recovery-across-lock-acquisition-sites)
   - 2.7 [RAII Unwind Safety in Streaming Iterators (`PushBatchGuard`)](#27-raii-unwind-safety-in-streaming-iterators-pushbatchguard)
   - 2.8 [Zero-Allocation Hot-Path Request Dispatch Across `catch_unwind`](#28-zero-allocation-hot-path-request-dispatch-across-catch_unwind)
3. [Memory Safety, Allocation Budgets & Zero-Allocation Patterns](#3-memory-safety-allocation-budgets--zero-allocation-patterns)
   - 3.1 [Static Small Array Optimization (`StaticSmall`)](#31-static-small-array-optimization-staticsmall)
   - 3.2 [UTF-8 Multibyte Slicing Safety in Inline SSO Buffers](#32-utf-8-multibyte-slicing-safety-in-inline-sso-buffers)
   - 3.3 [Double-Panic Containment in RAII Drops](#33-double-panic-containment-in-raii-drops)
   - 3.4 [Eliminating Sentinel Allocations on Hot Paths](#34-eliminating-sentinel-allocations-on-hot-paths)
   - 3.5 [In-Place Vector Buffer Draining (`chunk.drain(..)`) vs Reallocation](#35-in-place-vector-buffer-draining-chunkdrain-vs-reallocation)
   - 3.6 [Lazy Slot Allocation Buffers vs Eager Dummy Records](#36-lazy-slot-allocation-buffers-vs-eager-dummy-records)
   - 3.7 [31-Byte Stack Small String Optimization (SSO) with Zero-Padding 72-Byte Layout](#37-31-byte-stack-small-string-optimization-sso-with-zero-padding-72-byte-layout)
   - 3.8 [Zero-Copy Terminal Data Handoff via `std::mem::take`](#38-zero-copy-terminal-data-handoff-via-stdmemtake)
4. [Industrial Control Systems (ICS) & PLC Actuation Safety](#4-industrial-control-systems-ics--plc-actuation-safety)
   - 4.1 [Write Non-Idempotency & Duplicate Actuation Hazards](#41-write-non-idempotency--duplicate-actuation-hazards)
   - 4.2 [Active Group Poisoning on Array Length Mismatch](#42-active-group-poisoning-on-array-length-mismatch)
   - 4.3 [Precondition Validation Prior to OS Thread & Apartment Allocation](#43-precondition-validation-prior-to-os-thread--apartment-allocation)
   - 4.4 [Granular Industrial Input Quarantine vs Cascading Batch Abort (`CWE-626` / `CWE-400`)](#44-granular-industrial-input-quarantine-vs-cascading-batch-abort-cwe-626--cwe-400)
   - 4.5 [Two-Stage Positional Index Mapping Under Granular Defensive Screening](#45-two-stage-positional-index-mapping-under-granular-defensive-screening)
   - 4.6 [Reconnection Retry Accumulator Clean Slate on Idempotent Browse](#46-reconnection-retry-accumulator-clean-slate-on-idempotent-browse)
5. [Windows COM / DCOM FFI Boundaries & Win32 Peculiarities](#5-windows-com--dcom-ffi-boundaries--win32-peculiarities)
   - 5.1 [CWE-626 Null-Byte Injection in COM Wide Strings](#51-cwe-626-null-byte-injection-in-com-wide-strings)
   - 5.2 [Cryptic Win32 RPC Disconnect Classification](#52-cryptic-win32-rpc-disconnect-classification)
   - 5.3 [Decoupling Windows COM via Pluggable Initializers (`NoOpComInit`)](#53-decoupling-windows-com-via-pluggable-initializers-noopcominit)
   - 5.4 [Pure-Rust 128-bit CLSID Domain Type](#54-pure-rust-128-bit-clsid-domain-type)
   - 5.5 [Direct Proxy Blanketing vs `QueryInterface` Ephemeral Allocation (Windows KB5004442)](#55-direct-proxy-blanketing-vs-queryinterface-ephemeral-allocation-windows-kb5004442)
   - 5.6 [Interface Segregation Principle (ISP) on COM Interface Graphs](#56-interface-segregation-principle-isp-on-com-interface-graphs)
   - 5.7 [Canonical Win32 HRESULT Modeling vs Raw Integer Casts (`clippy::cast_possible_wrap`)](#57-canonical-win32-hresult-modeling-vs-raw-integer-casts-clippycast_possible_wrap)
6. [Testing Topology, Determinism & Quality Gate Governance](#6-testing-topology-determinism--quality-gate-governance)
   - 6.1 [Sub-Second Failure Cooldown Verification (No `sleep`)](#61-sub-second-failure-cooldown-verification-no-sleep)
   - 6.2 [Strict Elimination of `Box<dyn Error>` and `anyhow` in Libraries](#62-strict-elimination-of-boxdyn-error-and-anyhow-in-libraries)
   - 6.3 [Test Topology Normalization: Co-located Unit Tests vs Integrated Suites](#63-test-topology-normalization-co-located-unit-tests-vs-integrated-suites)
   - 6.4 [Multi-Shell PowerShell Automation Portability (`Join-Path` Positional Constraints)](#64-multi-shell-powershell-automation-portability-join-path-positional-constraints)
   - 6.5 [AST-Grep Multi-Line Comment Sibling Token Matching](#65-ast-grep-multi-line-comment-sibling-token-matching)
   - 6.6 [Mock Server State Isolation in Recursive Tree Walks](#66-mock-server-state-isolation-in-recursive-tree-walks)


---

## 1. Rust Type System & Trait Invariants

### 1.1 The Blanket `TryFrom` vs `From` Orphan Collision
- **Category:** Rust Language Invariants & Trait Coherence
- **Problem Statement:** Attempting to implement `impl TryFrom<&str> for MyType` when `impl From<&str> for MyType` already exists causes compiler error `E0119` (conflicting trait implementations).
- **Root Cause & Compiler Dynamics:** The Rust standard library `core::convert` includes the blanket implementation:
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
- **Architectural Solution:**
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

### 1.2 Eliminating the "Boolean Trap" with Domain Enums
- **Category:** API Ergonomics & Call-Site Clarity
- **Problem Statement:** Functions accepting multiple boolean flags (e.g. `dispatch_with_retry(pool, connector, ep, true, op)`) suffer from the "boolean trap"—callers cannot understand what `true` signifies without reading function signatures, and inverted boolean flags easily slip past code review.
- **Anti-Pattern:**
  ```rust
  // Ambiguous: Does `true` mean retry? Is it idempotent? Is it recursive?
  dispatch_with_retry(&mut pool, &connector, &endpoint, true, |_| Ok(()));
  ```
- **Architectural Solution:** Replace primitive boolean parameters with dedicated two-variant domain enums:
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

### 1.3 Compile-Time Typestate Encapsulation & Role Trait Boundaries
- **Category:** Domain Modeling & Type Safety
- **Problem Statement:** A multi-role client facade implementing generic role traits (e.g. `TagReader`, `TagWriter`, `TagBrowser`) can accidentally allow a session bound to `ServerA` to execute queries against `ServerB` if the trait signatures accept a server identifier.
- **Root Cause:** Standard role traits take `server: impl Into<ServerIdentifier>` to allow multi-server querying from unbound clients. When implemented on a `Bound` client, this creates an invariant hazard.
- **Architectural Solution:**
  1. Separate client typestates at compile time: `OpcDaClient<C, Unbound>` (gateway) vs `OpcDaClient<C, Bound>` (dedicated session).
  2. In role trait implementations on `Bound`, implement an explicit endpoint verification check (`validate_bound_server`) comparing requested hosts and ProgIDs against `self.state.endpoint`, returning `OpcError::InvalidState` on mismatch.
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

### 1.4 Native Rust 2024 AFIT (Async Fn in Trait)
- **Category:** Async Performance & Zero Allocation
- **Problem Statement:** Pre-Rust 2024 codebases rely on `#[async_trait]`, which desugars every async method into `Pin<Box<dyn Future<Output = ...> + Send + '_>>`. On high-frequency polling loops (e.g. reading 100 tags every 10ms), this incurs continuous heap allocation and vtable indirection.
- **Architectural Solution:** Migrate to native Rust 2024 async traits returning `impl Future<Output = ...> + Send`:
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

### 1.5 Associated Function Type Inference Ambiguity (`E0283`) with Default Generic Parameters
- **Category:** Rust Language Invariants & Generic Constructors
- **Problem Statement:** When a generic struct `OpcDaClientBuilder<C = DefaultBackendConnector>` implements `impl<C: ServerBackend + Default> OpcDaClientBuilder<C> { pub fn new() -> Self }`, calling `OpcDaClientBuilder::new()` fails with compiler error `E0283` ("type annotations needed for `C`").
- **Root Cause & Compiler Dynamics:** Default generic type parameters on struct declarations (`struct Foo<C = DefaultType>`) apply only when referring to the type without parameters in type position. In expression position, calling an associated function `Foo::new()` defined on a generic `impl<C>` block does not automatically constrain `C` to the default type parameter if multiple types implement `ServerBackend + Default`.
- **Anti-Pattern:**
  ```rust
  // FAILS TO COMPILE (E0283) at call site `OpcDaClientBuilder::new()`:
  impl<C: ServerBackend + Default> OpcDaClientBuilder<C> {
      pub fn new() -> Self {
          Self { connector: C::default(), ... }
      }
  }
  ```
- **Architectural Solution:** Implement `new()` directly on the concrete default type parameter, while providing generic construction via `Default` or explicit constructor arguments:
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

### 1.6 Intermediate Type Inference Collisions on Generic Collection Conversions (`E0283`)
- **Category:** Rust Type Solver & Trait Ergonomics
- **Problem Statement:** Implementing generic collection conversions such as `impl<S, V> IntoWriteBatch for Vec<(S, V)> where S: Into<String> + Send, V: Into<OpcValue> + Send` causes callers chaining `.into()` on string literals (e.g. `vec![("Tag.1".into(), val)].into_write_batch()`) to fail with `E0283` ("type annotations needed").
- **Root Cause:** When `"Tag.1".into()` is invoked, the compiler must resolve an intermediate type `S` such that `&str: Into<S>` and `S: Into<String>`. Because multiple standard types satisfy this relationship (`String`, `Cow<'_, str>`, `Box<str>`, `&str`), the trait solver cannot infer a unique type for `S`.
- **Architectural Solution:** Provide concrete types directly in collection fixtures (e.g. string slices `("Tag.1", val)` or owned `("Tag.1".to_string(), val)`). Because `&str` satisfies `Into<String> + Send` directly, the compiler immediately infers `S = &'static str` without type ambiguity or intermediate allocations.

### 1.7 Rust 2024 Anonymous Lifetimes in Associated Type Bounds (`E0658`)
- **Category:** Rust 2024 / MSRV 1.93.1 Lifetime Rules
- **Problem Statement:** In Rust 2024, declaring a function parameter using `impl Trait` with an elided reference lifetime in an associated type bound (e.g. `tags: impl ExactSizeIterator<Item = &str>`) triggers compiler error `E0658` ("anonymous lifetimes in `impl Trait` are unstable").
- **Architectural Solution:** Introduce an explicit named lifetime parameter in the function signature:
  ```rust
  // Stable in Rust 2024 / MSRV 1.93.1:
  pub(crate) fn assemble_tag_values<'a>(
      tags: impl ExactSizeIterator<Item = &'a str>,
      results: Vec<GroupItemResult>,
  ) -> OpcResult<TagValues> { ... }
  ```

### 1.8 Semantic Sequence Equality vs Derived Representation Equality
- **Category:** Domain Model Design & Mathematical Equivalence
- **Problem Statement:** Encapsulated collection types that support multiple storage representations (e.g. 31-byte stack SSO `InlineSingle`, static literals `StaticSingle`, owned strings `OwnedSingle`, and slices `Owned`) produce false negatives under derived `PartialEq`. An inline single write does not match an owned single write under discriminant comparison, even though their tag names and values are identical.
- **Architectural Solution:** Implement manual sequence `PartialEq` comparing sequence length and borrowed iterator items:
  ```rust
  impl PartialEq for WriteBatch {
      fn eq(&self, other: &Self) -> bool {
          self.len() == other.len() && self.iter().eq(other.iter())
      }
  }
  ```
  This guarantees representation-independent equality across all $5 \times 5 = 25$ cross-variant permutations.

### 1.9 Trait Object Safety Preservation Across Facade and Role Layers
- **Category:** API Architecture & Object Safety
- **Problem Statement:** Adding generic conversion parameters (e.g. `tags: impl IntoTags`, `writes: impl IntoWriteBatch`) directly to public SPI traits (`TagReader`, `TagWriter`, `OpcProvider`) permanently destroys trait object safety (`dyn TagReader`), making dynamic dispatch and mock generation (`mockall`) impossible.
- **Architectural Solution:**
  1. Keep SPI role traits strictly concrete: `fn write_tag_batch(&self, server: &ServerIdentifier, batch: WriteBatch)`.
  2. Implement ergonomic generic polymorphism (`impl IntoWriteBatch`) on inherent facade methods on `OpcDaClient`:
     ```rust
     impl<C: ServerBackend, State> OpcDaClient<C, State> {
         pub async fn write_tags(&self, server: impl TryInto<ServerIdentifier>, writes: impl IntoWriteBatch) -> OpcResult<Vec<WriteResult>> {
             let batch = writes.into_write_batch();
             // dispatch concrete batch to SPI role trait...
         }
     }
     ```

---

## 2. Concurrency, Backpressure & Background Worker Resilience

### 2.1 Preserving Channel Backpressure Across Async/Sync Boundaries
- **Category:** Concurrency Safety & Memory Bounding
- **Problem Statement:** An async Tokio task sends requests to a dedicated background OS worker thread via a bounded channel (`tokio::sync::mpsc::channel(32)`). To optimize throughput, the worker thread event loop drains incoming requests using `while let Ok(req) = rx.try_recv()`. Under sustained traffic bursts, this completely nullifies Tokio's backpressure, allowing the worker's internal priority queue to grow without bound until out-of-memory (OOM) occurs.
- **Anti-Pattern:**
  ```rust
  // DANGEROUS: Unbounded drain bypasses channel capacity:
  while let Ok(pending) = rx.try_recv() {
      queue.push(pending);
  }
  ```
- **Architectural Solution:** Enforce a strict compile-time queue depth ceiling (`MAX_QUEUE_DEPTH = 64`):
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

### 2.2 Transparent Panic Recovery with Explicit In-Flight Request Drainage
- **Category:** Concurrency Diagnostics & Error Provenance
- **Problem Statement:** When a synchronous COM call panics inside a background worker thread, `std::panic::catch_unwind` recovers the event loop. However, calling `queue.clear()` drops all pending requests. Awaiting Tokio tasks receive `oneshot::error::RecvError`, which maps to a misleading `WorkerTerminated` error even though the worker survived and is ready for traffic.
- **Architectural Solution:** Implement `drain_and_reject` on the priority queue to reply to all in-flight oneshot senders with an explicit `WorkerError::Panic`:
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

### 2.3 Early Cancellation Check (Avoiding Wasted FFI / RPC)
- **Category:** Performance & Resource Conservation
- **Problem Statement:** In high-concurrency systems, callers frequently set timeouts via `tokio::time::timeout`. If a caller times out or drops its receiver while a request is sitting in the worker queue, dispatching expensive synchronous COM RPC roundtrips wastes CPU, network, and COM proxy resources.
- **Architectural Solution:** Check `reply.is_closed()` immediately before initiating synchronous dispatch:
  ```rust
  if reply.is_closed() {
      tracing::debug!(op, server = %endpoint, "Caller cancelled request; skipping dispatch");
      return;
  }
  ```

### 2.4 Amortizing Mutex Contention via Chunked Accumulation
- **Category:** Concurrency Performance
- **Problem Statement:** When browsing large industrial server namespaces (e.g. 50,000 tags), acquiring a shared accumulator mutex (`Arc<Mutex<TagCollector>>`) per individual leaf tag causes severe lock contention, degrading throughput to <500 tags/sec.
- **Architectural Solution:** Buffer items in a local stack/thread-local buffer and flush in 256-item batches via `collector.push_batch(std::mem::replace(&mut chunk, Vec::with_capacity(256)))`.
   - Reduces mutex acquisitions by **99.6%**.
   - Provides a natural checkpoint for cooperative cancellation (`if collector.is_cancelled() || collector.is_full() { break; }`).

### 2.5 Reader-Writer Concurrency Decoupling for Streaming Accumulators (`RwLock` vs `Mutex`)
- **Category:** Concurrency Safety & Read Scalability
- **Problem Statement:** Using `std::sync::Mutex<Vec<String>>` to protect a shared tag accumulator forces exclusive lock acquisition even for read-only observer queries (e.g. `collector.snapshot()`). Holding an exclusive lock while deep-cloning 10,000 strings serializes concurrent readers (TUI rendering, CLI telemetry, monitoring threads) and completely stalls the background MTA ingestion worker.
- **Architectural Solution:**
  1. Upgrade storage to `std::sync::RwLock<Vec<String>>`.
  2. Implement atomic lock-free queries (`AtomicUsize` for `len()`, `AtomicBool` for `is_cancelled()`, `is_full()`).
  3. `snapshot()` acquires a shared read lock (`read()`), allowing unlimited parallel observer reads while background ingestion acquires write locks (`write()`) only for brief chunk flushes.

### 2.6 Symmetrical Poison Recovery Across Lock Acquisition Sites
- **Category:** Concurrency Resilience & Lock Recovery
- **Problem Statement:** When a worker or observer thread panics while holding a standard lock, Rust marks the lock as poisoned. Calling `.unwrap()` or `.expect()` on poisoned locks cascades panics to healthy observer threads, causing entire applications to crash.
- **Architectural Solution:** Implement uniform symmetrical poison recovery across all lock acquisition sites (`clear`, `snapshot`, `harvest`, `push`, `push_batch`):
  ```rust
  let mut guard = match self.inner.tags.write() {
      Ok(g) => g,
      Err(poisoned) => poisoned.into_inner(),
  };
  // Resynchronize atomic count with physical buffer length:
  self.inner.count.store(guard.len(), Ordering::Release);
  ```

### 2.7 RAII Unwind Safety in Streaming Iterators (`PushBatchGuard`)
- **Category:** Concurrency Invariants & Unwind Protection
- **Problem Statement:** In streaming methods accepting arbitrary iterators (`push_batch(iter: impl IntoIterator<Item = String>)`), if an untrusted third-party iterator panics midway through iteration, items already appended to the internal buffer remain uncounted in atomic telemetry (`count`), corrupting subsequent size and capacity checks.
- **Architectural Solution:** Wrap streaming ingestion in an RAII drop guard:
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

### 2.8 Zero-Allocation Hot-Path Request Dispatch Across `catch_unwind`
- **Category:** Hot-Path Allocation Optimization
- **Problem Statement:** Dispatching background requests through `std::panic::catch_unwind(AssertUnwindSafe(...))` often tempts engineers to pre-clone endpoints or pre-format host strings so they are accessible in the panic recovery block, creating heap allocations on 100% of happy-path dispatches.
- **Architectural Solution:** Borrow the endpoint reference `&OpcServerEndpoint` across the `AssertUnwindSafe` closure boundary. Restrict all string formatting and heap allocations strictly to the cold panic branch:
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

---

## 3. Memory Safety, Allocation Budgets & Zero-Allocation Patterns

### 3.1 Static Small Array Optimization (`StaticSmall`)
- **Category:** Zero-Allocation Hot Paths
- **Problem Statement:** Polling loops frequently read small, fixed sets of tags (e.g. `["Sensor1", "Sensor2"]`). Standard implementations convert array literals into heap-allocated `Vec<String>`, causing millions of short-lived allocations per minute.
- **Architectural Solution:** Define a multi-tier `TagBatchRepr` containing an inline array variant for $N \le 4$:
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

### 3.2 UTF-8 Multibyte Slicing Safety in Inline SSO Buffers
- **Category:** Memory Safety & Panic Prevention
- **Problem Statement:** When storing small strings inline in fixed-size byte buffers (e.g. 31 bytes), truncating strings that contain multibyte UTF-8 characters (e.g. Cyrillic, kanji, accents) at a fixed byte length will slice through a code point, causing `std::str::from_utf8` to fail or panic.
- **Architectural Solution:** Use `std::str::from_utf8` combined with `Utf8Error::valid_up_to` to safely fallback to the largest valid character boundary:
  ```rust
  match std::str::from_utf8(&self.bytes[..len]) {
      Ok(s) => s,
      Err(e) => {
          let valid_len = e.valid_up_to();
          std::str::from_utf8(&self.bytes[..valid_len]).unwrap_or("")
      }
  }
  ```

### 3.3 Double-Panic Containment in RAII Drops
- **Category:** Crash Prevention & Unwind Safety
- **Problem Statement:** Rust aborts the entire process if a panic occurs while another panic is unwinding the stack. If a resource drop guard (e.g. `GroupGuard`, `BrowsePositionGuard`, or `ConnectionPool::clear`) encounters a COM RPC fault and panics during an active unwind, the application dies instantly.
- **Architectural Solution:** Wrap all foreign drop operations in `std::panic::catch_unwind`:
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

### 3.4 Eliminating Sentinel Allocations on Hot Paths
- **Category:** Performance Optimization
- **Problem Statement:** In read operations, initializing results by pre-populating a vector with placeholder `"Not read"` error strings forces unnecessary string allocations on every single tag read cycle, even when reads succeed immediately.
- **Architectural Solution:** Pre-allocate exact vector capacity and use single-pass construction mapping COM item state records directly to `TagValue` without intermediate sentinel values.

### 3.5 In-Place Vector Buffer Draining (`chunk.drain(..)`) vs Reallocation
- **Category:** Memory Churn Elimination & High-Throughput Buffering
- **Problem Statement:** In chunked streaming operations (e.g. flat namespace browsing), replacing the local accumulator at batch boundaries via `std::mem::replace(&mut chunk, Vec::with_capacity(256))` allocates and deallocates a new 6 KiB buffer on every single flush. Over 10,000 tags, this creates ~40 throwaway vectors (~240 KB of garbage collection / allocator churn).
- **Architectural Solution:** Use `chunk.drain(..)` into receiver traits accepting `impl IntoIterator<Item = T>`:
  ```rust
  // Reuses the same physical buffer across thousands of flushes:
  collector.push_batch(chunk.drain(..));
  ```
  Note: Clippy nursery lint `iter_with_drain` suggests `.into_iter()`, but `.into_iter()` deallocates the vector buffer. A scoped `#[allow(clippy::iter_with_drain)]` is appropriate when retaining vector buffer capacity across hot loops.

### 3.6 Lazy Slot Allocation Buffers vs Eager Dummy Records
- **Category:** Allocator Efficiency & Industrial Actuation Optimization
- **Problem Statement:** In batch write operations, pre-populating a result vector with eager dummy failure records (`WriteResult::failure(*tag_id, OpcError::InvalidState("..."))`) allocates $2N$ throwaway heap strings ($N$ cloned tag names $+ N$ error strings). On a 10,000-tag batch where 100% of writes succeed, this allocates 20,000 strings only to overwrite and drop them immediately, generating 50,005 allocator calls.
- **Architectural Solution:** Initialize a slot buffer of `Vec<Option<WriteResult>> = vec![None; items.len()]`. Successful or quarantined write results populate slots lazily upon completion, reducing allocator calls by **80.0%** (from 50,005 to 10,004) and completely eliminating dead store string allocations.

### 3.7 31-Byte Stack Small String Optimization (SSO) with Zero-Padding 72-Byte Layout
- **Category:** Cache Budgeting & Zero-Allocation Primitives
- **Problem Statement:** High-frequency control loops (e.g. 50 Hz PLC setpoints) commanding scalar tag writes generate thousands of heap allocations per minute if the domain primitive encapsulates an owned `String`.
- **Architectural Solution:** Implement an opaque `WriteBatch` backed by a 5-variant `WriteBatchRepr` featuring `InlineSingle([u8; 31], u8, OpcValue)`.
  - Tags $\le 31$ bytes reside entirely in a 32-byte inline stack buffer (31 data bytes $+ 1$ length byte).
  - Memory layout on `x86_64` is exactly 72 bytes with zero internal padding bytes.
  - Safe UTF-8 extraction via `inline_as_str()` is backed by constructor validation, eliminating codepoint tearing hazards (`CWE-20/787`).
  - Completely eliminates 3,000 heap allocations per minute in 50 Hz control loops.

### 3.8 Zero-Copy Terminal Data Handoff via `std::mem::take`
- **Category:** Move Semantics & Terminal Lifecycle Optimization
- **Problem Statement:** Long-running accumulation tasks (such as namespace browsing) that return collected tags via `collector.snapshot()` perform a deep copy of the entire collection (10,001 heap allocations, ~860 KB memory churn for 10k tags), leaving duplicate zombie strings pinned inside the collector instance.
- **Architectural Solution:** Provide a dedicated `harvest()` method utilizing `std::mem::take(&mut *guard)`:
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

---

## 4. Industrial Control Systems (ICS) & PLC Actuation Safety

### 4.1 Write Non-Idempotency & Duplicate Actuation Hazards
- **Category:** Industrial Functional Safety
- **Problem Statement:** In traditional HTTP/REST services, connection drops are often transparently retried. In industrial automation, retrying a write operation (`IOPCSyncIO::Write`) across a disconnected or timed-out COM connection can cause **duplicate physical actuations** on PLCs (e.g. dispensing double raw material, toggling a motor twice, or pulsing a valve).
- **Architectural Invariant:**
  - **Reads, Browsing, Pings (`RetryPolicy::Idempotent`):** Stale connection is evicted; a fresh connection is established transparently and the read is re-executed.
  - **Single & Batch Writes (`RetryPolicy::NonIdempotent`):** Stale connection is evicted from the pool to protect subsequent calls, but the write operation **MUST FAIL FAST** and return the error to the caller. The application layer must decide whether physical state permits a re-write.

### 4.2 Active Group Poisoning on Array Length Mismatch
- **Category:** State Machine Resilience
- **Problem Statement:** OPC DA servers return arrays of item states corresponding to registered item handles. If a faulty or misconfigured server returns an item state array whose length does not match the registered tags, propagating the error via `?` leaves the active group cached in the client connection pool.
- **Consequence:** Every subsequent read hitting that cached group will continuously fail with array index out-of-bounds or length mismatches, causing persistent outage until application restart.
- **Architectural Solution:** Before returning an error on state length mismatch, explicitly evict and destroy the cached group (`pooled.clear_active_group()`).

### 4.3 Precondition Validation Prior to OS Thread & Apartment Allocation
- **Category:** Resource Allocation Safety
- **Problem Statement:** A client builder (`OpcDaClientBuilder::build_bound`) that initializes the COM worker thread before checking whether a server endpoint was configured spawns a dedicated OS thread and initializes a Windows COM MTA apartment, only to fail immediately afterwards.
- **Architectural Solution:** Validate all required configuration parameters (server identity, timeout constraints, batch limits) *prior* to allocating OS resources:
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

### 4.4 Granular Industrial Input Quarantine vs Cascading Batch Abort (`CWE-626` / `CWE-400`)
- **Category:** Industrial Functional Safety & Availability
- **Problem Statement:** If a batch write operation validates tag strings by failing fast via `?` on the first invalid tag (e.g. containing an interior null byte `\0`), the entire batch is immediately aborted. In an industrial control system commanding multiple physical actuators (e.g. tank valves, safety interlocks, motor setpoints), a single contaminated tag would starve all valid commands in the batch, causing industrial process halts or safety hazards.
- **Architectural Solution:** Implement granular item-level quarantine at Entry Gate 1:
  1. Inspect each tag string individually during initial partitioning.
  2. If a tag contains `\0`, isolate it directly into `write_results[orig_idx] = Some(WriteResult::failure(...))` with structured warning telemetry (`tracing::warn!(..., tag = %tag_id.escape_debug())`).
  3. Valid tags in the batch proceed without interruption to COM item registration and write execution.
  4. If and only if 100% of tags in the batch are quarantined, short-circuit immediately before allocating an ephemeral COM group.

### 4.5 Two-Stage Positional Index Mapping Under Granular Defensive Screening
- **Category:** Data Integrity & State Attribution
- **Problem Statement:** When defensive filtering isolates invalid items prior to COM registration, array sizes diverge (`registered_items.len() < requested_items.len()`). Using direct indexing (`results[idx]`) corrupts result attribution, assigning the write status of `Tag3` to `Tag2`.
- **Architectural Solution:** Establish two-stage positional index mapping:
  - **Stage 1 (`valid_orig_indices: Vec<usize>`):** Maps valid screened inputs to their original batch positions.
  - **Stage 2 (`valid_write_orig_indices: Vec<usize>`):** Maps successfully registered COM item handles to their original batch positions.
  Using `.zip()` pairing and array parity validation ensures strict 1:1 positional correspondence between requested tag order and returned result vectors, regardless of arbitrary partial rejections.

### 4.6 Reconnection Retry Accumulator Clean Slate on Idempotent Browse
- **Category:** State Machine Resilience & Data Integrity
- **Problem Statement:** When an idempotent namespace browse operation (`ComRequest::BrowseTags`) encounters a transient DCOM disconnection (e.g. `RPC_S_SERVER_UNAVAILABLE`), the connection pool automatically reconnects and re-dispatches the request with the *same* `TagCollector` accumulator instance. If the handler does not clear the accumulator on entry, tags accumulated prior to the drop remain in the buffer, appending duplicate entries from the root and prematurely hitting `max_tags` capacity limits.
- **Architectural Solution:** Enforce a clean slate entry guard in idempotent browse handlers:
  ```rust
  if !collector.is_empty() {
      tracing::debug!("Clearing partial browse collector accumulator prior to retry traversal");
      collector.clear();
  }
  ```
  This guarantees that retries start with a pristine accumulator and zero duplicate items.

---

## 5. Windows COM / DCOM FFI Boundaries & Win32 Peculiarities

### 5.1 CWE-626 Null-Byte Injection in COM Wide Strings
- **Category:** Application Security & FFI Integrity
- **Problem Statement:** Win32 COM APIs consume null-terminated wide string pointers (`PWSTR`, `PCWSTR`). In Rust, a `&str` or `String` can contain embedded interior null bytes (`\0`). When converting a Rust string with interior nulls into a wide string via pointer casting, the COM FFI function silently truncates the string at the first `\0`.
- **Exploitation / Hazard:** An attacker or misconfigured tag name (`"Sensors.Temperature\0.PrivilegedValve"`) can bypass tag namespace authorization checks or cause buffer miscalculations.
- **Architectural Solution:** Hardened string conversion rejecting interior null bytes:
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

### 5.2 Cryptic Win32 RPC Disconnect Classification
- **Category:** COM Error Handling & Diagnostic Mapping
- **Problem Statement:** When an out-of-process or remote OPC DA server crashes or network connectivity drops, Windows COM calls return raw HRESULT codes:
  - `0x800706BA` (`RPC_S_SERVER_UNAVAILABLE`): Server died or host unreachable.
  - `0x800706BE` (`RPC_S_CALL_FAILED`): RPC call was executed but communication link broken during transmission.
- **Diagnostic Trap:** These errors often originate from internal `IUnknown` or `IOPCServer` interface calls and are encapsulated within `OpcError::Server(msg, code)`. If `OpcError::is_connection_error` only checks `OpcError::Com`, automatic reconnection logic will fail to detect RPC disconnects.
- **Architectural Solution:** Inspect `OpcError::Server` codes using Win32 HRESULT bitwise classification:
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

### 5.3 Decoupling Windows COM via Pluggable Initializers (`NoOpComInit`)
- **Category:** Cross-Platform Portability & Mockability
- **Problem Statement:** Code interacting with Windows COM requires `CoInitializeEx(None, COINIT_MULTITHREADED)`. Calling this on non-Windows platforms or in headless Linux CI environments causes compilation failure or runtime crashes, preventing automated testing of domain logic.
- **Architectural Solution:** Abstract initialization behind a pure-Rust SPI trait with an associated RAII guard:
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

### 5.4 Pure-Rust 128-bit CLSID Domain Type
- **Category:** Boundary Hygiene & Platform Isolation
- **Problem Statement:** Using `windows::core::GUID` throughout high-level domain types leaks Windows SDK dependencies into pure-Rust domain layers, breaking `--no-default-features` compilation.
- **Architectural Solution:** Define a pure-Rust 128-bit `Clsid` (`#[repr(C)] struct Clsid([u8; 16])`) with RFC-4122 parsing, formatting, and unconditional bidirectional conversion to `windows::core::GUID`.

### 5.5 Direct Proxy Blanketing vs `QueryInterface` Ephemeral Allocation (Windows KB5004442)
- **Category:** Windows DCOM Security & FFI Proxy Topology
- **Problem Statement:** To satisfy modern Windows DCOM hardening (KB5004442 / CVE-2021-26414), remote COM calls require `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY`. When implementing `apply_proxy_blanket`, invoking `.cast::<windows::core::IUnknown>()` triggers an underlying `QueryInterface(&IUnknown::IID)` call. This produces a brand new, independently allocated proxy pointer. Calling `CoSetProxyBlanket` on that temporary pointer configures security on an ephemeral interface that is dropped immediately upon function return. The caller's actual dispatch interface proxy (`IOPCServer`, `IOPCItemMgt`, `IOPCSyncIO`) remains completely unblanketed, causing remote calls to fail with `0x80070005` (`E_ACCESSDENIED`).
- **Architectural Solution:** Re-borrow the concrete proxy's vtable pointer directly as `&windows::core::IUnknown` using raw pointer casting without `QueryInterface`:
  ```rust
  // Re-borrows the EXACT proxy instance without allocating an ephemeral duplicate:
  let unk = unsafe { &*std::ptr::from_ref(proxy).cast::<windows::core::IUnknown>() };
  apply_proxy_blanket(unk)?;
  ```
  This guarantees that DCOM security blankets apply directly to the dispatch proxy used for physical IPC round-trips.

### 5.6 Interface Segregation Principle (ISP) on COM Interface Graphs
- **Category:** COM Resource Optimization & Legacy Compatibility
- **Problem Statement:** Querying and caching secondary COM interfaces (such as `IOPCGroupStateMgt`, `IOPCAsyncIO2`, `IConnectionPointContainer`, `IOPCCommon`, `IOPCItemProperties`) that are never consumed by client logic introduces two severe architectural flaws:
  1. Mandatory fallible casts (`?`) cause connection or group creation to abort with `E_NOINTERFACE` (`0x80004002`) on standard minimal OPC DA 2.05a servers.
  2. Each unread interface query requires a synchronous DCOM `QueryInterface` RPC round-trip over the network, adding 12 redundant network round-trips per session.
- **Architectural Solution:** Strictly adhere to the Interface Segregation Principle by querying and retaining only the minimal interfaces required for synchronous telemetry:
  - `ComServer`: Retains strictly `server: IOPCServer` and optional `browse_server_address_space`.
  - `ComGroup`: Retains strictly `item_mgt: IOPCItemMgt` and `sync_io: IOPCSyncIO`.
  This guarantees maximum compatibility with industrial servers while saving ~12 network round-trips per connection.

### 5.7 Canonical Win32 HRESULT Modeling vs Raw Integer Casts (`clippy::cast_possible_wrap`)
- **Category:** Type Safety & Static Analysis Compliance
- **Problem Statement:** In Windows SDK bindings (`windows-core`), `HRESULT` wraps a signed 32-bit integer (`i32`). Win32 error codes with the high error bit set (e.g. `0x8000_4005` for `E_FAIL` or `0x8007_0005` for `E_ACCESSDENIED`) represent positive numbers in unsigned representation. Writing `0x8000_4005u32 as i32` triggers Clippy's `clippy::cast_possible_wrap` lint under `-D warnings`.
- **Architectural Solution:** Define canonical constants using `.cast_signed()` in a centralized `hresult` module:
  ```rust
  pub const E_FAIL: HRESULT = HRESULT(0x8000_4005_u32.cast_signed());
  pub const E_ACCESSDENIED: HRESULT = HRESULT(0x8007_0005_u32.cast_signed());
  ```
  Reference these canonical constants workspace-wide instead of ad-hoc integer casting.

---

## 6. Testing Topology, Determinism & Quality Gate Governance

### 6.1 Sub-Second Failure Cooldown Verification (No `sleep`)
- **Category:** Test Performance & Determinism
- **Problem Statement:** Testing circuit breakers or cooldown windows (e.g. 5-second failure backoff) by introducing real-time `tokio::time::sleep(Duration::from_millis(5100))` drastically slows down test suites, leading to flaky CI builds and degraded developer feedback loops.
- **Architectural Solution:** Test circuit breakers instantaneously by asserting immediate rejection on back-to-back requests executed within the active window without sleeping.

### 6.2 Strict Elimination of `Box<dyn Error>` and `anyhow` in Libraries
- **Category:** Library Engineering Standards
- **Problem Statement:** Using `Box<dyn std::error::Error>` or `anyhow::Result` in library code or doc-test helpers obscures concrete error variants from consumers and defeats static pattern matching.
- **Architectural Solution:**
  1. Enforce `thiserror` for all library errors and expose strongly-typed `OpcResult<T>`.
  2. Use structural pattern scanners in CI to reject `Box<dyn Error>` even in hidden doc-test runner functions (`/// # fn run() -> opc_da_client::OpcResult<()>`).

### 6.3 Test Topology Normalization: Co-located Unit Tests vs Integrated Suites
- **Category:** Maintainability & Clean Testing Architecture
- **Problem Statement:** Placing domain unit tests and mock contract checks into `tests/` inflates integration test counts and splits unit tests away from the code they verify.
- **Architectural Rule:**
  - **Unit Tests:** Must reside in co-located `mod tests` blocks within their respective source files (e.g. `src/types/handles.rs`, `src/types/batch.rs`).
  - **Integration Tests:** `tests/` is strictly reserved for true multi-subsystem integration suites (`domain_pipeline_test.rs`, `tag_io_integration_test.rs`, `resilience_and_pool_integration_test.rs`) exercising public facade contracts without private access.

### 6.4 Multi-Shell PowerShell Automation Portability (`Join-Path` Positional Constraints)
- **Category:** Toolchain Integrity & Windows Shell Compatibility
- **Problem Statement:** In PowerShell Core (`pwsh` 7+), `Join-Path` dynamically accepts three or more positional arguments (`Join-Path $PSScriptRoot ".." "compat"`). However, in Windows PowerShell 5.1 (the default built-in shell on Windows systems), `Join-Path` only defines two positional parameters (`-Path` and `-ChildPath`). Executing a script with 3 positional arguments under Windows PowerShell 5.1 immediately fails with `PositionalParameterNotFound`.
- **Architectural Solution:** Standardize all path joins to 2-parameter relative paths:
  ```powershell
  $compatDir = Join-Path $PSScriptRoot "..\compat"
  ```
  This guarantees 100% portability across Windows PowerShell 5.1 and PowerShell Core 7+.

### 6.5 AST-Grep Multi-Line Comment Sibling Token Matching
- **Category:** Static Analysis & AST Linter Architecture
- **Problem Statement:** In tree-sitter AST parsers, consecutive comment lines starting with `//` are parsed as individual, distinct `line_comment` nodes. An AST-Grep rule enforcing `require-safety-comment` using a `follows: { kind: line_comment, regex: "SAFETY:" }` selector inspects only the *immediate preceding sibling* comment node. If an unsafe block is preceded by a multi-line explanation where only the first line contains `SAFETY:`, the immediate sibling comment node fails the regex match, triggering a false-positive violation under Gate 6.
- **Architectural Solution:** Repeat the `// SAFETY:` prefix on every single line of a multi-line safety justification comment:
  ```rust
  // SAFETY: Pointer is non-null and points to an active DCOM proxy.
  // SAFETY: Casting to IUnknown conforms to standard Win32 COM ABI rules.
  let unk = unsafe { &*std::ptr::from_ref(proxy).cast::<windows::core::IUnknown>() };
  ```

### 6.6 Mock Server State Isolation in Recursive Tree Walks
- **Category:** Mock Fidelity & Test Suite Determinism
- **Problem Statement:** A mock server double (`MockConnectedServer::default()`) configured with simulated default child branches (e.g. `["Random", "Simulation"]`) will cause recursive walk tests targeting leaf chunking at depth 0 to traverse simulated child branches. If child branches re-enumerate the same leaf nodes, tests encounter unexpected item counts and premature capacity exhaustion.
- **Architectural Solution:** In unit and integration tests verifying leaf batching and traversal mechanics, explicitly chain builder methods to reset simulated child branch tags:
  ```rust
  let server = MockConnectedServer::default()
      .with_tags(leaf_tags)
      .with_branch_tags(Vec::new()) // Isolates depth-0 walk from mock branch re-traversal
      .with_organization(ServerOrganization::Hierarchical);
  ```
