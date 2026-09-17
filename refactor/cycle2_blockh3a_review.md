# Cycle 2 Qualitative Architecture & Code Quality Review: Sub-Block H3a

> **Document Status:** Active Engineering Review Report & Planning Foundation  
> **Workspace:** `opc-cli`  
> **Target Crate:** `opc-da-client` (v0.2.0 $\rightarrow$ v0.3.0 Release Candidate)  
> **Evaluation Date:** 2026-09-17  
> **Sub-Block Focus:** Sub-Block H3a: Server Identity & Host Canonicalization  
> **Reference Documents:** [`refactor/cycle2_blockH3_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockH3_review.md), [`refactor/cycle2_blockH_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockH_review.md)  
> **Review Scope:** Server identity domain models, hostname normalization, client validation facades, and connection pool deduplication (`src/types/server.rs`, `src/client/gateway.rs`, `src/com/worker/pool.rs`)  
> **Review Pipeline:** Decomposed Multi-Lens Audit across Logic, Design, Performance, Security, and API lenses  
> **User Interview Alignment & Consensuses:**
> 1. **Decomposition Mapping:** Approved Sub-Block H3a isolating Server Identity & Host Canonicalization (Findings #3, #4, #10b).
> 2. **Execution Ordering:** Strictly sequential — Sub-Block H3a precedes H3b and H3c.
> 3. **Host Canonicalization Strategy:** Eager ASCII lowercase normalization in `normalize_host` (`str::to_ascii_lowercase`), keeping `normalize_host_str` strictly for loopback aliases (no null-byte mapping to `None`), and routing `OpcServerEndpoint::from_str` through `normalize_host`.
> 4. **Identifier Matching Strategy:** Preserve standard derived `PartialEq, Eq, Hash` on `ServerIdentifier` and introduce a dedicated `ServerIdentifier::matches(&self, other: &Self) -> bool` method.
> 5. **Gateway Session Receiver Scope:** Retain `impl<C: ServerBackend + 'static, State: Send + Sync + 'static> OpcDaClient<C, State>` on `validate_bound_server` to support both Bound and Unbound states and avoid breaking trait implementations (`TagBrowser`, `TagReader`, `TagWriter`).
> 6. **Rustdoc Relocation:** Reconnect the orphaned doc block at line 460 to `OpcServerEndpoint::local` at line 508 with runnable doc-tests.
> 7. **TDD Matrix:** Dedicated Red-Green test matrix included to guide implementation planning.

---

## 1. Executive Summary

Sub-Block H3a focuses on establishing **canonical server identity and case-insensitive matching** across domain types and client validation layers without compromising Rust's foundational `Eq` and `Hash` invariants.

In industrial Windows COM environments, NetBIOS/DNS hostnames and ProgIDs registered in `HKEY_CLASSES_ROOT` are strictly case-insensitive. Prior to Sub-Block H3a, hostname casing was unnormalized during endpoint parsing, and bound client validation evaluated byte-exact inequality (`!=`). This resulted in two major architectural failure modes:
1. **Connection Pool Key Fragmentation:** Endpoints targeting the same host with varying casing (e.g. `\\SRV1\prog` vs `\\srv1\prog`) created duplicate active DCOM connections in `ConnectionPool`, incurring 200–2000ms connection latencies and exhausting limited server client license seats.
2. **False-Positive Session Rejections:** Bound clients rejected valid requests targeting differing casing (e.g. `"Matrikon.OPC.Simulation.1"` vs `"matrikon.opc.simulation.1"`) with `OpcError::InvalidState`.

Sub-Block H3a resolves these issues by introducing eager ASCII lowercase normalization for hostnames, adding semantic `matches()` methods on `ServerIdentifier` and `OpcServerEndpoint`, and updating `validate_bound_server` and `ConnectionPool`.

---

## 2. Scoped Files & Target Symbols

| File | Target Symbols | Role in Sub-Block H3a |
|:---|:---|:---|
| [`opc-da-client/src/types/server.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs) | `normalize_host`<br>`normalize_host_str`<br>`ServerIdentifier::matches`<br>`OpcServerEndpoint::matches`<br>`<OpcServerEndpoint as FromStr>::from_str`<br>`OpcServerEndpoint::local` | Implement eager lowercase host normalization, add semantic `matches` methods, route parser through `normalize_host`, and document public constructor. |
| [`opc-da-client/src/client/gateway.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/client/gateway.rs) | `OpcDaClient::validate_bound_server` | Update bound session validation to use `.matches()` for both ProgID and hostname comparisons. |
| [`opc-da-client/src/com/worker/pool.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs) | `ConnectionPool::connections`<br>`ConnectionPool` lookup logic | Ensure connection pool deduplicates endpoints sharing semantic identity via canonical lowercase host keying. |

---

## 3. High-Level Objectives & Goals

| ID | Objective | Measurable Success Criteria |
|:---:|:---|:---|
| **O1** | Eager Hostname Lowercase Canonicalization | `normalize_host(Some("SCADA-01"))` returns `Some("scada-01")`. All parsing paths in `OpcServerEndpoint::from_str` delegate through `normalize_host`. |
| **O2** | Semantic `ServerIdentifier::matches` Method | `ServerIdentifier::matches` returns `true` for case-varying ProgIDs (e.g. `"Matrikon.OPC.1"` and `"matrikon.opc.1"`) and exact 128-bit equality for CLSIDs, while preserving derived `PartialEq, Eq, Hash`. |
| **O3** | Semantic `OpcServerEndpoint::matches` Method | `OpcServerEndpoint::matches` returns `true` for endpoints matching both hostname (case-insensitive) and server identifier (`matches`). |
| **O4** | Case-Insensitive Client Session Validation | `OpcDaClient::validate_bound_server` permits mixed-case queries matching the bound server, rejecting only truly distinct hosts or ProgIDs. |
| **O5** | Connection Pool Deduplication & License Protection | `ConnectionPool` stores canonical lowercase host keys, eliminating duplicate COM connection instances for case-varying host queries. |
| **O6** | Public Constructor Documentation & Doctests | `OpcServerEndpoint::local` and `ServerIdentifier::matches` have 100% rustdoc coverage with runnable doc-tests passing `cargo test --doc`. |

---

## 4. Multi-Lens Qualitative Assessment Matrix

| Lens | Severity | Finding Anchor | Summary & Lens-Specific Impact |
|:---|:---:|:---|:---|
| **Logic** | 🟠 Major | [`src/types/server.rs:631`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs#L631)<br>[`src/client/gateway.rs:375`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/client/gateway.rs#L375) | **Host Parser Bypass & False Session Rejection:** `from_str` bypassed `normalize_host` by calling `normalize_host_str().map(str::to_string)`. `validate_bound_server` evaluated `!=` on raw strings, rejecting valid requests on bound sessions. Receiver must remain generic over `State: Send + Sync + 'static` to support Unbound and trait calls. |
| **Design** | 🟠 Major | [`src/types/server.rs:127`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs#L127) | **Rust `Eq`/`Hash` Invariant Protection:** Overriding `PartialEq` to fold casing would violate `k1 == k2 => k1.to_string() == k2.to_string()` and corrupt `BTreeMap` ordering. Introducing `matches()` cleanly separates structural equality from domain equivalence. |
| **Performance** | 🟠 Major | [`src/types/server.rs:43`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs#L43)<br>[`src/com/worker/pool.rs:189`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/com/worker/pool.rs#L189) | **Duplicate COM Connection Overhead:** Unnormalized host casing produced distinct hash keys in `ConnectionPool`, triggering redundant 200–2000ms DCOM connection handshakes and exhausting limited server client seats. |
| **Security** | 🟠 Major | [`src/types/server.rs:21`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs#L21)<br>[`src/types/server.rs:617`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs#L617) | **CWE-626 Null Preservation & Host Isolation:** `normalize_host_str` must NEVER map poisoned strings (`\0`) to `None` (which would convert remote poisoned hosts into local machine calls). Null bytes must strictly error at parser/builder ingress. Case-insensitive comparison must stick to ASCII folding to prevent Unicode homoglyph spoofing. |
| **API** | 🟡 Minor | [`src/types/server.rs:127`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs#L127)<br>[`src/types/server.rs:509`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs#L509) | **Public Ergonomics & Documentation:** `ServerIdentifier::matches` and `OpcServerEndpoint::matches` provide standard semantic comparison tools for consumers; `OpcServerEndpoint::local` requires reattaching the orphaned doc block and adding runnable doctests. |

---

## 5. Technical Deliverables & Implementation Contracts

### 5.1 Eager Lowercase Host Normalization ([`src/types/server.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs))
```rust
#[must_use]
pub(crate) fn normalize_host_str(host: Option<&str>) -> Option<&str> {
    let h = host?.trim();
    if h.is_empty()
        || h.eq_ignore_ascii_case("localhost")
        || h == "127.0.0.1"
        || h == "::1"
    {
        None
    } else {
        Some(h)
    }
}

#[must_use]
pub(crate) fn normalize_host(host: Option<&str>) -> Option<String> {
    normalize_host_str(host).map(str::to_ascii_lowercase)
}
```
*Note: `normalize_host_str` retains loopback alias mapping (`localhost`, `127.0.0.1`, `::1` $\rightarrow$ `None`). It does **not** map interior null bytes (`\0`) to `None`, ensuring poisoned strings are never silently converted into local COM calls. Null-byte rejection is strictly enforced with `Err` at parser and builder ingress boundaries (`from_str:617`, `builder.host:97`, `bind_new_remote:130`).*

### 5.2 Router Alignment in `OpcServerEndpoint::from_str` ([`src/types/server.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs))
In `from_str`, lines 631, 652, and 669 must replace `normalize_host_str(Some(raw_host)).map(str::to_string)` with:
```rust
let host = normalize_host(Some(raw_host));
```

### 5.3 Semantic `matches()` on Domain Models ([`src/types/server.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs))
```rust
impl ServerIdentifier {
    /// Compares two server identifiers for semantic equality according to COM rules.
    ///
    /// ProgIDs are compared case-insensitively using ASCII casing rules, whereas CLSIDs
    /// are compared by exact 128-bit numerical equality. Returns `false` if one identifier
    /// is a ProgID and the other is a CLSID.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::ServerIdentifier;
    ///
    /// let id1: ServerIdentifier = "Matrikon.OPC.Simulation.1".parse().unwrap();
    /// let id2: ServerIdentifier = "matrikon.opc.simulation.1".parse().unwrap();
    /// assert!(id1.matches(&id2));
    /// assert_ne!(id1, id2); // Byte-exact structural equality remains false
    /// ```
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ProgId(a), Self::ProgId(b)) => a.eq_ignore_ascii_case(b),
            (Self::Clsid(a), Self::Clsid(b)) => a == b,
            _ => false,
        }
    }
}

impl OpcServerEndpoint {
    /// Compares two endpoints for semantic equality according to network and COM rules.
    ///
    /// Hostnames are compared case-insensitively, and server identifiers are evaluated
    /// via [`ServerIdentifier::matches`].
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        let host_matches = match (self.host.as_deref(), other.host.as_deref()) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
            (None, None) => true,
            _ => false,
        };
        host_matches && self.identifier.matches(&other.identifier)
    }
}
```

### 5.4 Refactored Client Gateway Validation ([`src/client/gateway.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/client/gateway.rs))
```rust
impl<C: ServerBackend + 'static, State: Send + Sync + 'static> OpcDaClient<C, State> {
    pub(crate) fn validate_bound_server(&self, server: &str) -> OpcResult<OpcServerEndpoint> {
        let requested_ep: OpcServerEndpoint = server.parse()?;
        if let Some(bound_ep) = &self.endpoint {
            let has_explicit_host = server.contains('\\') || server.contains('/');
            let host_mismatch = has_explicit_host
                && match (requested_ep.host(), bound_ep.host()) {
                    (Some(req), Some(bound)) => !req.eq_ignore_ascii_case(bound),
                    (None, None) => false,
                    _ => true,
                };
            if !requested_ep.identifier().matches(bound_ep.identifier()) || host_mismatch {
                return Err(OpcError::InvalidState(format!(
                    "Client is bound to server '{bound_ep}', but request targeted '{server}'"
                )));
            }
            Ok(bound_ep.clone())
        } else {
            Ok(requested_ep)
        }
    }
}
```

### 5.5 Public Constructor Documentation & Doctests ([`src/types/server.rs`](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/src/types/server.rs))
Relocate the orphaned doc block at line 460 directly onto `OpcServerEndpoint::local` at line 508 and supply runnable doc-tests:
```rust
/// Creates a new endpoint targeting the local machine.
///
/// # Arguments
///
/// * `identifier` - The ProgID or CLSID identifying the target server.
///
/// # Examples
///
/// ```
/// use opc_da_client::types::{OpcServerEndpoint, ServerIdentifier};
///
/// let ep = OpcServerEndpoint::local("Matrikon.OPC.Simulation.1");
/// assert!(!ep.is_remote());
/// assert_eq!(ep.host(), None);
/// ```
#[must_use]
pub fn local(identifier: impl Into<ServerIdentifier>) -> Self {
    Self {
        host: None,
        identifier: identifier.into(),
    }
}
```

---

## 6. Blast Radius Table

| Symbol | File | Direct Callers | Indirect Callers | Cross-Crate Impact | Risk Level |
|:---|:---|:---:|:---:|:---:|:---:|
| `normalize_host` | `src/types/server.rs` | 2 | 14 | None (`opc-cli` uses client API) | 🟢 Low |
| `normalize_host_str` | `src/types/server.rs` | 4 | 8 | None (internal `pub(crate)`) | 🟢 Low |
| `<OpcServerEndpoint as FromStr>::from_str` | `src/types/server.rs` | 6 | 28 | None (canonicalizes output) | 🟢 Low |
| `ServerIdentifier::matches` (New) | `src/types/server.rs` | 0 (New) | 0 | None (additive public method) | 🟢 Low |
| `OpcServerEndpoint::matches` (New) | `src/types/server.rs` | 0 (New) | 0 | None (additive public method) | 🟢 Low |
| `OpcDaClient::validate_bound_server` | `src/client/gateway.rs` | 4 | 12 | None (fixes false rejections) | 🟢 Low |
| `ConnectionPool::connections` | `src/com/worker/pool.rs` | 3 | 5 | None (improves cache hit rate) | 🟢 Low |
| `OpcServerEndpoint::local` | `src/types/server.rs` | 5 | 10 | None (pure documentation update) | 🟢 Low |

---

## 7. Things to Watch Out For (Defensive Invariants)

1. **CWE-626 Null-Byte Boundary Protection & Host Isolation:**
   `normalize_host_str` must strictly map known loopback strings (`"localhost"`, `"127.0.0.1"`, `"::1"`, or empty) to `None`. It must **NEVER** map strings containing interior nulls (`\0`) to `None`, as doing so would silently convert remote poisoned host inputs (`"victim\0.evil.com"`) into local machine COM calls. Interior null bytes are strictly rejected with structured errors (`Err`) at public ingress points (`OpcServerEndpoint::from_str`, `OpcDaClientBuilder::host`, `OpcDaClient::bind_new_remote`).
2. **Preserving Derived `Eq` and `Hash` Invariants:**
   Do **NOT** implement custom `PartialEq` or `Hash` on `ServerIdentifier` to fold casing. In Rust, `k1 == k2` must guarantee `k1.to_string() == k2.to_string()` and identical hash keys. Violating this breaks collections, `BTreeMap` ordering, and serde serialization. All case-insensitive comparisons must be explicit via `.matches()`.
3. **Strict ASCII Case Folding:**
   Always use `.eq_ignore_ascii_case()` rather than Unicode lowercase conversion. OPC ProgIDs and NetBIOS hostnames are strictly ASCII. Unicode case folding introduces locale-dependent security bugs (e.g. Turkish dotted/dotless I) and performance overhead.
4. **Connection Pool Key Deduplication Without Custom Hash/Eq:**
   Because `OpcServerEndpoint` eagerly canonicalizes `host` to lowercase via `normalize_host` in `from_str`, `remote`, and `remote_prog_id`, `ConnectionPool` automatically deduplicates connections targeting the same host regardless of input casing (`\\SCADA-01\srv` and `\\scada-01\srv` produce identical hash keys). Do not add custom hashing logic to `ConnectionPool` or override `Hash` on `OpcServerEndpoint`.

---

## 8. TDD Red-Green Verification Matrix

| Test Case | Scope | Red Baseline | Green Success Criteria |
|:---|:---|:---|:---|
| `test_normalize_host_lowercases_remote` | `src/types/server.rs` | Returns `Some("SCADA-01")` | Returns `Some("scada-01")` for `"SCADA-01"` and `None` for `"LOCALHOST"` |
| `test_endpoint_from_str_canonicalizes_host` | `src/types/server.rs` | `host` retains raw uppercase `"NODE1"` | `r"\\NODE1\Server".parse().unwrap().host()` returns `Some("node1")` |
| `test_server_identifier_matches_progid` | `src/types/server.rs` | Method missing | `"Matrikon.OPC.1".parse().matches(&"matrikon.opc.1".parse())` is `true`; `!=` remains `true` |
| `test_server_identifier_matches_clsid` | `src/types/server.rs` | Method missing | Matching CLSIDs return `true`; mismatched CLSIDs or ProgID vs CLSID return `false` |
| `test_endpoint_matches` | `src/types/server.rs` | Method missing | `\\HOST\Srv` matches `\\host\srv`; differing host returns `false` |
| `test_validate_bound_server_mixed_case` | `src/client/tests.rs` | Rejects with `InvalidState` | Bound to `"Matrikon.1"`, validating `"matrikon.1"` succeeds and returns bound endpoint |
| `test_validate_bound_server_host_mismatch` | `src/client/tests.rs` | Passes | Bound to `"host1/Server"`, validating `"host2/Server"` returns `Err(OpcError::InvalidState)` |
| `test_doctest_server_endpoint_local` | `src/types/server.rs` | Missing docs | `cargo test --doc` compiles and passes doc-tests cleanly |

---

📄 **Sub-Block Report:** [`refactor/cycle2_blockh3a_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockh3a_review.md)
