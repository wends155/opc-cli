# OPC DA Client Modernization & Integration Testing Architecture

> **Document:** `refactor/integration_tests.md`  
> **Repository:** `opc-cli`  
> **Crate:** `opc-da-client`  
> **Date:** 2026-09-16  
> **Author:** Architect  

---

## 1. Executive Summary

This document formalizes the modernization roadmap and integration testing architecture for `opc-da-client`, established to remediate the 29 findings identified during the multi-lens code review (`review_report.md`).

Historically, `opc-da-client` was tightly coupled to the Windows COM runtime and the `#[cfg(feature = "opc-da-backend")]` compilation gate, enforcing a convention that integration tests required live registered OPC DA servers (such as Kepware or Matrikon Simulation). This led to an inverted testing topology where over 1,100 lines of tests in `src/com/worker/tests.rs` bypassed the public `OpcDaClient` facade to exercise private internal actor channel messages (`ComRequest`), while public typestates, builder options, and session lifecycles went largely unverified.

Through **Blocks A, B, and C (C1–C3)**, the testing architecture was completely overhauled:
1. The client facade was decoupled from Windows COM MTA initialization using pluggable initialization strategies (`ComInitializer`, `NoOpComInit`).
2. High-fidelity pure-Rust Tier 2 SPI mock infrastructure (`opc_da_client::connector::mock::*`) was leveraged to enable **100% offline, cross-platform integration tests** without live Windows COM servers.
3. Dedicated integration test suites were established under `opc-da-client/tests/`, verifying public contracts, typestate transitions, streaming subscriptions, active group caching, self-healing error recovery, and deterministic lifecycle teardown.

---

## 2. Testing Topology & Architecture

```
                               ┌─────────────────────────────┐
                               │     Public Client Facade    │
                               │  OpcDaClient<C, Typestate>  │
                               └──────────────┬──────────────┘
                                              │
                     ┌────────────────────────┴────────────────────────┐
                     │                                                 │
        [Offline / CI Testing]                                [Production COM]
                     ▼                                                 ▼
      ┌─────────────────────────────┐                   ┌─────────────────────────────┐
      │     MockServerConnector     │                   │        ComConnector         │
      │  (Pure-Rust Tier 2 SPI)     │                   │  (Win32 COM / DCOM Runtime) │
      └──────────────┬──────────────┘                   └──────────────┬──────────────┘
                     │                                                 │
        ┌────────────┴────────────┐                                    │
        ▼                         ▼                                    ▼
┌───────────────┐         ┌───────────────┐             ┌─────────────────────────────┐
│ MockConnected │         │   MockState   │             │   Windows COM Server MTA    │
│    Server     │         │ (Fault Inject │             │ (OPC DA 2.05a / 3.0 Engine) │
└───────┬───────┘         │  & Telemetry) │             └─────────────────────────────┘
        │                 └───────────────┘
        ▼
┌───────────────┐
│ MockConnected │
│     Group     │
└───────────────┘
```

### 2.1 Decoupled Facade (Block A)
- **`ComInitializer` Associated Guard:** `trait ComInitializer` defines `type Guard: 'static; fn init() -> OpcResult<Self::Guard>`.
- **`NoOpComInit`:** A zero-overhead, pure-Rust initializer (`type Guard = ()`) used for offline mock builds.
- **Three-Tier Builder Defaults:** `OpcDaClientBuilder<C>` and `OpcDaClient<C>` conditionally resolve defaults based on active features:
  - **Tier 1 (`opc-da-backend`):** Defaults to `ComConnector`.
  - **Tier 2 (`test-support` without `opc-da-backend`):** Defaults to `MockServerConnector`.
  - **Tier 3 (unconstrained):** Generic parameter `C: ServerBackend`.

### 2.2 Integration Test Suites
All integration test suites reside in `opc-da-client/tests/` and are registered in `opc-da-client/Cargo.toml` under `required-features = ["test-support"]`.

| Test Binary | Target Scope | Key Contracts Tested |
|---|---|---|
| `domain_pipeline_test.rs` | Pure-Rust Domain Pipeline | `TagBatch` $\to$ `TagValues` $\to$ `TagValue` $\to$ `OpcQuality` $\to$ `WriteBatch` $\to$ `VarType` |
| `server_discovery_integration_test.rs` | Catalog & Registry Enumeration | `ServerDiscovery`, `list_servers`, `list_server_details`, structured metadata, host normalization |
| `tag_browsing_integration_test.rs` | Namespace Traversal | `TagBrowser`, flat browsing, hierarchical DFS walk, `BrowsePositionGuard`, `TagCollector` limits |
| `tag_io_integration_test.rs` | Synchronous Tag Read/Write | `TagReader`, `TagWriter`, mixed-type batch reading across 10 types, typed accessors, cache hits |
| `subscription_integration_test.rs` | Async Subscription Streaming | Inherent `subscribe()`, polling streams, dynamic mock value updates, receiver cancellation |
| `resilience_and_pool_integration_test.rs` | Resilience, Pooling & Lifecycle | `connect_eager()`, invalid handle recovery (`0xC0040001`), connection eviction (`0x800706BA`), cooldowns, RAII thread join |
| `typestate_client_test.rs` | Typestate Guarantees & Failover | `Unbound` gateway vs `Bound` session, CLSID endpoints, session unbind and rebind to standby |

---

## 3. Detailed Test Catalog

### 3.1 Server Discovery (`tests/server_discovery_integration_test.rs`)
- `test_server_discovery_local_and_remote_enumeration`: Validates local (`localhost`) vs. remote server enumeration against mock catalog.
- `test_server_discovery_structured_metadata_inspection`: Validates ProgID, CLSID, and user-friendly description parsing into `OpcServerInfo`.
- `test_server_discovery_trait_polymorphism`: Validates that both `OpcDaClient<_, Unbound>` and `OpcDaClient<_, Bound>` implement `ServerDiscovery`.
- `test_server_discovery_connection_failure_simulation`: Injects connection failure (`should_fail_connect`) and asserts correct `OpcError` propagation.
- `test_server_discovery_empty_catalog`: Asserts graceful handling of empty server catalogs returning empty vectors without error.

### 3.2 Namespace Browsing (`tests/tag_browsing_integration_test.rs`)
- `test_tag_browsing_flat_namespace`: Validates flat address space enumeration (`BrowseType::Flat`).
- `test_tag_browsing_hierarchical_fast_flat`: Validates fast flat acceleration fallback when hierarchical servers advertise flat organization.
- `test_tag_browsing_hierarchical_recursive_walk`: Validates recursive depth-first namespace traversal with `BrowsePositionGuard` cursor restoration.
- `test_tag_browsing_collector_limits_and_cancellation`: Asserts `TagCollector` capacity limits truncate browsing once max capacity is reached.
- `test_tag_browsing_bound_session_facade`: Validates inherent `bound.browse()` methods on `Bound` client sessions.
- `test_tag_browsing_guard_unwind_symmetry_and_error_recovery`: Asserts branch descent failure unrolls cursor position safely without leaking browse state.

### 3.3 Tag I/O & Caching (`tests/tag_io_integration_test.rs`)
- `test_tag_io_mixed_type_batch_read_and_typed_getters`: Exercises batch reads across 10 distinct OPC DA types (`Int`, `UInt`, `Float`, `Double`, `String`, `Bool`, `Array`) and validates typed accessors (`get_i32`, `get_f64`, `get_str`, `get_bool`, `get_as`).
- `test_tag_io_inherent_scalar_convenience_readers`: Validates `read_f64`, `read_i32`, `read_bool`, `read_str` on `Bound` client.
- `test_tag_io_partial_item_read_failure_handling`: Validates partial batch read failures where individual tags fail with `OPC_E_INVALIDITEMID` while sibling tags succeed.
- `test_tag_io_empty_batch_short_circuit`: Asserts empty read/write batches short-circuit immediately with empty collections without worker round-trips.
- `test_tag_io_batch_write_partial_failures_and_diagnostics`: Validates batch writes with partial failures, asserting HRESULT hints (Finding #23).
- `test_tag_io_role_trait_polymorphism`: Validates `TagReader` and `TagWriter` invocations across generic `State`.
- `test_tag_io_active_group_cache_hit`: Asserts repeated read cycles reuse the cached active group, verifying zero additional group creations.

### 3.4 Subscription Streaming (`tests/subscription_integration_test.rs`)
- `test_subscription_multi_tick_cadence`: Validates multi-tick periodic delivery cadence over Tokio time intervals.
- `test_subscription_dynamic_mock_value_updates`: Injects dynamic tag updates into `MockState` during streaming and asserts receiver observes mutations.
- `test_subscription_receiver_drop_cancellation`: Dropping the stream receiver causes the background polling task to terminate cleanly (Finding #22).
- `test_subscription_transient_error_resilience`: Injects transient read errors into the stream and asserts polling continues without terminating (Finding #21).
- `test_subscription_connection_error_termination`: Injects fatal connection drop and asserts channel terminates cleanly returning `None` (Finding #21).
- `test_subscription_zero_allocation_batch_sharing`: Validates `into_shareable()` batch projection across arrays and vectors.

### 3.5 Connection Resilience, Pooling & Lifecycle (`tests/resilience_and_pool_integration_test.rs`)
- `test_eager_ping_reachability`: Validates `connect_eager()` probe on healthy mock server vs. unreachable server (`RPC_S_SERVER_UNAVAILABLE`).
- `test_active_group_auto_recovery_on_invalid_handle`: Validates transparent invalidation and retry when a cached group returns `0xC0040001` (`OPC_E_INVALIDHANDLE`), asserting `add_group_count` increments to 2 and `read_tags` succeeds without error.
- `test_connection_drop_eviction_and_reconnection`: Simulates RPC disconnect (`0x800706BA` `RPC_S_SERVER_UNAVAILABLE`), verifying stale proxy eviction and fresh reconnection on retry (`connect_count` increments from 1 to 2).
- `test_circuit_breaker_failure_cooldown_short_circuit`: Validates immediate fast-path rejection during the 5-second failure cooldown window without slow sleeps.
- `test_client_worker_deterministic_lifecycle_teardown`: Validates clean channel disconnection and OS worker thread join when `OpcDaClient` is dropped.

### 3.6 Typestate Session Error Recovery & Rebinding (`tests/typestate_client_test.rs`)
- `test_typestate_failure_recovery_and_rebind`: Validates unbinding from a primary server endpoint (`bound.unbind()`) and rebinding (`unbound.bind()`) to an alternative standby server while preserving the underlying worker instance (Finding #24).

---

## 4. Full Modernization Roadmap (29 Findings)

| Block | Phase / Name | Primary Scope | Status |
|:---:|---|---|:---:|
| **Block A** | **Facade Decoupling & Ungating** | Decouple `OpcDaClient`, `ComWorker`, and `OpcDaClientBuilder` from `opc-da-backend` gate using `NoOpComInit`. | ✅ Complete (`02e1e16`) |
| **Block B** | **Test Topology Normalization & Domain Pipeline** | Relocate misplaced unit tests, decompose 1,927-line `types/tests.rs` into 10 submodules, deliver `domain_pipeline_test.rs`. | ✅ Complete |
| **Block C1** | **Server Discovery & Tag Browsing Test Suites** | Deliver `server_discovery_integration_test.rs` (5 tests) and `tag_browsing_integration_test.rs` (6 tests). | ✅ Complete |
| **Block C2** | **Tag I/O & Subscription Streaming Test Suites** | Deliver `tag_io_integration_test.rs` (7 tests) and `subscription_integration_test.rs` (6 tests). Closes Findings #2, #21, #22, #23. | ✅ Complete |
| **Block C3** | **Resilience, Pooling & Lifecycle Test Suites** | Deliver `resilience_and_pool_integration_test.rs` (5 tests) and extend `typestate_client_test.rs` (Finding #24). Closes Findings #2, #24. | ✅ Complete |
| **Block D** | **Worker Resilience, Concurrency & Panic Safety** | Remediate worker panic recovery (`catch_unwind` on `pool.clear()`), in-flight request drainage on worker panic, write idempotency safety, active group length mismatch cache invalidation, channel backpressure, and early request cancellation. | ✅ Complete |
| **Block E** | **Hot-Path Performance & Allocation Optimization** | Eliminate redundant string clones and throwaway error strings on cache hits in `handle_read`, reduce browse mutex lock contention, implement bounded LRU active group caching per endpoint, bound connection pool proxy pruning, and tag batch chunking. | ✅ Complete |
| **Block F** | **API Invariants, Security & Error Diagnostics** | Strict ProgID validation on `ServerIdentifier::new`, typestate role trait scoping (`Bound` vs `Unbound`), constructor renaming (`bind_new` vs async `connect`), argument validation in `build_bound`, RPC error classification in `is_connection_error`, tag context retention in `TagExtractError`, null-byte validation in `to_wide_null`, and diagnostic `get_value_checked` in `TagValues`. | ✅ Complete |

---

## 5. Verification Baseline & Quality Gates

Every block must pass the workspace 9-gate quality verification pipeline without warning or exemption:

```powershell
pwsh -File scripts/verify.ps1
```

1. **Gate 1 (Formatter):** `cargo fmt --all -- --check` (100% compliant).
2. **Gate 2 (Linter):** `cargo clippy --all-targets --all-features -- -D warnings` (zero warnings).
3. **Gate 3 (Doc Tests):** `cargo test --doc` (88 doc-tests passed, 2 compile-fail passed).
4. **Gate 4 (Unit & Integration Tests):** `cargo test --all-features` (all 400+ workspace tests passed).
5. **Gate 5 (Cross-Platform Feature Independence):** `cargo check -p opc-da-client --no-default-features` (offline headless builds green).
6. **Gate 6 (Polyfills Build & Test):** `bcryptprimitives`, `api_ms_win_core_synch_l1_2_0`, `winrt-error-polyfill`.
7. **Gate 7 (AST-Grep Scans & Rules):** Structural AST linting (no unhandled panics or `.unwrap()` in production code).
8. **Gate 8 (Forbidden Pattern Scanner):** Zero `println!`, `dbg!`, `todo!`, `anyhow`, or `Box<dyn Error>` in library code.
9. **Gate 9 (Script Syntax Validation):** PowerShell AST strict mode validation across all repository scripts.
