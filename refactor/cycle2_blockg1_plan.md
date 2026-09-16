# Implementation Plan: Block G1: Clean Slate API Excision & Struct Deduplication

**Role:** Architect • **Date:** 2026-09-16 • **Tier:** M
**Scope:** Block G1: Clean Slate API Excision & Struct Deduplication in `opc-da-client`

### Builder Context
Read before starting:
- `opc-da-client/src/connector/traits.rs` L187-337 (ServerCatalogDiscovery, ServerConnector, ConnectedServer, ConnectedGroup — all synchronous)
- `opc-da-client/src/client/mod.rs` L27-165 (OpcDaClient struct definitions ×3, builder() ×2, connect/connect_remote, ComWorker)
- `opc-da-client/src/client/builder.rs` L12-190 (OpcDaClientBuilder struct definitions ×3, new(), with_legacy_dcom, Default impl)
- `opc-da-client/src/provider.rs` L241-330 (TagWriter trait), L338-348 (OpcProvider trait + blanket impl), L351-448 (mock module with 5 mockall blocks), L497-621 (unit tests), L680-812 (full contract stability test)
- `opc-da-client/src/client/gateway.rs` L151-195 (TagWriter impl for OpcDaClient)
- `opc-da-client/src/lib.rs` L1-54 (root crate re-exports and module doc comments)
- `opc-da-client/src/errors/hresult.rs` L1-75 (HRESULT re-export, format_hresult)
- `opc-da-client/src/errors.rs` L39-90 (OpcError enum variants — NO `Unavailable` variant; use `NotImplemented(String)`)
- `opc-da-client/README.md` L605-640 (deprecation table and role trait definitions)
- `refactor/cycle2_review.md` (Findings #1, #4, #16, #17, #19)
- `.agents/rules/coding-standard.md` (governance core rules)

### Phase Context
- **Phase:** Block G1 of 2 (Cycle 2 Modernization: Block G1 Clean Slate Excision -> Block G2 Ergonomics & Documentation)
- **Prior phase:** Cycle 1 completed all offline integration testing infrastructure, 6 facade integration suites, and worker resilience remediation (Block D).
- **Stubs for this phase:** `NoopServerBackend`, `NoopConnectedServer`, `NoopConnectedGroup` in `src/connector/traits.rs` to serve as headless fallback backend when `opc-da-backend` and `test-support` are absent.
- **Following phase:** Block G2 (Ergonomic Alignment & Comprehensive Documentation — `write_tag_batch` accepting `impl IntoWriteBatch`, `write_tag_value` accepting `impl Into<OpcValue>`, and unbound aliases `read_tags`, `write_tags`, `browse`).

---

### Problem Statement
In `refactor/cycle2_review.md`, five findings compromise the public API clarity, type ergonomics, structural deduplication, and deprecation hygiene of `opc-da-client`:
1. **Finding #1 (Critical - API Design)**: Clean break API deprecation excision. Obsolete v0.1/v0.2 methods (`connect`, `connect_remote`, and `TagWriter::write_tag_values`) linger in the codebase wrapped with `#[deprecated]` and blanket `#[allow(deprecated)]` suppression across provider mocks, trait definitions, and tests. Per user architectural mandate for semver 0.3.0, these obsolete methods must be cleanly excised.
2. **Finding #4 (Major - API Design)**: Root re-export omissions. `ParseEndpointError` and `DefaultOpcDaClient` are defined in submodules (`types::server` and `client`) but not re-exported unconditionally at the crate root, causing ergonomics friction for downstream consumers.
3. **Finding #16 (Major - Design)**: Duplicated struct definitions in `src/client/mod.rs` and `src/client/builder.rs`. `OpcDaClient` and `OpcDaClientBuilder` are each declared 3 times with mutually exclusive `#[cfg(...)]` blocks rather than leveraging a unified `DefaultBackendConnector` type alias.
4. **Finding #17 (Major - Design)**: Incomplete headless backend stubbing. When built with `--no-default-features` (without `opc-da-backend` and without `test-support`), the third `OpcDaClient` definition has no default for `C`, leaving non-Windows offline users unable to instantiate the client without third-party mock implementations.
5. **Finding #19 (Minor - API Design)**: `HRESULT` re-export and dead code warning in `src/errors/hresult.rs`. `windows_core::HRESULT` is not re-exported, forcing downstream crates to depend directly on `windows-core`. `format_hresult` has `#[allow(dead_code)]` without doctests.

---

### Plan Objectives
| ID | Objective | Success Criteria | Steps |
|----|-----------|-----------------|-------|
| O1 | Introduce `NoopServerBackend` headless connector fallback (Finding #17) | `NoopServerBackend` implemented synchronously in `connector/traits.rs` using `OpcError::NotImplemented`; `DefaultBackendConnector` defaults to `NoopServerBackend` when neither COM nor mock features are active | 1 |
| O2 | Consolidate `OpcDaClient` to a single generic struct definition and excise `connect`/`connect_remote` (Findings #1, #16, #17) | Single `pub struct OpcDaClient<C: ServerBackend + 'static = DefaultBackendConnector, State = Unbound>`; single canonical `builder()`; `connect` and `connect_remote` deleted | 2 |
| O3 | Consolidate `OpcDaClientBuilder` to a single generic struct definition (Finding #16) | Single `pub struct OpcDaClientBuilder<C = DefaultBackendConnector>`; `new()` available when `C: ServerBackend + Default`; `with_legacy_dcom` synchronized for `ComConnector` | 3 |
| O4 | Excise `TagWriter::write_tag_values` and purge ALL `#[allow(deprecated)]` (Finding #1) | `write_tag_values` deleted from `TagWriter` (L315-329), `OpcProvider` trait `#[allow(deprecated)]` (L338, L344) removed, `mockall` macros purged (L388-392, L441-445), all test references deleted; zero `#[allow(deprecated)]` in crate | 4 |
| O5 | Excise `<OpcDaClient as TagWriter>::write_tag_values` delegation (Finding #1) | Delegation arm in `gateway.rs` (L185-194) removed | 5 |
| O6 | Re-export `ParseEndpointError`, `DefaultOpcDaClient`, and `HRESULT` at public roots (Findings #4, #19) | `opc_da_client::ParseEndpointError`, `opc_da_client::DefaultOpcDaClient`, and `errors::hresult::HRESULT` accessible | 6 |
| O7 | Add runnable doctest to `format_hresult` and remove dead code suppression (Finding #19) | `format_hresult` has passing doctest in Gate 3 (`cargo test --doc`) with zero `#[allow(dead_code)]` | 6 |
| O8 | Make `#![doc]` inclusion unconditional and update `README.md` deprecation schedule (Findings #1, #17) | `src/lib.rs:2` `#![doc]` unconditional; `README.md` reflects clean removal of `write_tag_values` in 0.3.0 | 7, 8 |
| O9 | Full workspace 9-gate quality verification green | `pwsh -File scripts/verify.ps1` exits 0 with zero warnings | 9 |

---

### Review History & Verdict
| Cycle | Reviewer | Model | Verdict | Adjustments Applied |
|---|---|---|---|---|
| 1 | `plan-reviewer` | `flash` | ⚠️ Revisions Recommended | (1) Added `README.md` doc inclusion to Blast Radius Table. (2) Removed `DefaultOpcDaClient` duplicate alias in `client/mod.rs`. (3) Added unit test for `NoopServerBackend`. (4) Removed unused `WriteResult` and `#[allow(deprecated)]` in mock and provider tests. (5) Kept `DefaultBackendConnector` type alias in `client/mod.rs`. (6) Added `test_noop_connector_compilation` headless test. (7) Verified `workspace.package.version` is already `0.3.0`. |
| 2 | `plan-reviewer` | `flash` | ⚠️ Revisions Recommended | (1) Synchronized both `self.legacy_dcom = legacy_dcom` and `self.connector = Some(ComConnector::with_legacy_dcom(legacy_dcom))` in `with_legacy_dcom`. (2) Corrected provider test target to `test_provider_default_read_tag_value` (lines 498-570), purged `write_tag_values` assertions, and renamed partial failure test. (3) Re-exported `windows_core::HRESULT` via `pub use`. (4) Pruned `write_tag_values` from `README.md`. |
| 3 | `plan-reviewer` | `flash` | ⚠️ Revisions Recommended (Cycle Cap) | (1) Replaced fabricated async trait signatures with synchronous SPI traits. (2) Preserved canonical `<C, State>` generic ordering. (3) Added mock purge targets L388-392, L441-445, L737-745, L793-801. (4) Made `#![doc]` unconditional and de-duplicated re-exports. (5) Dropped phantom test removal. |
| 4 | Architect (inline) | `claude-opus-4-6` | ✅ Approved | (1) **CRITICAL**: Replaced `OpcError::Unavailable(...)` with `OpcError::NotImplemented(...)` — `Unavailable` variant does not exist in `OpcError` enum. (2) Added `#[allow(deprecated)]` purge on `OpcProvider` trait L338 and blanket impl L344. (3) Corrected test name to `test_mock_opc_provider_full_contract_stability`. (4) Updated all advisory line numbers to match verified codebase. (5) Added consolidation of both `builder()` methods (L86-88 and L150-152). (6) Removed `⚠️ Reviewer Findings (Unresolved)` section — all items resolved. |

---

### Negative Scope
**Out of Scope:**
- Do NOT implement Block G2 ergonomics (`Unbound::write_tag_batch` with `impl IntoWriteBatch`, `Unbound::write_tag_value` with `impl Into<OpcValue>`, or unbound aliases `read_tags`, `write_tags`, `browse`). Reserved for Block G2.
- Do NOT modify worker internals or connection pool logic in `src/com/worker/`.
- Do NOT modify CLI crate commands in `opc-cli`.
- Do NOT touch external workspace dependencies in `Cargo.toml`.
- Do NOT add new `OpcError` variants (use existing `NotImplemented(String)` for noop backend).

---

### Interface Contracts

#### 1. Headless Fallback Backend Synchronous SPI (`src/connector/traits.rs`)

> [!IMPORTANT]
> All connector SPI traits are **100% synchronous**. Async scheduling is handled exclusively by `ComWorker` in Tier 2. The `NoopServerBackend` MUST use `OpcError::NotImplemented(String)` — the `OpcError::Unavailable` variant does **not exist**.

```rust
/// A no-op backend connector used as the default type parameter when running in headless
/// or offline environments without active COM or mock feature flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoopServerBackend;

impl ServerCatalogDiscovery for NoopServerBackend {
    fn enumerate_servers(&self, _host: &str) -> OpcResult<Vec<String>> {
        Ok(Vec::new())
    }

    fn enumerate_server_details(&self, _host: &str) -> OpcResult<Vec<OpcServerInfo>> {
        Ok(Vec::new())
    }
}

impl ServerConnector for NoopServerBackend {
    type Server = NoopConnectedServer;

    fn connect_identifier(&self, id: &ServerIdentifier) -> OpcResult<Self::Server> {
        Err(OpcError::NotImplemented(format!(
            "No active OPC DA backend configured to connect to '{id}'"
        )))
    }
    // connect_endpoint and connect have default impls that delegate to connect_identifier
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopConnectedServer;

impl ConnectedServer for NoopConnectedServer {
    type Group = NoopConnectedGroup;
    type ItemIterator = std::iter::Empty<OpcResult<String>>;

    fn ping(&self) -> OpcResult<()> {
        Ok(())
    }

    fn query_organization(&self) -> OpcResult<NamespaceType> {
        Err(OpcError::NotImplemented(
            "Noop backend has no address space".into(),
        ))
    }

    fn browse_opc_item_ids(
        &self,
        _: BrowseType,
        _: Option<&str>,
        _: VarType,
        _: u32,
    ) -> OpcResult<Self::ItemIterator> {
        Ok(std::iter::empty())
    }

    fn change_browse_position(&self, _: BrowseDirection, _: &str) -> OpcResult<()> {
        Err(OpcError::NotImplemented(
            "Noop backend cannot navigate branches".into(),
        ))
    }

    fn get_item_id(&self, _: &str) -> OpcResult<String> {
        Err(OpcError::NotImplemented(
            "Noop backend cannot resolve item IDs".into(),
        ))
    }

    fn add_group(&self, _: &GroupConfig<'_>) -> OpcResult<CreatedGroup<Self::Group>> {
        Err(OpcError::NotImplemented(
            "Noop backend cannot add groups".into(),
        ))
    }

    fn remove_group(&self, _: ServerGroupHandle, _: GroupRemovalMode) -> OpcResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopConnectedGroup;

impl ConnectedGroup for NoopConnectedGroup {
    fn add_items(&self, _: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>> {
        Ok(Vec::new())
    }

    fn read(
        &self,
        _: DataSource,
        _: &[ServerItemHandle],
    ) -> OpcResult<Vec<Result<GroupItemState, OpcError>>> {
        Ok(Vec::new())
    }

    fn write(&self, _: &[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>> {
        Ok(Vec::new())
    }
}
```

#### 2. Unified `DefaultBackendConnector`, `OpcDaClient`, and `OpcDaClientBuilder`

> [!IMPORTANT]
> `DefaultBackendConnector` does NOT currently exist in the codebase. It must be **created** (not modified). The `OpcDaClient` generic order is `<C, State>` (connector first). The struct fields include `worker: Arc<ComWorker<C>>` — this is the background MTA worker thread, NOT `Arc<C>`.

```rust
// In src/client/mod.rs — CREATE type aliases:
#[cfg(feature = "opc-da-backend")]
pub type DefaultBackendConnector = crate::com::connector::ComConnector;

#[cfg(all(not(feature = "opc-da-backend"), any(test, feature = "test-support")))]
pub type DefaultBackendConnector = crate::connector::MockServerConnector;

#[cfg(all(not(feature = "opc-da-backend"), not(any(test, feature = "test-support"))))]
pub type DefaultBackendConnector = crate::connector::NoopServerBackend;

// SINGLE struct definition replacing the 3 duplicates:
pub struct OpcDaClient<C: ServerBackend + 'static = DefaultBackendConnector, State = Unbound> {
    pub(crate) worker: Arc<ComWorker<C>>,
    pub(crate) endpoint: Option<OpcServerEndpoint>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) _state: std::marker::PhantomData<State>,
}

pub type DefaultOpcDaClient<State = Unbound> = OpcDaClient<DefaultBackendConnector, State>;

// SINGLE builder() replacing the 2 duplicates (L86-88 and L150-152):
impl OpcDaClient<DefaultBackendConnector, Unbound> {
    #[must_use]
    pub fn builder() -> OpcDaClientBuilder<DefaultBackendConnector> {
        OpcDaClientBuilder::new()
    }
}

// In src/client/builder.rs — SINGLE struct definition replacing the 3 duplicates:
#[derive(Debug, Clone)]
pub struct OpcDaClientBuilder<C = DefaultBackendConnector> {
    pub(crate) host: Option<String>,
    pub(crate) server: Option<ServerIdentifier>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) legacy_dcom: bool,
    pub(crate) connector: Option<C>,
}

impl<C: ServerBackend + Default> OpcDaClientBuilder<C> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            host: None,
            server: None,
            timeout: None,
            legacy_dcom: false,
            connector: Some(C::default()),
        }
    }
}

#[cfg(feature = "opc-da-backend")]
impl OpcDaClientBuilder<crate::com::connector::ComConnector> {
    #[must_use]
    pub fn with_legacy_dcom(mut self, legacy_dcom: bool) -> Self {
        self.legacy_dcom = legacy_dcom;
        self.connector = Some(crate::com::connector::ComConnector::with_legacy_dcom(legacy_dcom));
        self
    }
}
```

#### 3. Root Public Re-exports (`src/lib.rs` & `src/errors/hresult.rs`)
```rust
// In src/lib.rs:
#![doc = include_str!("../README.md")]  // unconditional (was cfg_attr gated)

pub use crate::types::ParseEndpointError;  // NEW
pub use crate::client::DefaultOpcDaClient;  // unconditional (was #[cfg(feature = "opc-da-backend")])

// In src/errors/hresult.rs:
pub use windows_core::HRESULT;  // was private `use`
```

---

### Blast Radius Table
| Symbol | File | Direct Callers | Indirect Deps | Test-Only | Cross-Package? |
|--------|------|---------------|---------------|-----------|----------------|
| `TagWriter::write_tag_values` | `src/provider.rs:315-329` | None (0 callers across workspace) | Public trait contract | No | Yes (Breaking removal) |
| `<OpcDaClient as TagWriter>::write_tag_values` | `src/client/gateway.rs:185-194` | None | Gateway trait impl | No | Yes (Breaking removal) |
| `OpcDaClient::connect` | `src/client/mod.rs:129-133` | 0 callers | Public facade | No | Yes (Breaking removal) |
| `OpcDaClient::connect_remote` | `src/client/mod.rs:139-144` | 0 callers | Public facade | No | Yes (Breaking removal) |
| `#[allow(deprecated)]` on `OpcProvider` | `src/provider.rs:338, 344` | Trait + blanket impl | Public trait | No | No |
| `OpcDaClientBuilder` struct decl ×3 | `src/client/builder.rs:13-46` | `client::builder()`, integration tests | Internal builder | No | No |
| `OpcDaClient` struct decl ×3 | `src/client/mod.rs:31-61` | Entire client facade | Workspace types | No | No |
| `builder()` method ×2 | `src/client/mod.rs:86-88, 150-152` | Public constructor | Gateway | No | No |
| `NoopServerBackend` | `src/connector/traits.rs` | `DefaultBackendConnector` headless fallback | Connector hierarchy | No | No |
| `ParseEndpointError` | `src/lib.rs` | `types::server` | Public root re-export | No | Yes (Additive) |
| `DefaultOpcDaClient` | `src/lib.rs` | `client::mod` | Public root re-export | No | Yes (Additive) |
| `HRESULT` re-export | `src/errors/hresult.rs:3` | `windows_core` | Public errors re-export | No | Yes (Additive) |
| `format_hresult` | `src/errors/hresult.rs:64-73` | Doctest | Diagnostic helper | No | No |
| `README.md` | `opc-da-client/README.md` | Root `#![doc = include_str!("../README.md")]` | Documentation & doctests | Yes | No |

---

### Deprecation Schedule

| Old Symbol | New Symbol | Introduced | Removal Target | Migration |
|:---|:---|:---:|:---:|:---|
| `TagWriter::write_tag_values` | `TagWriter::write_tag_batch` | 0.2.0 | 0.3.0 (Clean Break) | Pass `WriteBatch::Borrowed(&[...])` or `WriteBatch::Owned(...)` to `write_tag_batch` |
| `OpcDaClient::connect` | `OpcDaClient::builder().server(...).build_bound().await` | 0.1.0 | 0.3.0 (Clean Break) | Use fluent builder to configure endpoint and construct client |
| `OpcDaClient::connect_remote` | `OpcDaClient::builder().host(...).server(...).build_bound().await` | 0.1.0 | 0.3.0 (Clean Break) | Use fluent builder with `.host(...)` to configure remote endpoint |

---

### Test Plan (TDD)
1. **`test_noop_server_backend_behavior` (`src/connector/traits.rs`)**:
   - Instantiate `NoopServerBackend::default()`.
   - Assert `enumerate_servers` and `enumerate_server_details` return `Ok(empty)`.
   - Assert `connect_identifier` returns `Err(OpcError::NotImplemented(...))`.
   - Instantiate `NoopConnectedServer::default()`. Assert `ping()` is `Ok(())`, `query_organization()` is `Err(OpcError::NotImplemented(...))`, `browse_opc_item_ids()` is empty iterator, and `remove_group()` is `Ok(())`.
   - Instantiate `NoopConnectedGroup::default()`. Assert `add_items`, `read`, and `write` return empty success vectors.
2. **`test_noop_connector_compilation` (`tests/headless_compilation_test.rs`)**:
   - Verify `OpcDaClientBuilder::new()` and `OpcDaClient::builder()` compile cleanly without `opc-da-backend` or `test-support`.
3. **`test_provider_default_read_tag_value` regression**:
   - Verify provider default methods operate cleanly without `write_tag_values` or `#[allow(deprecated)]`.
4. **`test_format_hresult_doctest`**:
   - Validate doc test execution in Gate 3 (`cargo test --doc`).

---

### Global Execution Order

Step 1: [MODIFY] `opc-da-client/src/connector/traits.rs` — [+] `NoopServerBackend`, `NoopConnectedServer`, `NoopConnectedGroup` (after L337)
- Pre: ALL
- Target: `src/connector/traits.rs` after `ConnectedGroup` trait (L337)
- Action:
  1. Define synchronous `NoopServerBackend`, `NoopConnectedServer`, `NoopConnectedGroup` structs implementing `ServerCatalogDiscovery`, `ServerConnector`, `ConnectedServer`, and `ConnectedGroup`.
  2. Use `OpcError::NotImplemented(String)` for all error-returning methods (NOT `OpcError::Unavailable` which does not exist).
  3. For `ServerConnector`, only implement `connect_identifier` (required). `connect_endpoint` and `connect` have default impls that delegate to it.
  4. Add unit test `test_noop_server_backend_behavior`:
     ```rust
     #[test]
     fn test_noop_server_backend_behavior() {
         let backend = NoopServerBackend;
         assert!(backend.enumerate_servers("localhost").unwrap().is_empty());
         assert!(backend.enumerate_server_details("localhost").unwrap().is_empty());
         assert!(backend.connect_identifier(&ServerIdentifier::ProgId("Server.A".into())).is_err());
         let server = NoopConnectedServer;
         assert!(server.ping().is_ok());
         assert!(server.remove_group(ServerGroupHandle(1), GroupRemovalMode::Default).is_ok());
     }
     ```
- Post: CHECK

Step 2: [MODIFY] `opc-da-client/src/client/mod.rs` — [+] `DefaultBackendConnector`, [~] `OpcDaClient` consolidation, [~] `builder()` consolidation, [-] `connect`/`connect_remote` (L27-165)
- Pre: CHECK
- Target: `src/client/mod.rs:27-165`
- Action:
  1. **CREATE** (not modify) `DefaultBackendConnector` type alias with 3 `#[cfg]` arms. The headless fallback arm resolves to `crate::connector::NoopServerBackend`.
  2. **CREATE** `DefaultOpcDaClient<State = Unbound>` type alias.
  3. Consolidate the 3 duplicate `pub struct OpcDaClient` definitions (L31-61) into a single generic struct with `C: ServerBackend + 'static = DefaultBackendConnector`. Preserve all 4 fields: `worker`, `endpoint`, `timeout`, `_state`.
  4. Consolidate the 2 duplicate `builder()` methods (L86-88 under `#[cfg(feature = "opc-da-backend")]` and L150-152 under test-support cfg) into a single `impl OpcDaClient<DefaultBackendConnector, Unbound>` block with `pub fn builder() -> OpcDaClientBuilder<DefaultBackendConnector>`.
  5. Delete deprecated `pub fn connect` (L125-133) and `pub fn connect_remote` (L135-144).
  6. Remove the now-empty `#[cfg(feature = "opc-da-backend")] impl OpcDaClient<ComConnector, Unbound>` block if it only contained `builder()`, `connect`, and `connect_remote`. Keep `bind_new`, `bind_new_remote`, etc. that remain.
- Post: CHECK

Step 3: [MODIFY] `opc-da-client/src/client/builder.rs` — [~] `OpcDaClientBuilder` consolidation, [~] generic `new()`, [~] `with_legacy_dcom` (L12-69)
- Pre: CHECK
- Target: `src/client/builder.rs:12-69`
- Action:
  1. Consolidate the 3 duplicate `pub struct OpcDaClientBuilder` definitions (L13-46) into a single generic struct with `C = DefaultBackendConnector`.
  2. Replace the `#[cfg(feature = "opc-da-backend")] impl OpcDaClientBuilder<ComConnector> { pub fn new() }` (L48-60) with a generic constructor `impl<C: ServerBackend + Default> OpcDaClientBuilder<C> { pub fn new() -> Self { ... connector: Some(C::default()) } }`.
  3. Keep `#[cfg(feature = "opc-da-backend")] impl OpcDaClientBuilder<ComConnector>` for `with_legacy_dcom` (L62-68), ensuring it synchronizes both `self.legacy_dcom = legacy_dcom` and `self.connector = Some(ComConnector::with_legacy_dcom(legacy_dcom))`.
  4. Update the existing `impl<C: ServerBackend + Default + 'static> Default for OpcDaClientBuilder<C>` (L185) to delegate to `Self::new()` if it doesn't already.
- Post: CHECK

Step 4: [MODIFY] `opc-da-client/src/provider.rs` — [-] `TagWriter::write_tag_values`, [~] purge ALL `#[allow(deprecated)]`, mocks, and dead test assertions (L241-812)
- Pre: CHECK
- Target: `src/provider.rs`
- Action (comprehensive — 7 sub-actions):
  1. **DELETE** `TagWriter::write_tag_values` default method, deprecation attribute, and doc comments (L315-329).
  2. **DELETE** `#[allow(deprecated)]` on `OpcProvider` trait definition (L338).
  3. **DELETE** `#[allow(deprecated)]` on `OpcProvider` blanket impl (L344).
  4. **MODIFY** lint suppression on L352 from `#![allow(clippy::struct_field_names, deprecated)]` to `#![allow(clippy::struct_field_names)]`.
  5. **DELETE** `async fn write_tag_values` from `mockall::mock! { pub OpcProvider }` (L388-392) and from `mockall::mock! { pub TagWriter }` (L441-445).
  6. In `test_provider_default_read_tag_value` (L497-570): **DELETE** L498 `#[allow(deprecated)]`, **DELETE** L543-556 (obsolete `write_tag_values` call and assertions). In `test_provider_default_write_tag_values_partial_failure` (L572-621): **DELETE** L573 `#[allow(deprecated)]`, **RENAME** to `test_provider_default_write_tag_batch_partial_failure`, **UPDATE** L607-616 to call `write_tag_batch` with `WriteBatch::Borrowed`.
  7. In `test_mock_opc_provider_full_contract_stability` (L680-812): **DELETE** L737-745 (`mock.expect_write_tag_values()` setup), **DELETE** L793-801 (`#[allow(deprecated)]` + `provider.write_tag_values(...)` call and assertions).
- Post: CHECK, zero `#[allow(deprecated)]` in crate (`rg "allow(deprecated)" opc-da-client/src/ expects: 0 matches`)

Step 5: [MODIFY] `opc-da-client/src/client/gateway.rs` — [-] `<OpcDaClient as TagWriter>::write_tag_values` (L185-194)
- Pre: CHECK
- Target: `src/client/gateway.rs:185-194`
- Action:
  Delete the `#[allow(deprecated)]` attribute (L185), `#[tracing::instrument]` attribute (L186), and the `async fn write_tag_values` delegation method (L187-194).
- Post: CHECK

Step 6: [MODIFY] `opc-da-client/src/errors/hresult.rs` — [+] `pub use windows_core::HRESULT;`, [-] `#[allow(dead_code)]`, [+] runnable doctest (L3, L64-73)
- Pre: CHECK
- Target: `src/errors/hresult.rs:3, 64-73`
- Action:
  1. Line 3: change `use windows_core::HRESULT;` to `pub use windows_core::HRESULT;`.
  2. Remove `#[allow(dead_code)]` (L65) from `format_hresult` and add runnable doctest:
     ```rust
     /// Formats an HRESULT with a hexadecimal representation and optional friendly hint.
     ///
     /// # Examples
     ///
     /// ```
     /// use opc_da_client::errors::hresult::{format_hresult, E_POINTER};
     ///
     /// let formatted = format_hresult(E_POINTER);
     /// assert!(formatted.starts_with("0x80004003"));
     /// ```
     #[must_use]
     pub fn format_hresult(hr: HRESULT) -> String {
         let hex = format!("0x{:08X}", hr.0.cast_unsigned());
         match friendly_hresult_hint(hr) {
             Some(hint) => format!("{hex}: {hint}"),
             None => hex,
         }
     }
     ```
- Post: CHECK

Step 7: [MODIFY] `opc-da-client/src/lib.rs` — [~] Unconditional `#![doc]`, [+] `ParseEndpointError`, [~] unconditional `DefaultOpcDaClient` (L2, L23-31, L35-36)
- Pre: CHECK
- Target: `src/lib.rs:2, 23-31, 35-36`
- Action:
  1. Line 2: replace `#![cfg_attr(feature = "opc-da-backend", doc = include_str!("../README.md"))]` with unconditional `#![doc = include_str!("../README.md")]`.
  2. In `pub use types::{ ... }` (L23-31), add `ParseEndpointError` to the import list.
  3. Lines 35-36: replace `#[cfg(feature = "opc-da-backend")] pub use client::DefaultOpcDaClient;` with unconditional `pub use client::DefaultOpcDaClient;`.
- Post: CHECK

Step 8: [MODIFY] `opc-da-client/README.md` — [~] Prune `write_tag_values` from deprecation table and role traits table (L612, L625)
- Pre: CHECK
- Target: `opc-da-client/README.md:612, 625`
- Action:
  1. Delete line 612 (`| TagWriter::write_tag_values | 0.2.0 | 0.4.0 / 1.0.0 | TagWriter::write_tag_batch |`) from the deprecation table.
  2. Update line 625 TagWriter entry in API Surface table: remove `, deprecated write_tag_values` from the description.
- Post: CHECK

Step 9: [TEST] Workspace integration & quality verification — `pwsh -File scripts/verify.ps1` 🔒
- Pre: ALL
- Target: Workspace root
- Action: Execute full 9-gate quality pipeline to guarantee zero warnings under `-D warnings`.
- Post: VERIFIED 🔒

---

### Verification Plan
| Type | Command | Expected Result |
|------|---------|-----------------|
| Provider Unit Tests | `cargo test -p opc-da-client --lib provider` | All provider tests pass without deprecation warnings |
| Client Unit Tests | `cargo test -p opc-da-client --lib client` | All builder and client tests pass |
| Traits Unit Tests | `cargo test -p opc-da-client --lib connector::traits` | `NoopServerBackend` tests pass |
| Headless Offline Check | `cargo check -p opc-da-client --no-default-features` | Headless build compiles cleanly with `NoopServerBackend` |
| Deprecation Audit | `rg "allow(deprecated)" opc-da-client/src/` | 0 matches |
| Doctests Suite | `cargo test -p opc-da-client --doc` | All doctests pass including `format_hresult` and `README.md` |
| Full 9-Gate Pipeline | `pwsh -File scripts/verify.ps1` | All 9 gates pass with exit code 0 |

---

### Plan Summary
| Metric | Value |
|---|---|
| Tier | M (Refactor / API Excision) |
| Files Modified | 8 (`traits.rs`, `client/mod.rs`, `client/builder.rs`, `provider.rs`, `gateway.rs`, `hresult.rs`, `lib.rs`, `README.md`) |
| Steps | 9 steps |
| Checkpoints | 1 (Step 9) |
| Review Cycles | 3 subagent + 1 inline = 4 total (all findings resolved) |
| Estimated Effort | Low-Medium |
