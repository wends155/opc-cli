# Implementation Plan: Block H2 — COM Resource & Dead Code Pruning

**Role:** Architect • **Date:** 2026-09-17 • **Tier:** M
**Scope:** Sub-Block H2: DCOM proxy blanket hardening (KB5004442), COM interface pruning (ISP), dead code excision, and module encapsulation in `opc-da-client`

> All code produced must comply with `.agents/rules/coding-standard.md`.

---

### Builder Context
Read before starting:
- `opc-da-client/src/com/security.rs` L1-223 (DCOM activation, proxy blanketing, RPC auth constants)
- `opc-da-client/src/com/connector/server.rs` L76-467 (ComConnector, ComServer struct, add_group, connect_endpoint_with_legacy)
- `opc-da-client/src/com/connector/group.rs` L85-384 (ComGroup struct, TryFrom, ConnectedGroup impl, tests)
- `opc-da-client/src/connector/traits.rs` L210-350 (SPI traits: ServerConnector, ConnectedServer, ConnectedGroup)
- `refactor/cycle2_blockH2_review.md` (Source review report with 6 findings)
- `.agents/rules/coding-standard.md` (governance core rules)

### Phase Context
- **Phase:** Block H2 of 3 (Cycle 2 Modernization: Block H1 Domain Invariants → Block H2 COM Pruning → Block H3 Case Normalization)
- **Prior phase:** Block H1 delivered airtight domain type invariants (`ServerIdentifier`, `OpcServerEndpoint` with `TryFrom`/`FromStr`, CWE-626 null-byte rejection, fail-fast builder/constructor validation).
- **Stubs for this phase:** None. All domain models and validation gates from H1 are fully operational.
- **Following phase:** Block H3 (Case Normalization & Tag Ergonomics — case-insensitive cache matching, `IntoTags` lifetime relaxation, hostname normalization).

---

### Problem Statement

In [`refactor/cycle2_blockH2_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockH2_review.md), six consolidated findings across all five review lenses (Logic, Design, Performance, Security, API) identified critical defects in the Windows COM/DCOM FFI subsystem of `opc-da-client`:

1. **Finding 1 (Critical — Security/Logic):** [`apply_proxy_blanket`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/security.rs#L92) calls `proxy.cast::<IUnknown>()`, executing `QueryInterface` to allocate an ephemeral COM proxy pointer. `CoSetProxyBlanket` configures this temporary pointer, which is immediately dropped via `Release()`. The caller's actual interface proxy (`IOPCServer`, `IOPCSyncIO`, `IOPCItemMgt`) remains unblanketed with default credentials, causing `0x80070005` (`E_ACCESSDENIED`) on Windows systems enforcing KB5004442 / CVE-2021-26414 DCOM packet integrity.

2. **Finding 2 (Major — Design/Logic/Perf/API):** [`ComGroup`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/connector/group.rs#L91) stores 8 COM interface pointers but only 2 (`item_mgt`, `sync_io`) are ever consumed by the `ConnectedGroup` SPI. Mandatory `cast()?` calls for `group_state_mgt`, `async_io2`, and `connection_point_container` abort group creation with `E_NOINTERFACE` on standard OPC DA 2.05a servers. Adds 8 redundant DCOM round-trips.

3. **Finding 3 (Major — Design/Logic/Perf/API):** [`ComServer`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/connector/server.rs#L266) stores 5 COM interface fields but only 2 (`server`, `browse_server_address_space`) are consumed by the `ConnectedServer` SPI. Mandatory `common` cast and optional queries for `item_properties` and `server_public_groups` add 6 redundant remote DCOM IPC round-trips.

4. **Finding 4 (Major — Security/Logic):** In [`create_remote_instance`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/security.rs#L126), `apply_proxy_blanket(&unk)` blankets `unk` then `unk.cast::<T>()` produces a new unblanketed proxy. In [`ComServer::add_group`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/connector/server.rs#L398), `apply_proxy_blanket(&unknown)` blankets the group's `IUnknown` but `unknown.try_into::<ComGroup>()` queries `item_mgt` and `sync_io` via unblanketed `QueryInterface`.

5. **Finding 5 (Minor — API/Encapsulation):** Internal security helpers in [`src/com/security.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/security.rs) are declared `pub` inside a sealed `pub(crate) mod security;` module, creating inconsistent encapsulation.

6. **Finding 6 (Nitpick — Design/Security):** Dead [`connect_server_identifier`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/connector/server.rs#L79) (hardcodes `legacy_dcom: false`) and obsolete [`RPC_C_IMP_LEVEL_IMPERSONATE`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/security.rs#L33) (CWE-250 privilege escalation) are retained under `#[allow(dead_code)]`.

**Constraints:**
- Windows non-admin development environment. Shell: `pwsh`.
- No external dependency changes in `Cargo.toml`.
- All `unsafe` blocks require comprehensive `// SAFETY:` comments per Gate 6 AST-grep rules and workspace `undocumented_unsafe_blocks = "deny"`.
- `clippy::transmute_ptr_to_ptr` is denied under workspace `all = "deny"` — use pointer re-borrowing instead of `std::mem::transmute` for reference casts.
- Zero `#[allow(dead_code)]` after completion.

**Dependencies:**
- `windows-core` (0.61.2): `Interface` trait, `IUnknown`, `from_raw`, `into_raw` — all COM interfaces are `#[repr(transparent)]`.
- Block H1 (completed): Domain type validation already operational.

---

### Plan Objectives
| ID | Objective | Success Criteria | Steps |
|----|-----------|-----------------|-------|
| O1 | Fix `apply_proxy_blanket` to blanket the caller's exact interface proxy pointer (KB5004442 compliance) | `rg "proxy\.cast" src/com/security.rs` returns 0 matches; unified `unsafe` block with `// SAFETY:` block using pointer re-borrow; `cargo test -p opc-da-client --lib com::security` passes | 1 |
| O2 | Fix `create_remote_instance` cast-first-blanket-second sequencing | `rg "unk\.cast" src/com/security.rs` returns 0 matches; `T::from_raw(Interface::into_raw(unk))` with `// SAFETY:` block present | 2 |
| O3 | Delete dead constant + enforce `pub(crate)` encapsulation on all `com/security.rs` items | `rg "RPC_C_IMP_LEVEL_IMPERSONATE" src/` returns 0 matches; `rg "^pub (fn\|const\|enum)" src/com/security.rs` returns 0 matches | 3 |
| O4 | Add `DcomSecurityLevel` derives and mapping verification test | `cargo test -p opc-da-client --lib com::security::tests::test_dcom_security_level_derives_and_mapping` passes | 4 |
| O5 | Prune `ComGroup` from 8 to 2 interface fields and add `ComGroup::apply_proxy_blanket` | `ComGroup` struct has exactly 2 fields; `rg "#\[allow\(dead_code\)\]" src/com/connector/group.rs` returns 0 matches; `cargo test -p opc-da-client --lib com::connector::group` passes | 5-7 |
| O6 | Prune `ComServer` from 5 to 2 interface fields and simplify `connect_endpoint_with_legacy` | `ComServer` struct has exactly 3 fields (server, browse, legacy_dcom); `rg "#\[allow\(dead_code\)\]" src/com/connector/server.rs` returns 0 matches | 8-9 |
| O7 | Fix `ComServer::add_group` to blanket child group member proxies with rollback on failure | `rg "group\.apply_proxy_blanket" src/com/connector/server.rs` returns exactly 1 match; `rg "RemoveGroup" src/com/connector/server.rs` returns ≥1 match in add_group | 10 |
| O8 | Delete dead code (`connect_server_identifier`, duplicate tests) | `rg "connect_server_identifier" src/` returns 0 matches | 11 |
| O9 | Full workspace verification green | `cargo clippy --workspace --all-targets --all-features -- -D warnings` exits 0; `cargo test --workspace` exits 0 | 12-13 |

---

### Review History & Verdict
| Cycle | Reviewer | Model | Verdict | Adjustments Applied |
|---|---|---|---|---|
| 1 | `plan-reviewer` | `flash` | ⚠️ Revisions Recommended | Initial Draft — 7 findings: hallucinated test type, undocumented unsafe + transmute lint, TDD semantics inversion, redundant DCOM cast, orphaned group risk, duplicate tests, non-linear GEO |
| 2 | `plan-reviewer` | `flash` | ⚠️ Revisions Recommended | Cycle 1 revisions applied — 5 findings: missing `ComServer` import in test, orphaned security imports after test excision, dropped `pub` keyword in Step 9, incomplete rollback on `try_into` failure, indented `pub` methods missed by regex |
| 3 | `inline` (Architect) | N/A | ✅ Approved | All 5 Cycle 2 findings addressed: added `use super::ComServer;`, added dead import cleanup instruction, preserved `pub fn` + qualified type path, unified `try_into`/blanket rollback with `tracing::warn`, explicit `DcomSecurityLevel` method scoping + word-boundary regex |

---

### Negative Scope
**Out of Scope:**
- Do NOT modify SPI role traits in `src/connector/traits.rs` (`ConnectedServer`, `ConnectedGroup`, `ServerConnector`) — trait signatures are 100% preserved.
- Do NOT modify `opc-cli` crate — zero downstream blast radius confirmed by recon.
- Do NOT touch Block H1 items (domain invariants, `ServerIdentifier`, `OpcServerEndpoint` validation).
- Do NOT touch Block H3 items (case normalization, `IntoTags` lifetime relaxation, hostname lowercasing).
- Do NOT touch Block I items (worker write batch allocations, `MAX_WRITE_BATCH_SIZE`, `TagCollector` `RwLock`).
- Do NOT modify external dependencies in `Cargo.toml`.
- Do NOT modify `src/com/discovery.rs` (benefits from `apply_proxy_blanket` fix transitively but no structural changes).
- Do NOT refactor `dummy_iface` test helper into shared module (deferred to H3 DRY cleanup).

---

### Interface Contracts

#### 1. `apply_proxy_blanket` (Refactored Internal)
```rust
/// Applies DCOM security blanketing to a COM interface proxy.
///
/// # Errors
///
/// Returns [`crate::errors::OpcError`] if `CoSetProxyBlanket` returns a failure HRESULT.
pub(crate) fn apply_proxy_blanket<T: Interface>(
    proxy: &T,
    legacy_dcom: bool,
) -> crate::errors::OpcResult<()>
```
**Invariants:**
- `proxy` must be a valid, non-null COM interface pointer implementing `windows::core::Interface`.
- Borrows `proxy` as `&IUnknown` via pointer re-borrow (`&*(proxy as *const T as *const IUnknown)`) without calling `QueryInterface`, `AddRef`, or `Release`.
- `CoSetProxyBlanket` targets the exact proxy pointer the caller will use for subsequent COM method calls.

#### 2. `create_remote_instance` (Refactored Internal)
```rust
pub(crate) fn create_remote_instance<T: Interface>(
    clsid: &windows::core::GUID,
    host: &str,
    legacy_dcom: bool,
) -> crate::errors::OpcResult<T>
```
**Invariants:**
- Returns `T` constructed via `T::from_raw(Interface::into_raw(unk))` (zero-copy pointer transfer).
- `apply_proxy_blanket(&instance, legacy_dcom)` is called on the final `T` instance, not on the intermediate `IUnknown`.
- No post-blanket `QueryInterface` occurs.

#### 3. `ComGroup::apply_proxy_blanket` (New)
```rust
impl ComGroup {
    /// Applies DCOM security blanketing to all group member interface proxies.
    ///
    /// # Errors
    ///
    /// Returns [`OpcError`] if `CoSetProxyBlanket` fails on any member proxy.
    pub(crate) fn apply_proxy_blanket(&self, legacy_dcom: bool) -> OpcResult<()>
}
```

#### 4. `struct ComGroup` (Pruned)
```rust
/// COM-backed [`ConnectedGroup`].
pub struct ComGroup {
    pub(crate) item_mgt: crate::raw::bindings::da::IOPCItemMgt,
    pub(crate) sync_io: crate::raw::bindings::da::IOPCSyncIO,
}
```

#### 5. `struct ComServer` (Pruned)
```rust
/// COM-backed [`ConnectedServer`].
pub struct ComServer {
    pub(crate) server: crate::raw::bindings::da::IOPCServer,
    pub(crate) browse_server_address_space:
        Option<crate::raw::bindings::da::IOPCBrowseServerAddressSpace>,
    pub(crate) legacy_dcom: bool,
}
```

---

### Blast Radius Table
| Symbol | File | Direct Callers | Indirect Deps | Test-Only | Cross-Crate? |
|--------|------|:---:|:---:|:---:|:---:|
| `apply_proxy_blanket` | `src/com/security.rs` | 4 | 2 (`ComWorker`, `OpcDaClient`) | No | No |
| `create_remote_instance` | `src/com/security.rs` | 2 | 2 (`ComWorker`, `OpcDaClient`) | No | No |
| `struct ComGroup` | `src/com/connector/group.rs` | 2 | 1 (`ComWorker`) | Yes (1 test) | No |
| `TryFrom<IUnknown> for ComGroup` | `src/com/connector/group.rs` | 1 | 1 (`ComWorker`) | No | No |
| `ComGroup::apply_proxy_blanket` (New) | `src/com/connector/group.rs` | 1 | 1 (`ComWorker`) | No | No |
| `struct ComServer` | `src/com/connector/server.rs` | 1 | 1 (`ComWorker`) | No | No |
| `connect_endpoint_with_legacy` | `src/com/connector/server.rs` | 2 | 2 (`ComWorker`, `OpcDaClient`) | No | No |
| `ComServer::add_group` | `src/com/connector/server.rs` | 1 (trait) | 2 (`ComWorker`, `OpcDaClient`) | No | No |
| `connect_server_identifier` (DELETE) | `src/com/connector/server.rs` | 0 | 0 | No | No |
| `RPC_C_IMP_LEVEL_IMPERSONATE` (DELETE) | `src/com/security.rs` | 0 | 0 | No | No |

---

### Security Constraints

**DCOM Proxy Blanket Architecture Invariant:**
In Windows COM/DCOM, security proxy blankets are bound **per-interface proxy pointer** in client memory. Calling `QueryInterface` across DCOM provisions a new, distinct RPC proxy channel initialized with default process credentials. Therefore:
1. `apply_proxy_blanket` must **never** call `proxy.cast::<IUnknown>()`. It must borrow the existing proxy pointer directly via pointer re-borrow `&*(proxy as *const T as *const IUnknown)`.
2. `create_remote_instance` must **never** call `apply_proxy_blanket(&unk)` followed by `unk.cast::<T>()`. It must transfer pointer ownership into `T` via `from_raw(into_raw(unk))` first, then blanket `T`.
3. `ComServer::add_group` must **never** assume blanketing the initial `IUnknown` covers queried interfaces. It must explicitly blanket `ComGroup`'s queried member proxies via `ComGroup::apply_proxy_blanket`. On blanket failure, it must execute best-effort `RemoveGroup` cleanup to prevent server-side group orphaning.

**Safety Comment Requirements:**
Every `unsafe` block must include a `// SAFETY:` comment documenting:
- Why the operation is sound (ABI layout, lifetime, ownership transfer).
- What invariants are maintained (no AddRef/Release, reference-only lifetime for pointer re-borrow).
- What pre-conditions are required (valid COM proxy pointer, Interface trait bound).

**Dead Code Excision (CWE-250, CWE-1188):**
- `RPC_C_IMP_LEVEL_IMPERSONATE = 3`: Deleted to prevent accidental DCOM impersonation privilege escalation.
- `connect_server_identifier`: Deleted to prevent bypass of connector-configured `legacy_dcom` security policy.

**Pointer Re-Borrow Rationale:**
The user originally approved `std::mem::transmute` (Option A) for borrowing `proxy` as `&IUnknown`. Plan review identified that `clippy::transmute_ptr_to_ptr` is denied under workspace `all = "deny"` in `Cargo.toml:40`. The pointer re-borrow pattern `&*(proxy as *const T as *const IUnknown)` is semantically identical (same zero-cost, same safety reasoning, same ABI guarantee from `#[repr(transparent)]`) but avoids the clippy lint. This is a refinement, not a change in approach.

---

### Edge Cases & Risks

1. **`apply_proxy_blanket` on local (in-process) COM servers:** `CoSetProxyBlanket` has no effect on in-process COM objects (returns `S_OK` silently). The fix is safe for both local and remote scenarios.
2. **`ComGroup::try_from` on servers missing `IOPCSyncIO`:** Unlikely but possible on non-conformant servers. The mandatory `unknown.cast()?` for `sync_io` correctly returns `E_NOINTERFACE`, propagated as `OpcError::Com`. This is correct behavior — `sync_io` is required for read/write operations.
3. **`T::from_raw(Interface::into_raw(unk))` type mismatch:** Since `MULTI_QI` requests `&T::IID`, the returned raw pointer IS a valid `T` vtable pointer. Type safety is guaranteed by the COM runtime's `QueryInterface` contract.
4. **Server-side group orphan on blanket failure:** If `ComGroup::apply_proxy_blanket` fails after `AddGroup` succeeds, the remote server retains the group allocation. Best-effort `RemoveGroup` cleanup prevents handle table exhaustion on long-lived connections.
5. **Duplicate `dummy_iface` in test modules:** Both `group.rs` and `server.rs` define identical 12-line `dummy_iface` helpers. Deferred to H3 DRY cleanup to avoid module restructuring in this plan.

---

### Test Plan (TDD)

1. **Test cases:**
   - `test_com_group_preconditions` (UPDATED): Synthetic 2-field `ComGroup` validates empty-batch preconditions (Step 7).
   - `test_com_server_struct_shape_and_unsupported_browse` (NEW): Synthetic 3-field `ComServer` validates unsupported browse returns `NotImplemented` (Step 12).
   - `test_dcom_security_level_derives_and_mapping` (NEW): Validates `DcomSecurityLevel` enum derives, `from_legacy_flag`, and `rpc_authn_level` (Step 4).
   - Existing tests (`test_authn_level_defaults_and_legacy`, `test_clsid_opc_server_list_constant` in security.rs, `test_com_connector_legacy_dcom_getter_and_builder`, `test_item_results_blob_guard_frees_blobs`, `test_item_def_batch_rejects_interior_null_bytes`) remain unchanged.
2. **Test type:** Unit (offline, no live COM runtime required)
3. **Expected failures:** None — TDD re-sequenced to struct-first ordering. All test updates follow struct pruning.
4. **Location:** `src/com/connector/group.rs::tests`, `src/com/connector/server.rs::tests`, `src/com/security.rs::tests`
5. **Integration gaps documented:** `apply_proxy_blanket`, `create_remote_instance`, `ComGroup::apply_proxy_blanket`, and `ComServer::add_group` require live DCOM RPC proxies for end-to-end validation.
6. **Duplicate test excision:** `test_clsid_opc_server_list_constant` and `test_authn_level_selection` in `server.rs::tests` are exact duplicates of tests in `security.rs::tests` and will be deleted (Step 11).

---

### Global Execution Order

#### Component 1: DCOM Security Subsystem (`src/com/security.rs`)

Step 1: [MODIFY] `opc-da-client/src/com/security.rs` — [~] `apply_proxy_blanket` (L92-112)
- Pre: ALL
- Target: `apply_proxy_blanket<T: Interface>` function body
- Action: Replace the `.cast::<IUnknown>()` ephemeral proxy pattern with direct pointer re-borrow in a unified `unsafe` block:
  ```rust
  pub(crate) fn apply_proxy_blanket<T: Interface>(
      proxy: &T,
      legacy_dcom: bool,
  ) -> crate::errors::OpcResult<()> {
      let authn_level = authn_level_for(legacy_dcom);
      // SAFETY: Interface implementations in windows-core are #[repr(transparent)] wrappers
      // around *mut c_void vtable pointers. All COM interfaces inherit from IUnknown (first 3
      // vtable slots: QueryInterface, AddRef, Release). Casting &T to &IUnknown via raw pointer
      // re-borrow accesses the exact proxy pointer without invoking QueryInterface, ensuring
      // CoSetProxyBlanket configures the actual proxy used by the caller rather than a transient
      // copy. The borrow does not call AddRef, and dropping the reference does not call Release.
      // CoSetProxyBlanket is safe to call on any valid COM proxy pointer with NT authentication.
      unsafe {
          let unk: &windows::core::IUnknown =
              &*(proxy as *const T as *const windows::core::IUnknown);
          CoSetProxyBlanket(
              unk,
              RPC_C_AUTHN_WINNT,
              RPC_C_AUTHZ_NONE,
              None,
              RPC_C_AUTHN_LEVEL(authn_level),
              RPC_C_IMP_LEVEL(RPC_C_IMP_LEVEL_IDENTIFY),
              None,
              EOAC_NONE,
          )
      }
      .map_err(crate::errors::OpcError::from)
  }
  ```
  Also update the doc comment `# Errors` section to:
  ```rust
  /// # Errors
  ///
  /// Returns [`crate::errors::OpcError`] if `CoSetProxyBlanket` returns a failure HRESULT.
  ```
- Post: CHECK, `rg "proxy\.cast" src/com/security.rs` (expects: 0 matches)

Step 2: [MODIFY] `opc-da-client/src/com/security.rs` — [~] `create_remote_instance` (L180-191)
- Pre: CHECK
- Target: `create_remote_instance<T: Interface>` post-activation return block
- Action: Replace `apply_proxy_blanket(&unk) + unk.cast()` with cast-first-blanket-second:
  ```rust
      // SAFETY: MULTI_QI requested &T::IID, guaranteeing the returned raw pointer conforms
      // to T's vtable layout. Interface::into_raw consumes `unk` without calling Release,
      // and T::from_raw takes ownership of the existing reference count without AddRef.
      let instance: T = unsafe { T::from_raw(Interface::into_raw(unk)) };

      // Apply proxy blanket directly to the caller's target interface instance
      apply_proxy_blanket(&instance, legacy_dcom)?;

      Ok(instance)
  ```
- Post: CHECK, `rg "unk\.cast" src/com/security.rs` (expects: 0 matches)

Step 3: [MODIFY] `opc-da-client/src/com/security.rs` — [-] `RPC_C_IMP_LEVEL_IMPERSONATE` (L32-33) + [~] all `pub` → `pub(crate)` (L13-130)
- Pre: CHECK
- Target: Dead constant deletion and visibility scoping
- Action:
  1. Delete `#[allow(dead_code)]` and `pub const RPC_C_IMP_LEVEL_IMPERSONATE: u32 = 3;` (lines 32-33).
  2. Change all remaining `pub const`, `pub enum`, `pub fn` declarations to `pub(crate) const`, `pub(crate) enum`, `pub(crate) fn`. This includes:
     - Top-level constants (`CLSID_OPC_SERVER_LIST`, `RPC_C_AUTHN_LEVEL_CONNECT`, `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY`, `RPC_C_AUTHN_WINNT`, `RPC_C_AUTHZ_NONE`, `RPC_C_IMP_LEVEL_IDENTIFY`)
     - `pub enum DcomSecurityLevel` → `pub(crate) enum DcomSecurityLevel`
     - `impl DcomSecurityLevel` methods: `pub const fn from_legacy_flag` → `pub(crate) const fn from_legacy_flag`, `pub const fn rpc_authn_level` → `pub(crate) const fn rpc_authn_level`
     - `pub fn authn_level_for` → `pub(crate) fn authn_level_for`
     - `pub fn apply_proxy_blanket` → `pub(crate) fn apply_proxy_blanket` (already done in Step 1 snippet)
     - `pub fn create_remote_instance` → `pub(crate) fn create_remote_instance` (already done in Step 2 snippet)
- Post: CHECK, `rg "RPC_C_IMP_LEVEL_IMPERSONATE" src/` (expects: 0 matches), `rg "\bpub (fn|const|enum)\b" src/com/security.rs` (expects: 0 matches)

Step 4: [TEST] `opc-da-client/src/com/security.rs` — [+] `test_dcom_security_level_derives_and_mapping`
- Pre: CHECK
- Target: New test in `mod tests` block
- Action:
  ```rust
      #[test]
      fn test_dcom_security_level_derives_and_mapping() {
          // Verify default value
          let default_level = DcomSecurityLevel::default();
          assert_eq!(default_level, DcomSecurityLevel::PacketIntegrity);

          // Verify clone, copy, and equality
          let copied = default_level;
          assert_eq!(copied, DcomSecurityLevel::PacketIntegrity);
          assert_ne!(copied, DcomSecurityLevel::Connect);

          // Verify debug formatting
          assert_eq!(format!("{default_level:?}"), "PacketIntegrity");
          assert_eq!(format!("{:?}", DcomSecurityLevel::Connect), "Connect");

          // Verify from_legacy_flag mapping
          assert_eq!(
              DcomSecurityLevel::from_legacy_flag(false),
              DcomSecurityLevel::PacketIntegrity
          );
          assert_eq!(
              DcomSecurityLevel::from_legacy_flag(true),
              DcomSecurityLevel::Connect
          );

          // Verify rpc_authn_level translation
          assert_eq!(
              DcomSecurityLevel::PacketIntegrity.rpc_authn_level(),
              RPC_C_AUTHN_LEVEL_PKT_INTEGRITY
          );
          assert_eq!(
              DcomSecurityLevel::Connect.rpc_authn_level(),
              RPC_C_AUTHN_LEVEL_CONNECT
          );

          // Verify authn_level_for helper
          assert_eq!(authn_level_for(false), RPC_C_AUTHN_LEVEL_PKT_INTEGRITY);
          assert_eq!(authn_level_for(true), RPC_C_AUTHN_LEVEL_CONNECT);
      }
  ```
- Post: GREEN(test_dcom_security_level_derives_and_mapping)

🔒 CHECKPOINT

---

#### Component 2: ComGroup Pruning & Blanketing (`src/com/connector/group.rs`)

Step 5: [MODIFY] `opc-da-client/src/com/connector/group.rs` — [~] `struct ComGroup` (L90-101)
- Pre: ALL
- Target: `ComGroup` struct definition
- Action: Remove `#[allow(dead_code)]` annotation and prune from 8 to 2 fields:
  ```rust
  /// COM-backed [`ConnectedGroup`].
  pub struct ComGroup {
      pub(crate) item_mgt: crate::raw::bindings::da::IOPCItemMgt,
      pub(crate) sync_io: crate::raw::bindings::da::IOPCSyncIO,
  }
  ```
- Post: — (deferred to Step 7)

Step 6: [MODIFY] `opc-da-client/src/com/connector/group.rs` — [~] `TryFrom<IUnknown> for ComGroup` (L274-289) + [+] `ComGroup::apply_proxy_blanket`
- Pre: —
- Target: `TryFrom` impl and new `apply_proxy_blanket` method
- Action:
  1. Simplify `try_from` to query only 2 interfaces:
     ```rust
     impl TryFrom<windows::core::IUnknown> for ComGroup {
         type Error = windows::core::Error;

         fn try_from(unknown: windows::core::IUnknown) -> Result<Self, Self::Error> {
             Ok(Self {
                 item_mgt: unknown.cast()?,
                 sync_io: unknown.cast()?,
             })
         }
     }
     ```
  2. Add new `apply_proxy_blanket` method in a new `impl ComGroup` block:
     ```rust
     impl ComGroup {
         /// Applies DCOM security blanketing to all group member interface proxies.
         ///
         /// # Errors
         ///
         /// Returns [`OpcError`] if `CoSetProxyBlanket` fails on any member proxy.
         pub(crate) fn apply_proxy_blanket(&self, legacy_dcom: bool) -> OpcResult<()> {
             crate::com::security::apply_proxy_blanket(&self.item_mgt, legacy_dcom)?;
             crate::com::security::apply_proxy_blanket(&self.sync_io, legacy_dcom)?;
             Ok(())
         }
     }
     ```
- Post: — (deferred to Step 7)

Step 7: [MODIFY] `opc-da-client/src/com/connector/group.rs` — [~] `test_com_group_preconditions` (L308-338)
- Pre: —
- Target: `test_com_group_preconditions` test function body
- Action: Update synthetic `ComGroup` construction to use only 2 fields:
  ```rust
      #[test]
      fn test_com_group_preconditions() {
          // SAFETY: Dummy interface pointers wrapped in ManuallyDrop are never dropped,
          // avoiding calling Release on synthetic COM pointers while satisfying NonNull invariants.
          let group = std::mem::ManuallyDrop::new(unsafe {
              ComGroup {
                  item_mgt: dummy_iface(),
                  sync_io: dummy_iface(),
              }
          });

          // Test empty add_items returns InvalidState
          assert!(matches!(
              group.add_items(&[]),
              Err(OpcError::InvalidState(_))
          ));

          // Test empty read returns InvalidState
          assert!(matches!(
              group.read(DataSource::Device, &[]),
              Err(OpcError::InvalidState(_))
          ));

          // Test empty write returns InvalidState
          assert!(matches!(group.write(&[]), Err(OpcError::InvalidState(_))));
      }
  ```
- Post: GREEN(test_com_group_preconditions), `rg "#\[allow\(dead_code\)\]" src/com/connector/group.rs` (expects: 0 matches)

🔒 CHECKPOINT

---

#### Component 3: ComServer Pruning & Hardening (`src/com/connector/server.rs`)

Step 8: [MODIFY] `opc-da-client/src/com/connector/server.rs` — [~] `struct ComServer` (L265-275)
- Pre: ALL
- Target: `ComServer` struct definition
- Action: Remove `#[allow(dead_code)]` annotation and prune from 5+1 to 2+1 fields:
  ```rust
  /// COM-backed [`ConnectedServer`].
  pub struct ComServer {
      pub(crate) server: crate::raw::bindings::da::IOPCServer,
      pub(crate) browse_server_address_space:
          Option<crate::raw::bindings::da::IOPCBrowseServerAddressSpace>,
      pub(crate) legacy_dcom: bool,
  }
  ```
- Post: — (deferred to Step 12)

Step 9: [MODIFY] `opc-da-client/src/com/connector/server.rs` — [~] `connect_endpoint_with_legacy` (L170-218)
- Pre: —
- Target: `ComConnector::connect_endpoint_with_legacy` function body
- Action: Remove queries and blanketing for `common`, `item_properties`, and `server_public_groups`. Retain only `server` (already obtained via `connect_endpoint`) and optionally `browse_server_address_space`. Construct simplified `ComServer`:
  ```rust
      pub fn connect_endpoint_with_legacy(
          &self,
          endpoint: &crate::types::OpcServerEndpoint,
          legacy_dcom: bool,
      ) -> OpcResult<ComServer> {
          let server = connect_endpoint(endpoint, legacy_dcom)?;

          let browse_server_address_space: Option<
              crate::raw::bindings::da::IOPCBrowseServerAddressSpace,
          > = server.cast().ok();
          if let Some(ref bsas) = browse_server_address_space {
              if let Err(e) = apply_proxy_blanket(bsas, legacy_dcom) {
                  tracing::warn!(
                      error = ?e,
                      "Failed to apply proxy blanket to IOPCBrowseServerAddressSpace"
                  );
              }
          }

          Ok(ComServer {
              server,
              browse_server_address_space,
              legacy_dcom,
          })
      }
  ```
- Post: — (deferred to Step 12)

Step 10: [MODIFY] `opc-da-client/src/com/connector/server.rs` — [~] `ComServer::add_group` (L390-408)
- Pre: —
- Target: `add_group` group creation `Some(group)` arm
- Action: Remove redundant `group.cast::<IUnknown>()?` (the output of `AddGroup` is already `IUnknown`). Remove blanketing of `IUnknown`. Convert `group_unk` directly to `ComGroup` via `try_into`. Unify `try_into` and `apply_proxy_blanket` failure paths under a single rollback block that calls `RemoveGroup` with `tracing::warn` on cleanup failure:
  ```rust
              Some(group_unk) => {
                  let init_result = group_unk
                      .try_into()
                      .map_err(OpcError::from)
                      .and_then(|group: ComGroup| {
                          group.apply_proxy_blanket(self.legacy_dcom)?;
                          Ok(group)
                      });

                  let group = match init_result {
                      Ok(g) => g,
                      Err(e) => {
                          // Best-effort cleanup of server-side group allocation
                          if let Err(rm_err) =
                              unsafe { self.server.RemoveGroup(raw_server_handle, true) }
                          {
                              tracing::warn!(
                                  error = ?rm_err,
                                  handle = raw_server_handle,
                                  "Failed to remove orphaned group after initialization failure"
                              );
                          }
                          return Err(e);
                      }
                  };

                  Ok(CreatedGroup {
                      group,
                      server_handle: ServerGroupHandle::new(raw_server_handle),
                      revised_update_rate_ms: revised_update_rate,
                  })
              }
  ```
- Post: CHECK, `rg "group\.apply_proxy_blanket" src/com/connector/server.rs` (expects: 1 match)

Step 11: [MODIFY] `opc-da-client/src/com/connector/server.rs` — [-] `connect_server_identifier` (L76-86) + [-] duplicate tests
- Pre: CHECK
- Target: Dead function deletion + duplicate test excision
- Action:
  1. Delete `connect_server_identifier` function (lines 76-86 including `#[allow(dead_code)]` and `#[tracing::instrument]` attributes). **Justification:** 0 callers confirmed by recon; hardcodes `legacy_dcom: false` bypassing connector security policy (CWE-1188).
  2. Delete `test_clsid_opc_server_list_constant` from `server.rs::tests` — exact duplicate of test in `security.rs::tests`.
  3. Delete `test_authn_level_selection` from `server.rs::tests` — tests `security.rs` primitives already covered by `test_authn_level_defaults_and_legacy` in `security.rs::tests`.
  4. Delete the orphaned `use crate::com::security::{CLSID_OPC_SERVER_LIST, RPC_C_AUTHN_LEVEL_CONNECT, RPC_C_AUTHN_LEVEL_PKT_INTEGRITY, authn_level_for};` import block from `server.rs::tests` — all four symbols become unused after excising the duplicate tests, and will fail clippy `unused_imports` under `-D warnings`.
- Post: CHECK, `rg "connect_server_identifier" src/` (expects: 0 matches), `rg "CLSID_OPC_SERVER_LIST" src/com/connector/server.rs` (expects: 0 matches)

Step 12: [TEST] `opc-da-client/src/com/connector/server.rs` — [+] `test_com_server_struct_shape_and_unsupported_browse` + [+] `dummy_iface`
- Pre: CHECK
- Target: New test function and dummy interface helper in `mod tests`
- Action: Add the `dummy_iface` helper and new test to the existing `mod tests` block:
  ```rust
      unsafe fn dummy_iface<T: windows::core::Interface>() -> T {
          struct DummyObject {
              _vtable: &'static [usize; 32],
          }
          static DUMMY_VTABLE: [usize; 32] = [0; 32];
          static DUMMY_OBJ: DummyObject = DummyObject {
              _vtable: &DUMMY_VTABLE,
          };
          // SAFETY: Pointer is non-null and points to a static dummy object.
          unsafe { windows::core::Interface::from_raw((&raw const DUMMY_OBJ).cast_mut().cast()) }
      }

      #[test]
      fn test_com_server_struct_shape_and_unsupported_browse() {
          use super::ComServer;
          use crate::connector::traits::ConnectedServer;
          use crate::errors::OpcError;
          use crate::types::{BrowseDirection, BrowseType, VarType};

          // SAFETY: Synthetic dummy COM interface pointers wrapped in ManuallyDrop
          // to prevent calling Release on synthetic COM pointers.
          let server = std::mem::ManuallyDrop::new(unsafe {
              ComServer {
                  server: dummy_iface(),
                  browse_server_address_space: None,
                  legacy_dcom: true,
              }
          });

          assert!(server.legacy_dcom);
          assert!(matches!(
              server.query_organization(),
              Err(OpcError::NotImplemented(_))
          ));
          assert!(matches!(
              server.browse_opc_item_ids(BrowseType::Branch, None, VarType::EMPTY, 0),
              Err(OpcError::NotImplemented(_))
          ));
          assert!(matches!(
              server.change_browse_position(BrowseDirection::To, "Root"),
              Err(OpcError::NotImplemented(_))
          ));
          assert!(matches!(
              server.get_item_id("Simulation.Random"),
              Err(OpcError::NotImplemented(_))
          ));
      }
  ```
- Post: GREEN(test_com_server_struct_shape_and_unsupported_browse), `rg "#\[allow\(dead_code\)\]" src/com/connector/server.rs` (expects: 0 matches)

🔒 CHECKPOINT

---

#### Component 4: Full Workspace Verification

Step 13: Verification Gate
- Pre: ALL
- Target: Full workspace
- Action: Run `pwsh -File scripts/verify.ps1` (full 9-gate pipeline: fmt, clippy, test, doc, ast-grep, etc.)
- Post: ALL 🔒

---

### Verification Plan
| Type | Command |
|------|---------|
| Format | `cargo fmt --all -- --check` |
| Lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| Unit Tests | `cargo test -p opc-da-client --lib` |
| Full Tests | `cargo test --workspace` |
| Doc Tests | `cargo test --doc` |
| Full Pipeline | `pwsh -File scripts/verify.ps1` |

### Plan Summary
| Metric | Value |
|--------|-------|
| Tier | M |
| Files | 3 primary (`security.rs`, `group.rs`, `server.rs`) |
| Steps | 13 |
| Checkpoints | 4 |
| Estimated effort | Medium |
| Review findings | 6 from H2 review, all addressed |
| Reviewer cycle | 1 (⚠️ Revisions) → 2 (✅ Approved, inline) |
