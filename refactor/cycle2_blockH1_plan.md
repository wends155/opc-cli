# Implementation Plan: Block H1: Domain Invariants & CWE-626 Hardening

**Role:** Architect • **Date:** 2026-09-16 • **Tier:** M  
**Scope:** Domain Invariants (`ServerIdentifier`, `OpcServerEndpoint`), CWE-626 Null-Byte Rejection (`LocalPointer`, `OpcServerEndpoint::remote`), Fail-Early Constructor Validation (`bind_new`, `bind_new_remote`), Builder Non-Clobbering Deferred Error Accumulation (`OpcDaClientBuilder::server`), and Comprehensive Test Suite Migration in `opc-da-client`

### Builder Context
Read before starting:
- `opc-da-client/src/types/server.rs` (L20-45 `normalize_host_str`, L65-96 `ParseServerIdError`/`ParseEndpointError`, L104-120 existing `From` for `OpcError`, L125-285 `ServerIdentifier` & `from_str`/`From`, L440-645 `OpcServerEndpoint`, `local`, `remote`, `from_str`/`From`)
- `opc-da-client/src/raw/memory.rs` (L560-645 `LocalPointer` & `From<S>`)
- `opc-da-client/src/com/connector/server.rs` (L35, 312, 335, 353, 367 `LocalPointer::from` call sites)
- `opc-da-client/src/com/security.rs` (L135 `LocalPointer::from(host)`)
- `opc-da-client/src/client/mod.rs` (L30-45 `DefaultBackendConnector`, L85-135 `bind_new`, `bind_new_remote`, `new`)
- `opc-da-client/src/client/builder.rs` (L20-40 `OpcDaClientBuilder`, L38-46 `new`, L78-86 `new_with_connector`, L111-122 `with_connector`, L85-115 `.server()`, L136-245 `build_internal`, `build`, `build_with_connector`, `build_bound`)
- `opc-da-client/src/errors.rs` (L90-110 `OpcError`, adding `From<Infallible>`)
- `opc-da-client/README.md` (L257 doctest)
- `refactor/cycle2_blockH_review.md` (Findings #3, #5, #6, #9)
- `.agents/rules/coding-standard.md` (governance core rules, zero forbidden macros, zero compiler warnings)

### Phase Context
- **Phase:** 1 of 3 (Cycle 2 Modernization: Block H — Phase 1: Sub-Block H1 Domain Invariants & CWE-626 Hardening)
- **Prior phase:** Modernization Block G (Blocks G1 & G2) completed API excision, struct deduplication, and public documentation.
- **Stubs for this phase:** None (production domain types and client constructors).
- **Following phase:** Sub-Block H2 (COM Interface Pruning & Security Blanketing — Findings #1, #4, #10) $\rightarrow$ Sub-Block H3 (Case Normalization & Tag Ergonomics — Findings #2, #7, #8).

### Problem Statement
In `refactor/cycle2_blockH_review.md`, four critical security and input validation flaws were identified in `opc-da-client`:
1. **Finding #3 (Major - Security / API / Logic)**: `ServerIdentifier::from_str` uses `trimmed.starts_with('{') || trimmed.contains('-')` to discriminate CLSIDs from ProgIDs. This falsely diverts valid industrial ProgIDs with hyphens (e.g. `KEPServerEX-V6.1`, `ABB.IndustrialIT-Server.1`) to `Clsid::from_str`, returning `InvalidClsid`. Infallible `From<&str>` uses `unwrap_or_else` to swallow syntax errors into invalid `ProgId` variants, bypassing CWE-20 input validation and blocking standard `TryFrom` implementations.
2. **Finding #5 (Major - Security)**: `LocalPointer<Vec<u16>>::from` does not check for interior null bytes (`\0`). When passed to Win32 COM APIs (`CoCreateInstanceEx` via `COSERVERINFO.pwszName` and `CLSIDFromProgID`), strings truncate at the null byte (CWE-626), leading to target host confusion and security bypasses. Similarly, host strings with interior nulls must be strictly rejected rather than silently normalized to localhost (`None`).
3. **Finding #6 (Major - API / Logic)**: Infallible `From<&str>` on `OpcServerEndpoint` catches parse errors with `unwrap_or_else` and silently falls back to local endpoints, masking syntax errors. `OpcDaClient::bind_new` only accepts `Into<ServerIdentifier>`, preventing callers from passing remote UNC paths (`r"\\host\server"`) or URI strings (`opc.da://host/server`) directly.
4. **Finding #9 (Minor - Design / Security)**: Both `OpcDaClient::bind_new` and `OpcDaClientBuilder::build_bound` eagerly initialize COM MTA apartments and spawn OS worker threads before validating server endpoint invariants, causing resource leakage on invalid inputs.

In accordance with user interview decisions and Cycles 1, 2, and 3 review reconciliations:
- `ServerIdentifier` and `OpcServerEndpoint` eliminate infallible `From<&str>` and `From<String>`, replacing them with strict `TryFrom`.
- `OpcServerEndpoint::local_prog_id` and `remote_prog_id` are introduced as convenience helpers for raw ProgID strings. `OpcServerEndpoint::local` and `remote` retain `impl Into<ServerIdentifier>`, cleanly supporting `ServerIdentifier`, `Clsid`, and `GUID` while rejecting `&str` and `String` at compile time. Symmetrical `OpcServerEndpoint::new(s: &str)` is provided. All test call sites and doctests across the workspace are cleanly migrated.
- `OpcDaClientBuilder::server` and `host` adopt non-clobbering deferred error accumulation (`self.server_err.get_or_insert_with(...)`), preserving fluent builder chaining while `build_internal` enforces validation before worker thread spawning across `build()`, `build_bound()`, and `build_with_connector()`.
- Generic trait bounds on `build_internal` and `build_with_connector` remain unconstrained (`C: ServerBackend + 'static`), with `C: Default` confined to `build()` and `build_bound()`. `OpcDaClientBuilder::new()` remains anchored on `DefaultBackendConnector` to prevent `E0282` inference failures.
- `OpcDaClient::bind_new` is anchored on `impl OpcDaClient<DefaultBackendConnector, Unbound>` to preserve type inference (`OpcDaClient::bind_new("...")`), accepts `impl TryInto<OpcServerEndpoint, Error: Into<OpcError>>`, auto-detecting remote UNC/URI or local endpoints and validating them *before* spawning worker threads.
- `LocalPointer` eliminates unchecked `From<S>`, providing `LocalPointer::try_from_str` and `TryFrom<&str>` returning `OpcError::InvalidState` on interior null bytes. All 5 active call sites in `com/connector/server.rs` and 1 in `com/security.rs` are converted to `try_from_str(...)`.
- `normalize_host_str` does not treat null bytes as localhost; explicit `\0` rejection with `Err` is enforced at all endpoint ingestion boundaries.

### Plan Objectives
| ID | Objective | Success Criteria | Steps |
|----|-----------|-----------------|-------|
| O1 | Remediate hyphen heuristic discrimination flaw in `ServerIdentifier::from_str` (Finding #3) | `Clsid::parse` used directly; industrial ProgIDs with hyphens (`KEPServerEX-V6.1`) parse cleanly as `ProgId`; CLSIDs parse as `Clsid` | 1, 2 |
| O2 | Remediate CWE-626 null-byte truncation in `LocalPointer` and endpoint host parsing (Finding #5) | `LocalPointer::try_from_str` and `TryFrom<&str>` reject `\0` with `OpcError::InvalidState`; unchecked `From<S>` removed; all 5 callers in `com/connector/server.rs` and 1 in `com/security.rs` migrated; null-injected hosts rejected with `Err` | 3, 4, 5, 6 |
| O3 | Clean Slate removal of infallible `From<&str>` and `From<String>` on domain types (Findings #3, #6) | Infallible `From` deleted on `ServerIdentifier` and `OpcServerEndpoint`; `TryFrom<&str>` and `TryFrom<String>` implemented; symmetrical `OpcServerEndpoint::new` added; zero validation bypasses | 7, 9 |
| O4 | Add `local_prog_id`/`remote_prog_id` and migrate all test call sites and doctests | `local_prog_id` and `remote_prog_id` available; all 80 call sites in `worker/`, `client/tests.rs`, `mock/tests.rs`, integration tests (`tests/typestate_client_test.rs`), and doctests compile and pass | 9, 10, 11 |
| O5 | Fail-early validation, anchored type inference, and unified endpoint parsing in `bind_new` and `bind_new_remote` (Findings #6, #9) | `bind_new` anchored on `DefaultBackendConnector`, accepts `impl TryInto<OpcServerEndpoint>`, parses UNC/URI/local endpoints, and validates *before* worker thread init; `bind_new_remote` validates host and server identifier early | 7, 9 |
| O6 | Builder non-clobbering deferred error accumulation and fail-early validation in `build_internal` (Findings #6, #9) | `OpcDaClientBuilder::server` and `host` use `get_or_insert_with`; `build_internal` checks error, guaranteeing `build()`, `build_bound()`, and `build_with_connector()` return `Err` without spawning worker threads on invalid inputs | 7, 8, 9 |
| O7 | Full workspace 9-gate quality verification green | `pwsh -File scripts/verify.ps1` exits 0 with zero warnings under `-D warnings` | 12 |

### Review History & Verdict
| Cycle | Reviewer | Model | Verdict | Adjustments Applied |
|---|---|---|---|---|
| 1 | `plan-reviewer` | `flash` | ⚠️ Revisions Recommended | (1) Removed duplicate `From` error conversions in `errors.rs` (added only `From<Infallible>`). (2) Added `com/connector/server.rs` and migrated 5 `LocalPointer::from` call sites. (3) Added `OpcServerEndpoint::local_prog_id`, migrated 76+ test sites, and updated `README.md:257` doctest. (4) Anchored `bind_new` on `DefaultBackendConnector` for type inference. (5) Rejected null bytes in hosts with `Err` instead of mapping to `None`. (6) Moved `server_err` check into `build_internal`. (7) Added 3 intermediate checkpoints. |
| 2 | `plan-reviewer` | `flash` | ⚠️ Revisions Recommended | (1) Fixed builder error clobbering via `get_or_insert_with` in `server()` and `host()`. (2) Kept `build_internal` and `build_with_connector` in `impl<C: ServerBackend + 'static>`, only requiring `C: Default` on `build()` and `build_bound()`. (3) Initialized `server_err: None` in `new()`/`new_with_connector()`, propagated across `with_connector()`. (4) Added `remote_prog_id`, updated doctests at `src/types/server.rs:487, 520`, and updated test callers in `server.rs`, `mock/tests.rs`, and `typestate.rs`. (5) Restored DRY delegation `self.build()?` in `build_bound()`. (6) Retained `local(impl Into<ServerIdentifier>)` preserving `Clsid`/`GUID` ergonomics. (7) Fully enumerated all test files in Step 10. |
| 3 | `plan-reviewer` | `flash` | ⚠️ Revisions Recommended | Applied all 6 Cycle 3 adjustments: (1) Confined `OpcDaClientBuilder::new()` strictly to `impl OpcDaClientBuilder<DefaultBackendConnector>` to eliminate `E0282` inference ambiguity. (2) Updated `src/types/server.rs:519` doctest to `local_prog_id("Server")`. (3) Added `tests/typestate_client_test.rs` (L13, 42, 55, 102) to Step 10. (4) Added `OpcServerEndpoint::new(s: &str) -> Result<Self, ParseEndpointError> { s.parse() }` symmetrically with `ServerIdentifier::new`. (5) Added `server.rs:810` and `870` to Step 10. (6) Borrowed `server_err` via `.as_ref()` in `build_bound()` to prevent partial moves. |
| 4 | `plan-reviewer` + `codebase_recon` | `flash` | ⚠️ Revisions Recommended | Applied 5 Cycle 4 adjustments: (1) **Topological reordering** — merged old Steps 8 (From deletion), 12 (bind_new), 14 (From\<Infallible\>), and 15 (builder) into atomic Step 9, preventing 28 `.server("...")` compilation failures at the Step 10 checkpoint. (2) **Step 3 API fix** — changed `OpcServerEndpoint::new()` (not yet defined) to `.parse::<OpcServerEndpoint>()` in null-byte rejection test. (3) **14 missing call sites** — added `src/com/worker/tests.rs` L957, 995, 1024, 1028, 1083, 1123, 1163, 1199, 1220, 1259, 1268, 1289, 1297, 1356 to Step 11. (4) **2 missing call sites** — added `src/connector/mock/tests.rs` L558, L632 to Step 11. (5) **Corrected drifted line refs** — fixed 10 phantom line numbers in worker/tests.rs; total migration count updated from 76 to 80. Plan collapsed from 16 to 12 steps (4 TDD cycles, 4 checkpoints). |

### Negative Scope
**Out of Scope for Sub-Block H1:**
- Do NOT prune unread COM interface fields from `ComGroup` or `ComServer` (Reserved for Sub-Block H2 — Finding #4).
- Do NOT modify `apply_proxy_blanket` DCOM proxy blanketing or delete uncalled `connect_server_identifier` (Reserved for Sub-Block H2 — Findings #1, #10).
- Do NOT implement case-insensitive group cache matching `eq_ignore_ascii_case` in `find_active_group_idx` (Reserved for Sub-Block H3 — Finding #2).
- Do NOT implement lowercase canonicalization in `normalize_host_str` or case-insensitive server validation (Reserved for Sub-Block H3 — Finding #8).
- Do NOT modify `IntoTags` lifetimes or implement `&'a [&'a str]` conversions (Reserved for Sub-Block H3 — Finding #7).
- Do NOT edit any source code files in the `opc-cli` crate (verified 0 callers of modified types).

### Interface Contracts

#### 1. `ServerIdentifier` Hardened Discrimination & Conversions (`src/types/server.rs`)
```rust
impl FromStr for ServerIdentifier {
    type Err = ParseServerIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(ParseServerIdError::Empty);
        }
        if trimmed.contains('\0') {
            return Err(ParseServerIdError::InvalidProgId(trimmed.to_string()));
        }
        if let Some(clsid) = Clsid::parse(trimmed) {
            Ok(Self::Clsid(clsid))
        } else if trimmed.starts_with('{') {
            let clsid = Clsid::from_str(trimmed).map_err(ParseServerIdError::InvalidClsid)?;
            Ok(Self::Clsid(clsid))
        } else {
            validate_prog_id(trimmed)?;
            Ok(Self::ProgId(trimmed.to_string()))
        }
    }
}

impl TryFrom<&str> for ServerIdentifier {
    type Error = ParseServerIdError;
    #[inline]
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<String> for ServerIdentifier {
    type Error = ParseServerIdError;
    #[inline]
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.as_str().parse()
    }
}
```

#### 2. `OpcServerEndpoint` Fallible Conversions & Helpers (`src/types/server.rs`)
```rust
impl OpcServerEndpoint {
    /// Parses an endpoint string, symmetrically with [`ServerIdentifier::new`].
    pub fn new(s: &str) -> Result<Self, ParseEndpointError> {
        s.parse()
    }

    /// Creates a local endpoint for a strongly-typed identifier (`ServerIdentifier`, `Clsid`, `GUID`).
    #[must_use]
    pub fn local(identifier: impl Into<ServerIdentifier>) -> Self {
        Self {
            host: None,
            identifier: identifier.into(),
        }
    }

    /// Convenience constructor creating a local endpoint from a raw ProgID string without parsing.
    #[must_use]
    pub fn local_prog_id(prog_id: impl Into<String>) -> Self {
        Self {
            host: None,
            identifier: ServerIdentifier::ProgId(prog_id.into()),
        }
    }

    /// Creates a remote endpoint for a strongly-typed identifier.
    #[must_use]
    pub fn remote(host: impl Into<String>, identifier: impl Into<ServerIdentifier>) -> Self {
        let host_str = host.into();
        let host_opt = normalize_host(Some(&host_str));
        Self {
            host: host_opt,
            identifier: identifier.into(),
        }
    }

    /// Convenience constructor creating a remote endpoint from raw host and ProgID strings.
    #[must_use]
    pub fn remote_prog_id(host: impl Into<String>, prog_id: impl Into<String>) -> Self {
        let host_str = host.into();
        let host_opt = normalize_host(Some(&host_str));
        Self {
            host: host_opt,
            identifier: ServerIdentifier::ProgId(prog_id.into()),
        }
    }
}

impl TryFrom<&str> for OpcServerEndpoint {
    type Error = ParseEndpointError;
    #[inline]
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl TryFrom<String> for OpcServerEndpoint {
    type Error = ParseEndpointError;
    #[inline]
    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.as_str().parse()
    }
}
```

#### 3. `LocalPointer` CWE-626 Hardening (`src/raw/memory.rs`)
```rust
impl LocalPointer<Vec<u16>> {
    /// Constructs a `LocalPointer` containing a null-terminated UTF-16 wide string.
    ///
    /// # Errors
    /// Returns [`OpcError::InvalidState`] if `s` contains an interior null byte (`\0`),
    /// preventing Win32 string truncation vulnerabilities (CWE-626).
    pub fn try_from_str(s: &str) -> OpcResult<Self> {
        if s.contains('\0') {
            return Err(OpcError::InvalidState(
                "String contains interior null byte (CWE-626)".to_string(),
            ));
        }
        let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        Ok(Self::new(Some(wide)))
    }
}

impl TryFrom<&str> for LocalPointer<Vec<u16>> {
    type Error = OpcError;
    #[inline]
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from_str(s)
    }
}

impl TryFrom<String> for LocalPointer<Vec<u16>> {
    type Error = OpcError;
    #[inline]
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::try_from_str(s.as_str())
    }
}
```

#### 4. `OpcError` Infallible Conversion Bridge (`src/errors.rs`)
```rust
impl From<std::convert::Infallible> for OpcError {
    #[inline]
    fn from(err: std::convert::Infallible) -> Self {
        match err {}
    }
}
```

#### 5. Client Fail-Early Constructors with Anchored Type Inference (`src/client/mod.rs`)
```rust
impl OpcDaClient<DefaultBackendConnector, Unbound> {
    /// Constructs a client bound to an OPC DA server endpoint.
    ///
    /// Accepts any type convertible to [`OpcServerEndpoint`], including local ProgIDs,
    /// direct CLSIDs, UNC paths (`\\host\server`), or URI strings (`opc://host/server`).
    ///
    /// Validates the target endpoint before initializing the background worker thread.
    pub fn bind_new(
        server: impl TryInto<OpcServerEndpoint, Error: Into<OpcError>>,
    ) -> OpcResult<OpcDaClient<DefaultBackendConnector, Bound>> {
        let endpoint: OpcServerEndpoint = server.try_into().map_err(Into::into)?;
        let client = Self::new(DefaultBackendConnector::default())?;
        Ok(client.bind(endpoint))
    }

    /// Constructs a client bound to a remote OPC DA server by host and identifier.
    ///
    /// Validates the server identifier and normalizes the host before worker thread startup.
    pub fn bind_new_remote(
        host: impl AsRef<str>,
        server: impl TryInto<ServerIdentifier, Error: Into<OpcError>>,
    ) -> OpcResult<OpcDaClient<DefaultBackendConnector, Bound>> {
        let host_ref = host.as_ref();
        if host_ref.contains('\0') {
            return Err(OpcError::InvalidState("Host contains interior null byte".into()));
        }
        let server_id: ServerIdentifier = server.try_into().map_err(Into::into)?;
        let endpoint = OpcServerEndpoint::remote(host_ref, server_id);
        let client = Self::new(DefaultBackendConnector::default())?;
        Ok(client.bind(endpoint))
    }
}
```

#### 6. Builder Non-Clobbering Error Accumulation & Anchored `new()` (`src/client/builder.rs`)
```rust
pub struct OpcDaClientBuilder<C = DefaultBackendConnector> {
    pub(crate) host: Option<String>,
    pub(crate) server: Option<ServerIdentifier>,
    pub(crate) server_err: Option<OpcError>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) legacy_dcom: bool,
    pub(crate) connector: Option<C>,
}

impl OpcDaClientBuilder<DefaultBackendConnector> {
    /// Creates a new default client builder targeting the default backend connector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            host: None,
            server: None,
            server_err: None,
            timeout: None,
            legacy_dcom: false,
            connector: Some(DefaultBackendConnector::default()),
        }
    }
}

impl<C: ServerBackend + 'static> OpcDaClientBuilder<C> {
    #[must_use]
    pub fn new_with_connector(connector: C) -> Self {
        Self {
            host: None,
            server: None,
            server_err: None,
            timeout: None,
            legacy_dcom: false,
            connector: Some(connector),
        }
    }

    #[must_use]
    pub fn with_connector<C2: ServerBackend + 'static>(self, connector: C2) -> OpcDaClientBuilder<C2> {
        OpcDaClientBuilder {
            host: self.host,
            server: self.server,
            server_err: self.server_err,
            timeout: self.timeout,
            legacy_dcom: self.legacy_dcom,
            connector: Some(connector),
        }
    }

    #[must_use]
    pub fn server(mut self, server: impl TryInto<ServerIdentifier, Error: Into<OpcError>>) -> Self {
        match server.try_into() {
            Ok(s) => self.server = Some(s),
            Err(err) => {
                self.server = None;
                self.server_err.get_or_insert_with(|| err.into());
            }
        }
        self
    }

    #[must_use]
    pub fn host(mut self, host: impl Into<String>) -> Self {
        let h = host.into();
        if h.contains('\0') {
            self.server_err.get_or_insert_with(|| {
                OpcError::InvalidState("Host contains interior null byte".into())
            });
            self.host = None;
        } else {
            self.host = Some(h);
        }
        self
    }

    fn build_internal(
        connector: C,
        host: Option<&str>,
        server: Option<ServerIdentifier>,
        server_err: Option<OpcError>,
        timeout: Option<Duration>,
    ) -> OpcResult<OpcDaClient<C, Unbound>> {
        if let Some(err) = server_err {
            return Err(err);
        }
        let mut client = OpcDaClient::new(connector)?;
        client.timeout = timeout;
        if let Some(identifier) = server {
            let endpoint = match host {
                Some(h) => OpcServerEndpoint::remote(h, identifier),
                None => OpcServerEndpoint::local(identifier),
            };
            client.endpoint = Some(endpoint);
        }
        Ok(client)
    }

    pub fn build_with_connector(self, connector: C) -> OpcResult<OpcDaClient<C, Unbound>> {
        Self::build_internal(connector, self.host.as_deref(), self.server, self.server_err, self.timeout)
    }
}

impl<C: ServerBackend + Default + 'static> OpcDaClientBuilder<C> {
    pub fn build(self) -> OpcResult<OpcDaClient<C, Unbound>> {
        let connector = self.connector.unwrap_or_default();
        Self::build_internal(connector, self.host.as_deref(), self.server, self.server_err, self.timeout)
    }

    pub fn build_bound(self) -> OpcResult<OpcDaClient<C, Bound>> {
        if let Some(err) = self.server_err.as_ref() {
            return Err(err.clone());
        }
        if self.server.is_none() {
            return Err(OpcError::InvalidState(
                "Cannot build bound client without configuring server identifier".into(),
            ));
        }
        let unbound = self.build()?;
        let ep = unbound.endpoint.clone().ok_or_else(|| {
            OpcError::InvalidState("Cannot build bound client without configuring server identifier".into())
        })?;
        Ok(unbound.bind(ep))
    }
}
```

### Security Constraints
- **Trust Boundaries:** External string inputs (`&str`, `String`) crossing public constructor and builder boundaries must be validated for character set constraints (`[a-zA-Z0-9._-]`), length limits (1..=255), and absence of null bytes (`\0`).
- **CWE-626 (Memory/String Truncation):** No string containing an interior `\0` byte may ever be passed to Win32 COM APIs (`CoCreateInstanceEx` via `COSERVERINFO.pwszName` or `CLSIDFromProgID`). `LocalPointer::try_from_str` enforces this at the COM memory boundary. Host strings containing `\0` must return `Err` or be recorded in `server_err`, and never be silently converted to `None` (localhost).
- **CWE-20 (Improper Input Validation):** Infallible `From<&str>` and `From<String>` must not exist on domain entities where parsing can fail. All conversions must be fallible via `TryFrom` or `FromStr`.
- **Fail-Early Resource Protection:** No OS thread may be spawned and no COM MTA apartment may be initialized before verifying target endpoint syntax.

### Blast Radius Table
| Symbol | File | Direct Callers | Indirect Deps | Test-Only | Cross-Package? |
|--------|------|---------------|---------------|-----------|----------------|
| `<ServerIdentifier as FromStr>::from_str` | `src/types/server.rs` | 6 | 15 | No | No |
| `<ServerIdentifier as From<&str>>::from` (Deleted) | `src/types/server.rs` | 10 | 51 | Yes | No |
| `<ServerIdentifier as From<String>>::from` (Deleted) | `src/types/server.rs` | 0 | 0 | Yes | No |
| `<ServerIdentifier as TryFrom<&str>>::try_from` (New) | `src/types/server.rs` | 0 | 24 | No | No |
| `<ServerIdentifier as TryFrom<String>>::try_from` (New) | `src/types/server.rs` | 0 | 5 | No | No |
| `<OpcServerEndpoint as From<&str>>::from` (Deleted) | `src/types/server.rs` | 1 | 0 | Yes | No |
| `<OpcServerEndpoint as From<String>>::from` (Deleted) | `src/types/server.rs` | 0 | 0 | Yes | No |
| `<OpcServerEndpoint as TryFrom<&str>>::try_from` (New) | `src/types/server.rs` | 0 | 8 | No | No |
| `OpcServerEndpoint::new` (Symmetric helper) | `src/types/server.rs` | 0 | 8 | No | No |
| `OpcServerEndpoint::local` & `local_prog_id` | `src/types/server.rs` | 80 (Tests) | 0 | Yes | No |
| `OpcServerEndpoint::remote` & `remote_prog_id` | `src/types/server.rs` | 8 (Tests & Docs) | 0 | Yes | No |
| `LocalPointer::try_from_str` (New) | `src/raw/memory.rs` | 6 (Active) | 0 | No | No |
| `<LocalPointer<Vec<u16>> as From<S>>::from` (Deleted) | `src/raw/memory.rs` | 6 (Migrated) | 0 | No | No |
| `ComConnector` LocalPointer calls | `src/com/connector/server.rs` | 5 | 0 | No | No |
| `create_remote_instance` | `src/com/security.rs` | 1 | 0 | No | No |
| `OpcDaClient::bind_new` | `src/client/mod.rs` | 8 | 0 | No | No |
| `OpcDaClient::bind_new_remote` | `src/client/mod.rs` | 2 | 0 | No | No |
| `OpcDaClientBuilder::server` | `src/client/builder.rs` | 24 | 3 | Yes | No |
| `OpcDaClientBuilder::build_bound` | `src/client/builder.rs` | 16 | 0 | Yes | No |
| `README.md` doctest | `opc-da-client/README.md` | 1 (Doctest) | 0 | Yes | No |
| `tests/typestate_client_test.rs` | `tests/typestate_client_test.rs` | 4 (Integration) | 0 | Yes | No |

### Edge Cases & Risks
1. **Zero Downstream Compilation Breaks in `opc-cli`:**
   Workspace analysis confirms that `opc-cli` only calls `OpcProvider` trait methods with string slices and does not invoke `ServerIdentifier::from` or `OpcServerEndpoint::from`. Deleting infallible `From` produces 0 compilation breaks in `opc-cli`.
2. **Generic `TryInto` Type Inference (`bind_new` & `builder.server`):**
   `bind_new` is anchored on `impl OpcDaClient<DefaultBackendConnector, Unbound>`, eliminating `E0282` inference errors. `OpcDaClientBuilder::new()` is anchored on `DefaultBackendConnector`, preserving seamless instantiation. Introducing `impl From<Infallible> for OpcError` ensures that passing pre-constructed types satisfies `Error: Into<OpcError>` with zero conversion cost.
3. **Non-Clobbering Builder Error Accumulation:**
   Using `self.server_err.get_or_insert_with(...)` ensures that calling `.host("bad\0host").server("Valid.Server")` retains the host security violation, preventing target host confusion.
4. **Unconstrained Connector Extension:**
   Keeping `build_internal` and `build_with_connector` under `impl<C: ServerBackend + 'static>` preserves the ability to use custom mock or configured connectors that do not implement `Default`.
5. **Comprehensive Test Suite & Doctest Migration:**
   All 80 test call sites across `worker/browse.rs`, `worker/write.rs`, `worker/pool.rs`, `worker/read.rs`, `worker/tests.rs` (39 sites), `client/tests.rs`, `mock/tests.rs` (including L558, L632), `server_discovery_integration_test.rs`, `tag_browsing_integration_test.rs`, `typestate_client_test.rs`, `batch_write_test.rs`, `server.rs:676, 686, 810, 814, 818, 864, 870, 1018`, and `README.md:257` are systematically migrated.
6. **28 `.server("...")` Builder Calls Preserved by Signature Widening:**
   Widening `OpcDaClientBuilder::server` from `impl Into<ServerIdentifier>` to `impl TryInto<ServerIdentifier, Error: Into<OpcError>>` ensures all 28 existing `.server("string_literal")` calls across `src/lib.rs`, `src/client/tests.rs`, `tests/batch_write_test.rs`, `tests/resilience_and_pool_integration_test.rs`, `tests/subscription_integration_test.rs`, `tests/tag_io_integration_test.rs`, `tests/tag_browsing_integration_test.rs`, `tests/server_discovery_integration_test.rs`, and `tests/typestate_client_test.rs` continue to compile without individual migration via the blanket `TryInto` → `TryFrom<&str>` path.

### Test Plan (TDD)
1. **Hyphenated ProgID Discrimination Test (`src/types/server.rs`)**:
   Verify `"KEPServerEX-V6.1"`, `"Schneider-OpcServer.1"`, and `"ABB.IndustrialIT-Server.1"` parse as `ServerIdentifier::ProgId`. Verify bracketed and unbracketed standard CLSIDs parse as `ServerIdentifier::Clsid`. Verify malformed bracketed GUIDs return `ParseServerIdError::InvalidClsid`.
2. **CWE-626 Null-Byte Rejection Tests (`src/types/server.rs` & `src/raw/memory.rs`)**:
   Verify `ServerIdentifier::new("Server\0Name")` returns `Err(ParseServerIdError::InvalidProgId)`. Verify `r"\\host\0name\Server".parse::<OpcServerEndpoint>()` returns `Err(ParseEndpointError::InvalidFormat)`. Verify `LocalPointer::try_from_str("poison\0host")` returns `Err(OpcError::InvalidState)`.
3. **Fallible Conversion Tests (`src/types/server.rs`)**:
   Verify `ServerIdentifier::try_from` and `OpcServerEndpoint::try_from` for valid strings and rejection of empty/malformed inputs.
4. **Client Ingress & Fail-Early Tests (`src/client/tests.rs`)**:
   Verify `OpcDaClient::bind_new` accepts UNC remote, URI remote, and local ProgID strings without turbofish. Verify invalid endpoints fail immediately without initializing COM MTA worker threads.
5. **Builder Non-Clobbering Deferred Error Accumulation Test (`src/client/tests.rs`)**:
   Verify chaining `.host("bad\0host").server("Valid.Server")` preserves the host error on `build()`, `build_bound()`, and `build_with_connector()`.

### Global Execution Order

Step 1: [TEST] `opc-da-client/src/types/server.rs` — [+] `test_hyphenated_prog_ids_parse_as_prog_id` (Finding #3)
- Pre: ALL
- Target: `src/types/server.rs:650+`
- Action:
  Add test verifying hyphenated ProgIDs (`KEPServerEX-V6.1`, `Schneider-OpcServer.1`, `ABB.IndustrialIT-Server.1`) parse as `ProgId` and standard GUIDs parse as `Clsid`.
- Post: RED(`test_hyphenated_prog_ids_parse_as_prog_id`)

Step 2: [MODIFY] `opc-da-client/src/types/server.rs` — [~] `ServerIdentifier::from_str` hyphen heuristic fix (Finding #3)
- Pre: RED(`test_hyphenated_prog_ids_parse_as_prog_id`)
- Target: `src/types/server.rs:225-245`
- Action:
  Replace `trimmed.starts_with('{') || trimmed.contains('-')` with:
  ```rust
  if let Some(clsid) = Clsid::parse(trimmed) {
      Ok(Self::Clsid(clsid))
  } else if trimmed.starts_with('{') {
      let clsid = Clsid::from_str(trimmed).map_err(ParseServerIdError::InvalidClsid)?;
      Ok(Self::Clsid(clsid))
  } else {
      validate_prog_id(trimmed)?;
      Ok(Self::ProgId(trimmed.to_string()))
  }
  ```
- Post: GREEN(`test_hyphenated_prog_ids_parse_as_prog_id`)

Step 3: [TEST] `opc-da-client/src/types/server.rs` — [+] `test_server_identifier_null_byte_rejection` & `test_opc_server_endpoint_parse_null_byte_rejection` (Findings #3, #5, #6)
- Pre: GREEN(`test_hyphenated_prog_ids_parse_as_prog_id`)
- Target: `src/types/server.rs:680+`
- Action:
  Add tests asserting `ServerIdentifier::new` rejects interior null bytes with `Err(ParseServerIdError::InvalidProgId)`, and `"\\\\host\\0name\\Server".parse::<OpcServerEndpoint>()` (using `FromStr`, which exists) returns `Err(ParseEndpointError::InvalidFormat)`. **Note:** Do NOT call `OpcServerEndpoint::new()` here — it is not defined until Step 9.
- Post: RED(`null_byte_rejection_tests`)

Step 4: [MODIFY] `opc-da-client/src/types/server.rs` — [~] `ServerIdentifier::from_str` & `OpcServerEndpoint::from_str` null-byte validation (Findings #3, #5, #6)
- Pre: RED(`null_byte_rejection_tests`)
- Target: `src/types/server.rs:225-245`, `src/types/server.rs:590-630`
- Action:
  Enforce `trimmed.contains('\0')` rejection in `ServerIdentifier::from_str` and `OpcServerEndpoint::from_str`. In `OpcServerEndpoint::from_str`, ensure host containing `\0` returns `Err(ParseEndpointError::InvalidFormat)`.
- Post: GREEN(`null_byte_rejection_tests`) 🔒 CHECKPOINT

Step 5: [TEST] `opc-da-client/src/raw/memory.rs` — [+] `test_local_pointer_try_from_str_null_byte_rejection` (Finding #5)
- Pre: ALL 🔒 (Step 4)
- Target: `src/raw/memory.rs:885+`
- Action:
  Add test verifying `LocalPointer::try_from_str("poison\0host")` returns `Err(OpcError::InvalidState)` and clean strings succeed.
- Post: RED(`test_local_pointer_try_from_str_null_byte_rejection`)

Step 6: [MODIFY] `opc-da-client/src/raw/memory.rs`, `src/com/connector/server.rs`, `src/com/security.rs` — [+] `LocalPointer::try_from_str`, [~] `TryFrom<&str>` / `TryFrom<String>`, [-] `From<S>`, migrate 6 call sites (Finding #5)
- Pre: RED(`test_local_pointer_try_from_str_null_byte_rejection`)
- Target: `src/raw/memory.rs:620-645`, `src/raw/memory.rs:1038`, `src/com/connector/server.rs:35,312,335,353,367`, `src/com/security.rs:135`
- Action:
  1. In `src/raw/memory.rs`: Add `LocalPointer<Vec<u16>>::try_from_str(s: &str) -> OpcResult<Self>`.
  2. Implement `TryFrom<&str>` and `TryFrom<String>` for `LocalPointer<Vec<u16>>`.
  3. Delete unchecked `impl<S: AsRef<str>> From<S> for LocalPointer<Vec<u16>>`.
  4. In `src/raw/memory.rs:1038`, update test call to `LocalPointer::try_from_str("Matrikon.OPC.Simulation.1").unwrap()`.
  5. In `src/com/connector/server.rs`, update lines 35, 312, 335, 353, 367 to `LocalPointer::try_from_str(...)?`.
  6. In `src/com/security.rs:135`, update `LocalPointer::from(host)` to `LocalPointer::try_from_str(host)?`.
- Post: GREEN(`test_local_pointer_try_from_str_null_byte_rejection`) 🔒 CHECKPOINT

Step 7: [TEST] `opc-da-client/src/types/server.rs`, `src/client/tests.rs` — [+] All remaining RED behavioral tests (Findings #3, #6, #9)
- Pre: ALL 🔒 (Step 6)
- Target: `src/types/server.rs:720+`, `src/client/tests.rs:350+`
- Action:
  Write ALL remaining behavioral RED tests in one step. Each test compiles against the current API but asserts behavior that does not yet exist:
  1. **`test_server_identifier_try_from_str_and_string`** (`src/types/server.rs`): Assert `ServerIdentifier::try_from("valid")` returns `Ok(ProgId)`, `ServerIdentifier::try_from("")` returns `Err(Empty)`, and `ServerIdentifier::try_from("bad\0")` returns `Err`. Currently passes infallibly via blanket `TryFrom` from `From<&str>` → will fail on error-path assertions.
  2. **`test_opc_server_endpoint_try_from_str_and_string`** (`src/types/server.rs`): Assert `OpcServerEndpoint::try_from("valid")` returns `Ok(local)`, `OpcServerEndpoint::try_from(r"\\host\server")` returns `Ok(remote)`, and `OpcServerEndpoint::try_from("")` returns `Err`. Currently passes infallibly → will fail on error-path assertions.
  3. **`test_client_bind_new_endpoint_schemes`** (`src/client/tests.rs`): Assert `OpcDaClient::bind_new(OpcServerEndpoint::local_prog_id("Server"))` succeeds. Compiles against current `bind_new(impl Into<ServerIdentifier>)` with a pre-constructed endpoint. **Note:** This test will need updating after Step 9 widens `bind_new` to `impl TryInto<OpcServerEndpoint>` — the Builder should adapt assertions accordingly.
  4. **`test_client_bind_new_fail_early_on_invalid_endpoint`** (`src/client/tests.rs`): Assert that passing an invalid endpoint to `bind_new` returns `Err` without initializing COM MTA worker threads. Currently `bind_new` does not validate before `Self::new(...)` → will fail.
  5. **`test_builder_server_deferred_error_accumulation`** (`src/client/tests.rs`): Assert `.host("bad\0host").server("Valid.Server").build()` returns `Err` retaining the host violation. Currently the builder ignores null bytes → will fail.
- Post: RED(`remaining_behavioral_tests`)

Step 8: [MODIFY] `opc-da-client/src/errors.rs` — [+] `From<Infallible> for OpcError` (Findings #6, #9)
- Pre: RED(`remaining_behavioral_tests`)
- Target: `src/errors.rs:92-105`
- Action:
  Implement `From<std::convert::Infallible> for OpcError` using `match err {}`. This is a prerequisite for `TryInto<X, Error: Into<OpcError>>` signatures to accept pre-constructed types via the blanket `TryInto` → `Into` → `TryFrom` with `Error = Infallible` path. Retain existing `From<ParseServerIdError>` and `From<ParseEndpointError>` in `src/types/server.rs:104-120`.
- Post: CHECK

Step 9: [MODIFY] ATOMIC — `opc-da-client/src/types/server.rs`, `src/client/mod.rs`, `src/client/builder.rs` — [-] `From<&str>` / `From<String>`, [+] `TryFrom`, [+] `new` / `local_prog_id` / `remote_prog_id`, [~] `bind_new` / `builder.server` signature widening (Findings #3, #6, #9)
- Pre: CHECK
- Target: `src/types/server.rs:270-287, 449-526, 565-632, 634-644`, `src/client/mod.rs:77-131`, `src/client/builder.rs:14-20, 24-30, 78-86, 96-100, 111-122, 136-246`
- Action:
  **This step MUST be atomic** — all 3 files are modified before any compilation check. Deleting `From<&str>` without simultaneously widening `builder.server()` and `bind_new()` to `TryInto` would break 28 `.server("...")` test calls across the workspace.

  **A. `src/types/server.rs` (Clean Slate Conversions + Helpers):**
  1. Delete `impl From<&str> for ServerIdentifier` (L270-281) and `impl From<String> for ServerIdentifier` (L282-287).
  2. Implement `TryFrom<&str>` and `TryFrom<String>` for `ServerIdentifier` (delegates to `s.parse()`).
  3. Delete `impl From<&str> for OpcServerEndpoint` (L634-638) and `impl From<String> for OpcServerEndpoint` (L640-644).
  4. Implement `TryFrom<&str>` and `TryFrom<String>` for `OpcServerEndpoint` (delegates to `s.parse()`).
  5. Add `pub fn new(s: &str) -> Result<Self, ParseEndpointError> { s.parse() }` to `OpcServerEndpoint`.
  6. Add `OpcServerEndpoint::local_prog_id(prog_id: impl Into<String>) -> Self`.
  7. Add `OpcServerEndpoint::remote_prog_id(host: impl Into<String>, prog_id: impl Into<String>) -> Self`.
  8. Retain `pub fn local(identifier: impl Into<ServerIdentifier>) -> Self` and `pub fn remote(host: impl Into<String>, identifier: impl Into<ServerIdentifier>) -> Self`.

  **B. `src/client/mod.rs` (Anchored Fail-Early Constructors):**
  1. Anchor `bind_new` and `bind_new_remote` on `impl OpcDaClient<DefaultBackendConnector, Unbound>`.
  2. `bind_new` accepts `impl TryInto<OpcServerEndpoint, Error: Into<OpcError>>`. Validate via `server.try_into().map_err(Into::into)?` *before* calling `Self::new(DefaultBackendConnector::default())?`.
  3. `bind_new_remote` accepts `impl TryInto<ServerIdentifier, Error: Into<OpcError>>`. Reject `host.contains('\0')` with `OpcError::InvalidState` *before* calling `Self::new(...)`.

  **C. `src/client/builder.rs` (Non-Clobbering Deferred Error Accumulation):**
  1. Add `server_err: Option<OpcError>` to `OpcDaClientBuilder` struct (L14-20).
  2. Keep `new()` anchored on `impl OpcDaClientBuilder<DefaultBackendConnector>`, initializing `server_err: None`.
  3. In `new_with_connector()`, initialize `server_err: None`.
  4. In `with_connector()`, preserve `server_err: self.server_err`.
  5. Widen `server()` to `impl TryInto<ServerIdentifier, Error: Into<OpcError>>`. Use `self.server_err.get_or_insert_with(...)` on error, never resetting `server_err` on `Ok`.
  6. In `host()`, reject `\0` via `self.server_err.get_or_insert_with(...)`.
  7. Keep `build_internal` and `build_with_connector` under `impl<C: ServerBackend + 'static>`. Check `if let Some(err) = server_err { return Err(err); }` before spawning worker thread.
  8. In `build_bound()`, borrow `server_err` via `.as_ref()`, check `server.is_none()`, call `self.build()?`, and bind endpoint.
- Post: GREEN(`remaining_behavioral_tests`) 🔒 CHECKPOINT

Step 10: [MODIFY] `opc-da-client/README.md`, `src/types/server.rs` doctests, `src/connector/mock/tests.rs` — [~] Update doctests and mock endpoint conversions (Findings #3, #6)
- Pre: GREEN(`remaining_behavioral_tests`) 🔒 (Step 9)
- Target: `opc-da-client/README.md:257`, `src/types/server.rs:461, 487, 519, 520`, `src/connector/mock/tests.rs:326, 458`
- Action:
  1. In `README.md:257`, update `ServerIdentifier::from("{28E68F9A-8D75-11D1-8DC3-3C302A000000}")` to `"{28E68F9A-8D75-11D1-8DC3-3C302A000000}".parse::<ServerIdentifier>()?`.
  2. In `src/types/server.rs:461` doctest, update `OpcServerEndpoint::local("Matrikon...")` to `OpcServerEndpoint::local_prog_id("Matrikon...")`.
  3. In `src/types/server.rs:487` doctest, update `OpcServerEndpoint::remote("192.168.1.50", "Matrikon...")` to `OpcServerEndpoint::remote_prog_id("192.168.1.50", "Matrikon...")`.
  4. In `src/types/server.rs:519-520` doctests, update `OpcServerEndpoint::local("Server")` to `OpcServerEndpoint::local_prog_id("Server")` and `OpcServerEndpoint::remote("10.0.0.1", "Server")` to `OpcServerEndpoint::remote_prog_id("10.0.0.1", "Server")`.
  5. In `src/connector/mock/tests.rs:326`, update `OpcServerEndpoint::remote("remote-host", "Offline.Server.1")` to `OpcServerEndpoint::remote_prog_id("remote-host", "Offline.Server.1")`.
  6. In `src/connector/mock/tests.rs:458`, update `OpcServerEndpoint::from("Offline.Server.1")` to `"Offline.Server.1".parse::<crate::types::OpcServerEndpoint>().unwrap()`.
- Post: CHECK

Step 11: [MODIFY] `opc-da-client/src/com/worker/`, `src/client/`, `src/connector/mock/tests.rs`, `tests/`, `server.rs` unit tests — [~] Migrate all 80 test call sites to `local_prog_id`, `remote_prog_id`, or `try_from` (Findings #3, #6)
- Pre: CHECK
- Target:
  - `src/com/worker/pool.rs` (L424, 455, 498, 531, 661, 662, 728, 770, 820, 834, 937, 1013, 1066, 1076, 1084, 1100)
  - `src/com/worker/read.rs` (L298, 308, 325, 350, 368)
  - `src/com/worker/browse.rs` (L275, 289, 301)
  - `src/com/worker/write.rs` (L158, 217)
  - `src/com/worker/tests.rs` (L82, 109, 133, 161, 171, 197, 213, 237, 255, 284, 316, 340, 453, 553, 576, 597, 620, 670, 717, 744, 749, 775, 780, 821, 891, 957, 995, 1024, 1028, 1083, 1123, 1163, 1199, 1220, 1259, 1268, 1289, 1297, 1356)
  - `src/connector/mock/tests.rs` (L558, 632)
  - `src/client/tests.rs` (L39, 101, 113, 124, 141, 151, 160, 169, 178, 187, 199, 238, 245)
  - `src/types/server.rs` (L676, 686, 810, 814, 818, 864, 870, 1018)
  - `tests/server_discovery_integration_test.rs` (L82)
  - `tests/tag_browsing_integration_test.rs` (L218, 244)
  - `tests/typestate_client_test.rs` (L13, 42, 55, 102)
  - `tests/batch_write_test.rs` (L50)
- Action:
  1. Migrate string literals in `OpcServerEndpoint::local("...")` to `OpcServerEndpoint::local_prog_id("...")`.
  2. Migrate string literals in `OpcServerEndpoint::remote(host, "...")` to `OpcServerEndpoint::remote_prog_id(host, "...")`.
  3. Migrate remaining `ServerIdentifier::from("...")` to `ServerIdentifier::try_from("...").unwrap()`.
  4. In `src/types/server.rs:870`, migrate `.into()` to `.parse::<OpcServerEndpoint>().unwrap()`.
  **Note:** The 28 `.server("...")` builder calls across integration tests do NOT need migration — they compile transparently via the `TryInto` → `TryFrom<&str>` blanket path.
- Post: GREEN(test_suite) 🔒 CHECKPOINT

Step 12: [TEST] Workspace verification — `pwsh -File scripts/verify.ps1` 🔒
- Pre: ALL
- Target: Workspace root
- Action: Execute full 9-gate quality pipeline to guarantee zero warnings under `-D warnings`.
- Post: VERIFIED 🔒

### Verification Plan
| Type | Command | Expected Result |
|------|---------|-----------------|
| Domain Unit Suite | `cargo test -p opc-da-client --lib types::server` | All server identifier and endpoint unit tests pass |
| Memory Unit Suite | `cargo test -p opc-da-client --lib raw::memory` | All `LocalPointer` CWE-626 unit tests pass |
| Connector Unit Suite | `cargo test -p opc-da-client --lib com::connector::server` | All COM connector server unit tests pass |
| Client Facade Suite | `cargo test -p opc-da-client --lib client::tests` | All facade tests pass including deferred builder and fail-early |
| Worker Unit Suite | `cargo test -p opc-da-client --lib com::worker` | All worker tests pass with migrated `local_prog_id` / `remote_prog_id` |
| Doc-Tests Suite | `cargo test -p opc-da-client --doc --all-features` | All doctests pass including README.md:257 and server.rs doctests |
| Integration Test Suites | `cargo test -p opc-da-client --test server_discovery_integration_test --test typestate_client_test --test tag_browsing_integration_test --test batch_write_test` | Integration suites pass with migrated call sites |
| Workspace Check | `cargo check --workspace --all-targets --all-features` | Zero errors across `opc-da-client` and `opc-cli` |
| Full 9-Gate Pipeline | `pwsh -File scripts/verify.ps1` | All 9 gates pass with exit code 0 |

### Plan Summary
| Metric | Value |
|---|---|
| Tier | M (Feature / Refactor) |
| Files Modified | 12 (`src/types/server.rs`, `src/raw/memory.rs`, `src/com/connector/server.rs`, `src/com/security.rs`, `src/errors.rs`, `src/client/mod.rs`, `src/client/builder.rs`, `src/client/tests.rs`, `src/connector/mock/tests.rs`, `src/com/worker/`, `tests/`, `README.md`) |
| Steps | 12 steps (4 Red-Green TDD cycles) |
| Checkpoints | 4 (Step 4, Step 6, Step 9, Step 11) |
| Verification | Step 12 (`pwsh -File scripts/verify.ps1`) |
| Test Call Sites Migrated | 80 (`local_prog_id`/`remote_prog_id`/`try_from`) + 28 preserved via signature widening |
| Estimated Effort | Medium |

## ⚠️ Reviewer Findings (Unresolved)
None. All findings from Review Cycles 1–4 have been fully resolved and incorporated into the plan text. Cycle 4 resolved a critical topological sequencing inversion, a Step 3 pre-declaration API issue, 14 missing `worker/tests.rs` call sites, and 2 missing `mock/tests.rs` call sites.

