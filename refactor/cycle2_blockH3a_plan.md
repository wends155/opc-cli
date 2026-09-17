# Sub-Block H3a: Server Identity & Host Canonicalization

**Role:** Architect • **Date:** 2026-09-17 • **Tier:** M
**Scope:** Eager ASCII lowercase host canonicalization, semantic `matches()` methods on `ServerIdentifier` and `OpcServerEndpoint`, case-insensitive `validate_bound_server`, and public constructor documentation.

---

### Builder Context

Read before starting:

- `opc-da-client/src/types/server.rs` L21-45 (normalize_host functions), L126-194 (ServerIdentifier), L450-545 (OpcServerEndpoint constructors), L609-681 (from_str)
- `opc-da-client/src/client/gateway.rs` L376-391 (validate_bound_server)
- `opc-da-client/src/com/worker/pool.rs` L189-195 (ConnectionPool struct)
- `opc-da-client/src/client/tests.rs` (existing test patterns)
- `architecture.md §8` (Error Handling Strategy)
- `coding-standard.md §4.4` (Type System & API Design — `#[must_use]`)

### Phase Context

- **Phase:** H3a of H3 (H3a → H3b → H3c strictly sequential)
- **Prior phases:** H1 (Domain Invariants & CWE-626) and H2 (COM Resource & Dead Code Pruning) completed.
- **Stubs for this phase:** None (clean additive implementation).

---

## Problem Statement

In industrial Windows COM environments, NetBIOS/DNS hostnames and OPC DA ProgIDs registered in `HKEY_CLASSES_ROOT` are case-insensitive. Prior to Sub-Block H3a, hostname casing was unnormalized during endpoint parsing, and bound client validation evaluated byte-exact inequality (`!=`). This caused:

1. **Connection Pool Key Fragmentation (Performance):** Endpoints targeting the same host with varying casing (e.g., `\\SRV1\prog` vs `\\srv1\prog`) created duplicate active DCOM connections in `ConnectionPool`, incurring 200–2000ms connection latencies and exhausting server client license seats.

2. **False-Positive Session Rejections (Logic):** Bound clients rejected valid requests targeting differing casing (e.g., `"Matrikon.OPC.Simulation.1"` vs `"matrikon.opc.simulation.1"`) with `OpcError::InvalidState`.

**Root Causes:**

- `normalize_host` at `server.rs:43-45` calls `.map(str::to_string)` instead of `.map(str::to_ascii_lowercase)`.
- `OpcServerEndpoint::from_str` at `server.rs:631,652,669` bypasses `normalize_host` by calling `normalize_host_str(…).map(str::to_string)` directly.
- `validate_bound_server` at `gateway.rs:380` uses `!=` (byte-exact) instead of semantic `.matches()`.

**Constraints:**

- ASCII case folding ONLY — no Unicode `.to_lowercase()` (CWE-178 prevention).
- `normalize_host_str` must NEVER map null bytes to `None` (CWE-626 boundary).
- Derived `PartialEq, Eq, Hash` on `ServerIdentifier` and `OpcServerEndpoint` must remain byte-exact.
- Zero cross-crate impact (`opc-cli` does not reference these types directly).

**Dependencies:** Uses existing `Clsid` (derives `PartialEq, Eq`), `normalize_host_str`, `OpcServerEndpoint::host()`, `ServerIdentifier` variants.

**Input Report:** [`refactor/cycle2_blockh3a_review.md`](file:///c:/Users/WSALIGAN/code/opc-cli/refactor/cycle2_blockh3a_review.md) — all 6 objectives mapped to proposed changes.

---

## Plan Objectives

| ID | Objective | Success Criteria | Steps |
|----|-----------|-----------------|-------|
| O1 | Eager hostname lowercase canonicalization | `normalize_host(Some("SCADA-01"))` returns `Some("scada-01")`; all `from_str` paths route through `normalize_host` | 1-4 |
| O2 | Semantic `ServerIdentifier::matches()` | `"Matrikon.OPC.1".parse::<ServerIdentifier>().matches(&"matrikon.opc.1".parse())` returns `true`; `!=` remains `true` | 5-7 |
| O3 | Semantic `OpcServerEndpoint::matches()` | `\\HOST\Srv` matches `\\host\srv`; differing host returns `false` | 8-9 |
| O4 | Case-insensitive client session validation | `validate_bound_server` permits mixed-case queries matching bound server | 10-12 |
| O5 | Connection pool deduplication verification | `HashSet` deduplicates case-varying endpoints after O1 canonicalization | 3 |
| O6 | Public constructor documentation & doctests | `OpcServerEndpoint::local` has full rustdoc with runnable doctest passing `cargo test --doc` | 13-14 |

---

## Review History & Verdict

| Cycle | Reviewer | Model | Verdict | Adjustments Applied |
|---|---|---|---|---|
| 1 | `plan-reviewer` | `flash` | ⚠️ Revisions Recommended | Initial Draft — 3 findings |
| 2 | `plan-reviewer` | `flash` | ✅ Approved | Fixed broken `local` doctest (`ServerIdentifier::new().unwrap()`); clarified Step 13 to preserve `pub fn new` docs; added cross-variant registry note to `ServerIdentifier::matches`; 2 advisory findings incorporated |

---

## Negative Scope

**Out of Scope:**

- Do NOT modify `ConnectionPool` production code (`pool.rs`) — deduplication is verified via test only
- Do NOT implement custom `PartialEq` or `Hash` on `ServerIdentifier` or `OpcServerEndpoint`
- Do NOT add null-byte checks to `normalize_host_str` (already rejected at ingress)
- Do NOT use Unicode `.to_lowercase()` anywhere
- Do NOT modify `src/client/mod.rs`, `src/client/builder.rs`, or `src/com/discovery.rs`
- Do NOT touch Sub-Block H3b or H3c scope

---

## Interface Contracts

### `ServerIdentifier::matches(&self, other: &Self) -> bool` (NEW)

```rust
impl ServerIdentifier {
    /// Compares two server identifiers for semantic equivalence under OPC DA naming rules.
    ///
    /// Programmatic Identifiers ([`ServerIdentifier::ProgId`]) are compared case-insensitively
    /// using ASCII folding. Class IDs ([`ServerIdentifier::Clsid`]) are compared for exact
    /// 128-bit numerical equality. Cross-variant comparisons always return `false` because
    /// resolving a ProgID to its CLSID requires Windows COM registry activation
    /// (`CLSIDFromProgID`), which is intentionally excluded from pure in-memory comparison.
    ///
    /// Unlike the derived [`PartialEq`] implementation (which is byte-exact for map and set
    /// key stability), `matches` implements domain-level equivalence.
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::ServerIdentifier;
    ///
    /// let id1 = ServerIdentifier::new("Matrikon.OPC.Simulation.1").unwrap();
    /// let id2 = ServerIdentifier::new("matrikon.opc.simulation.1").unwrap();
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
```

### `OpcServerEndpoint::matches(&self, other: &Self) -> bool` (NEW)

```rust
impl OpcServerEndpoint {
    /// Compares two endpoints for semantic equivalence under OPC DA and network naming rules.
    ///
    /// Hostnames are compared case-insensitively using ASCII folding. Server identifiers
    /// delegate to [`ServerIdentifier::matches`].
    ///
    /// # Examples
    ///
    /// ```
    /// use opc_da_client::types::OpcServerEndpoint;
    ///
    /// let ep1 = OpcServerEndpoint::remote_prog_id("HOST-01", "Matrikon.OPC.Simulation.1");
    /// let ep2 = OpcServerEndpoint::remote_prog_id("host-01", "matrikon.opc.simulation.1");
    /// assert!(ep1.matches(&ep2));
    /// ```
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

---

## Blast Radius Table

| Symbol | File | Direct Callers | Indirect Deps | Test-Only | Cross-Crate? |
|--------|------|---------------|---------------|-----------|--------------|
| `normalize_host` | `src/types/server.rs` | 5 | 8 | No | No |
| `normalize_host_str` | `src/types/server.rs` | 5 | 12 | No | No |
| `OpcServerEndpoint::from_str` | `src/types/server.rs` | 8 | Many | No | No |
| `ServerIdentifier::matches` (New) | `src/types/server.rs` | 1 | 5 | No | No |
| `OpcServerEndpoint::matches` (New) | `src/types/server.rs` | 0 (New) | 0 | No | No |
| `validate_bound_server` | `src/client/gateway.rs` | 5 | 12 | No | No |
| `OpcServerEndpoint::local` | `src/types/server.rs` | 80 | Test suites | Yes | No |

---

## Security Constraints

**CWE-626 Null-Byte Boundary Protection:**

- `normalize_host_str` must NEVER map `\0`-containing strings to `None`.
- Null bytes are rejected at 3 ingress points: `from_str:617`, `builder.host:95`, `bind_new_remote:130`.
- Post verification: `rg "contains.*\\\\0" opc-da-client/src/types/server.rs opc-da-client/src/client/builder.rs opc-da-client/src/client/mod.rs` (expects: ≥3 matches).

**CWE-178 ASCII Case Folding:**

- All case folding uses `to_ascii_lowercase()` and `eq_ignore_ascii_case()`.
- Post verification: `rg "to_lowercase\(\)" opc-da-client/src/` (expects: 0 matches).

**Local Machine Isolation:**

- Only `""`, `"localhost"`, `"127.0.0.1"`, `"::1"` map to `None` (local COM activation).

---

## Edge Cases & Risks

1. **Existing host normalization tests pass unchanged:** `test_host_normalization_and_remote_detection` at `server.rs:956-971` asserts `normalize_host(Some("192.168.1.50"))` returns `Some("192.168.1.50".to_string())`. This test PASSES after our change because IP addresses are already lowercase.
2. **ProgID preservation in Display/Debug:** ProgIDs are NOT lowercased — they retain original casing. Only `host` is lowercased. The `Display` impl for `OpcServerEndpoint` shows the stored (now-lowercase) host.
3. **`normalize_host_str` unchanged:** We do NOT modify `normalize_host_str`. It remains a borrowing function returning the original casing. Only `normalize_host` (the owning wrapper) adds lowercase.

---

## Test Plan (TDD)

**Test Cases (8 total):**

1. `test_normalize_host_lowercases_remote` — Unit in `src/types/server.rs`
2. `test_endpoint_from_str_canonicalizes_host` — Unit in `src/types/server.rs` (includes O5 HashSet dedup)
3. `test_server_identifier_matches_progid` — Unit in `src/types/server.rs`
4. `test_server_identifier_matches_clsid` — Unit in `src/types/server.rs`
5. `test_endpoint_matches` — Unit in `src/types/server.rs`
6. `test_validate_bound_server_mixed_case` — Unit in `src/client/tests.rs`
7. `test_validate_bound_server_host_mismatch` — Unit in `src/client/tests.rs`
8. Doc-test on `OpcServerEndpoint::local` — Doc in `src/types/server.rs`

---

## Global Execution Order

### Step 1: [TEST] `opc-da-client/src/types/server.rs` — [+] `test_normalize_host_lowercases_remote`

- Pre: ALL
- Target: `mod tests` block (append)
- Action: Add test asserting `normalize_host(Some("SCADA-01"))` returns `Some("scada-01")`, loopback aliases return `None`, `normalize_host_str` preserves original casing.

```rust
#[test]
fn test_normalize_host_lowercases_remote() {
    assert_eq!(normalize_host(Some("SCADA-01")), Some("scada-01".to_string()));
    assert_eq!(normalize_host(Some("Remote-PLC-01")), Some("remote-plc-01".to_string()));
    assert_eq!(normalize_host(Some("  SCADA-NODE-01  ")), Some("scada-node-01".to_string()));
    assert_eq!(normalize_host(Some("192.168.1.50")), Some("192.168.1.50".to_string()));
    assert_eq!(normalize_host(Some("localhost")), None);
    assert_eq!(normalize_host(Some("LOCALHOST")), None);
    assert_eq!(normalize_host(Some("127.0.0.1")), None);
    assert_eq!(normalize_host(Some("::1")), None);
    assert_eq!(normalize_host(None), None);
    assert_eq!(normalize_host_str(Some("SCADA-01")), Some("SCADA-01"));
}
```

- Post: RED(test_normalize_host_lowercases_remote)

---

### Step 2: [MODIFY] `opc-da-client/src/types/server.rs` — [~] `normalize_host` (L43-45)

- Pre: RED(test_normalize_host_lowercases_remote)
- Target: `normalize_host` function body and doc comment
- Action: Replace `.map(str::to_string)` with `.map(str::to_ascii_lowercase)`. Update doc comment to reflect lowercase canonicalization.

```rust
/// Normalizes a host string, returning `None` if it represents the local machine.
///
/// Strings that are empty, whitespace-only, `"localhost"`, `"127.0.0.1"`, or `"::1"`
/// (case-insensitive) are normalized to `None`. All other hosts return
/// `Some(lowercase_host)` using ASCII case folding.
#[must_use]
pub(crate) fn normalize_host(host: Option<&str>) -> Option<String> {
    normalize_host_str(host).map(str::to_ascii_lowercase)
}
```

- Post: GREEN(test_normalize_host_lowercases_remote)

---

### Step 3: [TEST] `opc-da-client/src/types/server.rs` — [+] `test_endpoint_from_str_canonicalizes_host`

- Pre: CHECK
- Target: `mod tests` block (append)
- Action: Add test asserting all 4 parsing paths canonicalize host to lowercase, and HashSet deduplicates case-varying endpoints (O5 verification).

```rust
#[test]
fn test_endpoint_from_str_canonicalizes_host() {
    use std::collections::HashSet;
    use std::str::FromStr;

    let ep_unc = OpcServerEndpoint::from_str(r"\\SCADA-NODE1\Matrikon.OPC.Simulation.1").unwrap();
    assert_eq!(ep_unc.host(), Some("scada-node1"));

    let ep_slash = OpcServerEndpoint::from_str("//SCADA-NODE1/Matrikon.OPC.Simulation.1").unwrap();
    assert_eq!(ep_slash.host(), Some("scada-node1"));

    let ep_uri = OpcServerEndpoint::from_str("opc.da://SCADA-NODE1/Matrikon.OPC.Simulation.1").unwrap();
    assert_eq!(ep_uri.host(), Some("scada-node1"));

    let ep_raw = OpcServerEndpoint::from_str("SCADA-NODE1/Matrikon.OPC.Simulation.1").unwrap();
    assert_eq!(ep_raw.host(), Some("scada-node1"));

    // O5: Connection pool deduplication guarantee
    let ep_upper = OpcServerEndpoint::from_str(r"\\SCADA-01\Server.1").unwrap();
    let ep_lower = OpcServerEndpoint::from_str(r"\\scada-01\Server.1").unwrap();
    assert_eq!(ep_upper, ep_lower);
    let pool: HashSet<OpcServerEndpoint> = HashSet::from([ep_upper, ep_lower]);
    assert_eq!(pool.len(), 1, "Case-varying endpoints must deduplicate");
}
```

- Post: RED(test_endpoint_from_str_canonicalizes_host)

---

### Step 4: [MODIFY] `opc-da-client/src/types/server.rs` — [~] `<OpcServerEndpoint as FromStr>::from_str` (L631, L652, L669)

- Pre: RED(test_endpoint_from_str_canonicalizes_host)
- Target: 3 host-parsing lines in `from_str`
- Action: Replace `normalize_host_str(Some(raw_host)).map(str::to_string)` with `normalize_host(Some(raw_host))` at all 3 locations (L631, L652, L669).

```rust
// At each of the 3 host-parsing branches:
let host = normalize_host(Some(raw_host));
```

- Post: GREEN(test_endpoint_from_str_canonicalizes_host), ALL
- 🔒 CHECKPOINT

---

### Step 5: [TEST] `opc-da-client/src/types/server.rs` — [+] `test_server_identifier_matches_progid`

- Pre: ALL
- Target: `mod tests` block (append)
- Action: Add test asserting ProgID case-insensitive matching via `matches()`, preserved byte-exact `!=`, ProgID-vs-CLSID `false`.

> **Builder Note:** Since `matches()` does not yet exist, the test will not compile. Add a minimal stub `pub fn matches(&self, _other: &Self) -> bool { false }` on `ServerIdentifier` first so the test compiles and fails at runtime (RED semantics).

```rust
#[test]
fn test_server_identifier_matches_progid() {
    use std::str::FromStr;
    let id_mixed = ServerIdentifier::from_str("Matrikon.OPC.Simulation.1").unwrap();
    let id_lower = ServerIdentifier::from_str("matrikon.opc.simulation.1").unwrap();
    let id_upper = ServerIdentifier::from_str("MATRIKON.OPC.SIMULATION.1").unwrap();
    let id_different = ServerIdentifier::from_str("Kepware.KEPServerEX.V6").unwrap();

    assert!(id_mixed.matches(&id_lower));
    assert!(id_mixed.matches(&id_upper));
    assert!(id_lower.matches(&id_mixed));
    assert_ne!(id_mixed, id_lower);
    assert!(!id_mixed.matches(&id_different));

    let clsid_id = ServerIdentifier::from_str("{28E68F9A-8D75-11D1-8DC3-3C302A000000}").unwrap();
    assert!(!id_mixed.matches(&clsid_id));
}
```

- Post: RED(test_server_identifier_matches_progid)

---

### Step 6: [TEST] `opc-da-client/src/types/server.rs` — [+] `test_server_identifier_matches_clsid`

- Pre: CHECK
- Target: `mod tests` block (append)
- Action: Add test asserting CLSID 128-bit exact matching, differing CLSIDs `false`, CLSID-vs-ProgID `false`.

```rust
#[test]
fn test_server_identifier_matches_clsid() {
    use std::str::FromStr;
    let id_bracketed = ServerIdentifier::from_str("{28E68F9A-8D75-11D1-8DC3-3C302A000000}").unwrap();
    let id_unbracketed = ServerIdentifier::from_str("28e68f9a-8d75-11d1-8dc3-3c302a000000").unwrap();
    assert!(id_bracketed.matches(&id_unbracketed));

    let id_zero = ServerIdentifier::Clsid(Clsid::zeroed());
    assert!(!id_bracketed.matches(&id_zero));

    let id_prog = ServerIdentifier::from_str("Matrikon.OPC.Simulation.1").unwrap();
    assert!(!id_bracketed.matches(&id_prog));
}
```

- Post: RED(test_server_identifier_matches_clsid)

---

### Step 7: [MODIFY] `opc-da-client/src/types/server.rs` — [+] `ServerIdentifier::matches` (L~185)

- Pre: RED(test_server_identifier_matches_progid), RED(test_server_identifier_matches_clsid)
- Target: `impl ServerIdentifier` block
- Action: Replace stub with full `matches()` method with full rustdoc and doctest (see Interface Contracts above for full code body including cross-variant registry boundary note).
- Post: GREEN(test_server_identifier_matches_progid), GREEN(test_server_identifier_matches_clsid)

---

### Step 8: [TEST] `opc-da-client/src/types/server.rs` — [+] `test_endpoint_matches`

- Pre: CHECK
- Target: `mod tests` block (append)
- Action: Add test asserting case-insensitive endpoint matching across remote and local endpoints.

> **Builder Note:** Since `OpcServerEndpoint::matches()` does not yet exist, add a minimal stub `pub fn matches(&self, _other: &Self) -> bool { false }` on `OpcServerEndpoint` first so the test compiles and fails at runtime (RED semantics).

```rust
#[test]
fn test_endpoint_matches() {
    use std::str::FromStr;
    let ep1 = OpcServerEndpoint::from_str(r"\\SCADA-01\Matrikon.OPC.Simulation.1").unwrap();
    let ep2 = OpcServerEndpoint::from_str(r"\\scada-01\matrikon.opc.simulation.1").unwrap();
    assert!(ep1.matches(&ep2));

    let ep_other = OpcServerEndpoint::from_str(r"\\scada-02\matrikon.opc.simulation.1").unwrap();
    assert!(!ep1.matches(&ep_other));

    let local1 = OpcServerEndpoint::from_str("Matrikon.OPC.Simulation.1").unwrap();
    let local2 = OpcServerEndpoint::from_str("matrikon.opc.simulation.1").unwrap();
    assert!(local1.matches(&local2));
    assert!(!local1.matches(&ep1));
}
```

- Post: RED(test_endpoint_matches)

---

### Step 9: [MODIFY] `opc-da-client/src/types/server.rs` — [+] `OpcServerEndpoint::matches` (L~526)

- Pre: RED(test_endpoint_matches)
- Target: `impl OpcServerEndpoint` block
- Action: Replace stub with full `matches()` method with full rustdoc and doctest (see Interface Contracts above for full code body).
- Post: GREEN(test_endpoint_matches), ALL
- 🔒 CHECKPOINT

---

### Step 10: [TEST] `opc-da-client/src/client/tests.rs` — [+] `test_validate_bound_server_mixed_case`

- Pre: ALL
- Target: `mod tests` block (append)
- Action: Add test asserting case-varying ProgID and host queries succeed against bound client.

```rust
#[test]
fn test_validate_bound_server_mixed_case() {
    let connector = MockServerConnector::new();

    // Local bound: case-varying ProgID
    let local_client = OpcDaClient::builder()
        .with_connector(connector.clone())
        .server("Matrikon.OPC.Simulation.1")
        .build_bound()
        .expect("must build");
    local_client.validate_bound_server("matrikon.opc.simulation.1")
        .expect("case-varying ProgID must validate");

    // Remote bound: case-varying host + ProgID (UNC)
    let remote_client = OpcDaClient::builder()
        .with_connector(connector)
        .host("SCADA-01")
        .server("Matrikon.OPC.Simulation.1")
        .build_bound()
        .expect("must build");
    remote_client.validate_bound_server(r"\\scada-01\matrikon.opc.simulation.1")
        .expect("case-varying UNC must validate");
}
```

- Post: RED(test_validate_bound_server_mixed_case)

---

### Step 11: [MODIFY] `opc-da-client/src/client/gateway.rs` — [~] `validate_bound_server` (L376-391)

- Pre: RED(test_validate_bound_server_mixed_case)
- Target: `validate_bound_server` method body
- Action: Replace byte-exact `!=` comparisons with semantic `.matches()`:

```rust
pub(crate) fn validate_bound_server(&self, server: &str) -> OpcResult<OpcServerEndpoint> {
    let requested_ep: OpcServerEndpoint = server.parse()?;
    if let Some(bound_ep) = &self.endpoint {
        let has_explicit_host = server.contains('\\') || server.contains('/');
        let mismatch = if has_explicit_host {
            !requested_ep.matches(bound_ep)
        } else {
            !requested_ep.identifier().matches(bound_ep.identifier())
        };
        if mismatch {
            return Err(OpcError::InvalidState(format!(
                "Client is bound to server '{bound_ep}', but request targeted '{server}'"
            )));
        }
        Ok(bound_ep.clone())
    } else {
        Ok(requested_ep)
    }
}
```

- Post: GREEN(test_validate_bound_server_mixed_case)

---

### Step 12: [TEST] `opc-da-client/src/client/tests.rs` — [+] `test_validate_bound_server_host_mismatch`

- Pre: CHECK
- Target: `mod tests` block (append)
- Action: Add test asserting true host mismatches and ProgID mismatches return `Err(InvalidState)`.

```rust
#[test]
fn test_validate_bound_server_host_mismatch() {
    let connector = MockServerConnector::new();

    let remote_client = OpcDaClient::builder()
        .with_connector(connector.clone())
        .host("host1")
        .server("Server.1")
        .build_bound()
        .expect("must build");

    let err = remote_client.validate_bound_server(r"\\host2\Server.1").unwrap_err();
    assert!(matches!(err, OpcError::InvalidState(_)));

    let err2 = remote_client.validate_bound_server(r"\\host1\Other.2").unwrap_err();
    assert!(matches!(err2, OpcError::InvalidState(_)));

    // Local bound rejects explicit remote host
    let local_client = OpcDaClient::builder()
        .with_connector(connector)
        .server("Server.1")
        .build_bound()
        .expect("must build");
    let err3 = local_client.validate_bound_server(r"\\host1\Server.1").unwrap_err();
    assert!(matches!(err3, OpcError::InvalidState(_)));
}
```

- Post: GREEN(test_validate_bound_server_host_mismatch), ALL
- 🔒 CHECKPOINT

---

### Step 13: [MODIFY] `opc-da-client/src/types/server.rs` — [~] `OpcServerEndpoint::local` (L460-514)

- Pre: ALL
- Target: Orphaned doc block at L460-473 (misattached to `pub fn new`) and `pub fn local` at L509
- Action: **Extract only the orphaned doc block lines** (the `/// Creates a new endpoint targeting the local machine...` block at L460-473) from above `pub fn new`. **Preserve `pub fn new`'s own documentation** (`/// Parses an endpoint from a string representation...`). Place the extracted doc block directly above `pub fn local` at L509, with corrected doctest using `ServerIdentifier::new().unwrap()` (since `From<&str>` was deleted in H1):

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
/// let id = ServerIdentifier::new("Matrikon.OPC.Simulation.1").unwrap();
/// let ep = OpcServerEndpoint::local(id);
/// assert!(!ep.is_remote());
/// assert_eq!(ep.host(), None);
/// assert_eq!(ep.identifier().to_string(), "Matrikon.OPC.Simulation.1");
/// ```
#[must_use]
pub fn local(identifier: impl Into<ServerIdentifier>) -> Self {
```

- Post: CHECK

---

### Step 14: [MODIFY] `opc-da-client/src/types/server.rs` — verification

- Pre: CHECK
- Target: Doctests
- Action: Run `cargo test --doc -p opc-da-client` to verify all doctests pass including new ones on `local`, `matches`.
- Post: ALL
- 🔒 CHECKPOINT

---

## Verification Plan

| Type | Command |
|------|---------|
| Unit Tests | `cargo test -p opc-da-client --lib` |
| Doc Tests | `cargo test --doc -p opc-da-client --all-features` |
| Integration Tests | `cargo test -p opc-da-client --test '*'` |
| Full Pipeline | `pwsh -File scripts/verify.ps1` |
| Security: No Unicode lowercase | `rg "to_lowercase\(\)" opc-da-client/src/` (expects: 0 matches) |
| Security: Null-byte guards | `rg "contains.*\\\\0" opc-da-client/src/types/server.rs opc-da-client/src/client/builder.rs opc-da-client/src/client/mod.rs` (expects: ≥3 matches) |

---

## Plan Summary

| Metric | Value |
|--------|-------|
| Tier | M |
| Files | 3 (server.rs, gateway.rs, tests.rs) |
| Steps | 14 |
| Checkpoints | 4 |
| New Tests | 7 unit + 1 doctest |
| New Methods | 2 public (`matches`) |
| Modified Functions | 3 (`normalize_host`, `from_str`, `validate_bound_server`) |
| Estimated effort | Medium |
