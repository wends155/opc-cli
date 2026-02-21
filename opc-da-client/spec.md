# 📋 spec.md — opc-da-client

> **Behavioral Source of Truth** for the `opc-da-client` library crate.
> Defines *what* each module should do — independent of current implementation.

---

## 1. Module / Component Contracts

### 1.1 `provider` — Core Trait & Data Types

**Purpose:** Define the async trait that all OPC DA backends must implement, plus the canonical data model for tag values.

#### Public API

##### `trait OpcProvider: Send + Sync`

All methods use `#[async_trait]`.

| Method | Signature | Description |
| :--- | :--- | :--- |
| `list_servers` | `async fn list_servers(&self, host: &str) -> Result<Vec<String>>` | Enumerate OPC DA servers available on `host`. |
| `browse_tags` | `async fn browse_tags(&self, server: &str, max_tags: usize, progress: Arc<AtomicUsize>, tags_sink: Arc<Mutex<Vec<String>>>) -> Result<Vec<String>>` | Recursively discover tags on `server`, pushing each to `tags_sink` as found. |
| `read_tag_values` | `async fn read_tag_values(&self, server: &str, tag_ids: Vec<String>) -> Result<Vec<TagValue>>` | Read current value, quality, and timestamp for the given tag IDs. |
| `write_tag_value` | `async fn write_tag_value(&self, server: &str, tag_id: &str, value: OpcValue) -> Result<WriteResult>` | Write a typed value to a single tag on `server`. |

**Error Conditions:**

| Method | Error Condition | Meaning |
| :--- | :--- | :--- |
| `list_servers` | COM init failure | Windows COM subsystem unavailable. |
| `list_servers` | Registry enumeration failure | OPC Core Components not installed or registry corrupt. |
| `browse_tags` | ProgID resolution failure | `server` string does not map to a registered CLSID. |
| `browse_tags` | Server connection failure | DCOM permissions, server offline, or licensing error. |
| `browse_tags` | Namespace walk failure | Browse position corrupted (failed `UP` navigation). |
| `read_tag_values` | ProgID resolution failure | Same as `browse_tags`. |
| `read_tag_values` | No valid items | None of the requested `tag_ids` could be added to the OPC group. |
| `read_tag_values` | Sync read failure | Server-side read error on all items. |
| `write_tag_value` | ProgID resolution failure | Same as `browse_tags`. |
| `write_tag_value` | Item add failure | The `tag_id` could not be added to the OPC group. |
| `write_tag_value` | Sync write failure | Server-side write error (e.g., read-only tag). |

**Invariants:**

*   All methods are `Send + Sync` safe; they are safe to call from an async context.
*   `list_servers` returns a **sorted, deduplicated** list of ProgID strings.
*   `browse_tags` **never** collects more than `max_tags` items.
*   `browse_tags` pushes tags to `tags_sink` incrementally; on timeout the caller can harvest partial results.
*   `browse_tags` updates `progress` atomically for each discovered tag.
*   `read_tag_values` returns a `TagValue` entry only for items that were successfully added to the OPC group; silently skips failed items.
*   `write_tag_value` returns `Ok(WriteResult)` in all non-fatal cases; per-tag success/error is reported inside `WriteResult`.


---

##### `struct TagValue`

**Purpose:** Canonical representation of a single OPC DA tag read result.

| Field | Type | Required | Description | Constraints |
| :--- | :--- | :--- | :--- | :--- |
| `tag_id` | `String` | Yes | Fully qualified tag identifier. | Non-empty. |
| `value` | `String` | Yes | Current value as a display string. | May be `"Empty"`, `"Null"`, or formatted number/string. |
| `quality` | `String` | Yes | OPC quality label. | One of `"Good"`, `"Bad"`, `"Uncertain"`, or `"Unknown(0xNNNN)"`. |
| `timestamp` | `String` | Yes | Last-change timestamp as local time. | Format `YYYY-MM-DD HH:MM:SS`, or `"N/A"` / `"Invalid"`. |

**Derives:** `Debug`, `Clone`.

---

##### `enum OpcValue`

**Purpose:** Typed representation of a value to be written to an OPC DA tag.

| Variant | Data Type | Description | COM VT Type |
| :--- | :--- | :--- | :--- |
| `String(String)` | `String` | Raw string value. | `VT_BSTR` |
| `Int(i32)` | `i32` | 32-bit signed integer. | `VT_I4` |
| `Float(f64)` | `f64` | 64-bit float. | `VT_R8` |
| `Bool(bool)` | `bool` | Boolean value. | `VT_BOOL` |

**Derives:** `Debug`, `Clone`, `PartialEq`.

---

##### `struct WriteResult`

**Purpose:** Canonical representation of an OPC DA tag write result.

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `tag_id` | `String` | Yes | The tag identifier that was written to. |
| `success` | `bool` | Yes | Whether the write operation succeeded. |
| `error` | `Option<String>` | No | Error message or hint if `success` is `false`. |

**Derives:** `Debug`, `Clone`, `PartialEq`.


---

##### `MockOpcProvider` *(feature = `test-support`)*

**Purpose:** Auto-generated mock of `OpcProvider` via `mockall`, exported when the `test-support` feature is enabled.

**Invariants:**
*   Provides `expect_*` methods for each trait method.
*   Must be fully compatible with `#[tokio::test]` async test harnesses.

---

### 1.2 `helpers` — COM Utility Functions

**Purpose:** Provide reusable helpers for COM error mapping, data conversion, and OPC data formatting.

#### Public API

##### `fn friendly_com_hint(err: &anyhow::Error) -> Option<&'static str>`

**Description:** Inspects the debug representation of `err` for known COM/DCOM HRESULT patterns and returns a human-readable hint.

**Inputs:** An `anyhow::Error` reference.
**Output:** `Some(hint)` if a known code is found, `None` otherwise.

**Known Mappings:**

| HRESULT | Hint |
| :--- | :--- |
| `0x80040112` | Server license does not permit OPC client connections |
| `0x80080005` | Server process failed to start — check if it is installed and running |
| `0x80070005` | Access denied — DCOM launch/activation permissions not configured for this user |
| `0x800706BA` | RPC server unavailable — the target host may be offline or blocking RPC |
| `0x800706F4` | COM marshalling error — try restarting the OPC server |
| `0x80040154` | Server is not registered on this machine |
| `0x80004003` | Invalid pointer — likely a known issue with the OPC DA crate's iterator initialization |
| `0xC0040004` | Server rejected write — the item may be read-only (OPC_E_BADRIGHTS) |


**Invariants:**
*   Pure function — no side effects, no I/O, no panics.
*   Pattern matching is case-sensitive on the hex string.

---

#### Internal API (crate-visible only, documented for completeness)

| Function | Signature | Purpose |
| :--- | :--- | :--- |
| `is_known_iterator_bug` | `fn(err: &windows::core::Error) -> bool` | Returns `true` for `E_POINTER` (`0x80004003`) errors from the upstream iterator bug. |
| `guid_to_progid` | `fn(guid: &GUID) -> Result<String>` | Converts a COM GUID to its registered ProgID string. |
| `variant_to_string` | `fn(variant: &VARIANT) -> String` | Formats a COM VARIANT as a display string. |
| `quality_to_string` | `fn(quality: u16) -> String` | Maps OPC quality bitmask to `"Good"` / `"Bad"` / `"Uncertain"`. |
| `filetime_to_string` | `fn(ft: &FILETIME) -> String` | Converts Win32 FILETIME to local `YYYY-MM-DD HH:MM:SS` string. |
| `opc_value_to_variant` | `fn(value: &OpcValue) -> VARIANT` | Converts an `OpcValue` to a COM `VARIANT`. |


---

### 1.3 `backend::opc_da` — Default OPC DA Backend

**Purpose:** Concrete `OpcProvider` implementation using the `opc_da` crate. Handles COM MTA initialization, server connection, namespace browsing, and synchronous I/O reads.

> [!NOTE]
> Only compiled when feature `opc-da-backend` is enabled (default).

#### Public API

##### `struct OpcDaWrapper`

| Method | Signature | Description |
| :--- | :--- | :--- |
| `new()` | `fn new() -> Self` | Constructs a new wrapper. |
| `default()` | `fn default() -> Self` | Same as `new()`. |

Implements `OpcProvider` for all four trait methods.

**Invariants:**
*   All COM work runs on a dedicated blocking thread via `tokio::task::spawn_blocking`.
*   COM is initialized via `ComGuard::new()` at the start of every blocking task; teardown is automatic on drop.
*   GUID filtering: zeroed GUIDs are skipped during server enumeration.
*   Server list is sorted and deduplicated before returning.

#### Internal: `browse_recursive`

**Signature:**
```rust
fn browse_recursive(
    server: &Server,
    tags: &mut Vec<String>,
    max_tags: usize,
    progress: &Arc<AtomicUsize>,
    tags_sink: &Arc<Mutex<Vec<String>>>,
    depth: usize,
) -> Result<()>
```

**Behavior:**
1.  Terminates if `depth > 50` (MAX_DEPTH) or `tags.len() >= max_tags`.
2.  Enumerates `OPC_BRANCH` items, descends into each via `change_browse_position(DOWN)`.
3.  **Always** navigates back `UP` after recursing — even if recursion itself fails — to prevent position corruption. Failure to navigate `UP` is a hard error.
4.  Enumerates `OPC_LEAF` items (soft-fail: errors logged and skipped).
5.  Converts browse names to fully-qualified item IDs via `get_item_id()`; falls back to browse name on failure.
6.  `E_POINTER` errors from `StringIterator` are filtered to `trace!` level.
7.  Each discovered tag is pushed to both `tags` and `tags_sink`, and `progress` is incremented.

---

### 1.4 `com_guard` — RAII COM Initialization

**Purpose:** Provide a drop guard that ensures `CoUninitialize` is called exactly once per successful `CoInitializeEx`, even on early returns or panics.

#### Public API

##### `struct ComGuard`

| Method | Signature | Description |
| :--- | :--- | :--- |
| `new()` | `fn new() -> anyhow::Result<Self>` | Initialize COM in Multi-Threaded Apartment (MTA) mode. Returns `Ok` on success or if already initialized (`S_FALSE`). |

**Drop behavior:** Calls `CoUninitialize` only if `CoInitializeEx` returned `Ok`.

**Error Conditions:**

| Error | Meaning |
| :--- | :--- |
| Fatal HRESULT from `CoInitializeEx` | Windows COM subsystem is unavailable or misconfigured. |

**Invariants:**
*   Must be used on the **same thread** that called `new()`.
*   `S_FALSE` (already initialized) is treated as success — the guard will still call `CoUninitialize` on drop.
*   The guard is **not** `Send` or `Sync` — it must remain on the thread that created it.

**Required Test Coverage:**
- [x] Doctest: `ComGuard::new()?` compiles in a `no_run` example.

---

## 2. Data Models

### `TagValue`

Defined in § 1.1. See table above.

### Feature Flags

| Flag | Default | Effect |
| :--- | :--- | :--- |
| `opc-da-backend` | ✅ Yes | Compiles the `backend::opc_da` module and exports `OpcDaWrapper`. |
| `test-support` | ❌ No | Enables `mockall` and exports `MockOpcProvider`. |

---

## 3. Integration Points

### 3.1 Upstream: `opc_da` Crate (v0.3.1)

**Boundary:** `OpcDaWrapper` → `opc_da::client::v2::Client` / `Server`.

| Operation | `opc_da` API Used |
| :--- | :--- |
| Server enumeration | `Client.get_servers()` |
| Server connection | `Client.create_server()` |
| Namespace detection | `Server.query_organization()` |
| Tag browsing | `Server.browse_opc_item_ids()`, `Server.change_browse_position()`, `Server.get_item_id()` |
| Tag reading | `Server.add_group()`, group `.add_items()`, group `.read()`, `Server.remove_group()` |
| Tag writing | `Server.add_group()`, group `.add_items()`, group `.write()`, `Server.remove_group()` |
| String iteration | `StringIterator::new()` |

**Error Handling at Boundary:**
*   All `opc_da` errors are wrapped with `anyhow::Context` to add operation context.
*   `create_server` failures additionally log a `friendly_com_hint` before propagating.
*   `E_POINTER` errors from `StringIterator` are identified via `is_known_iterator_bug()` and silenced to `trace!` level.

**Known Upstream Bugs:**

| ID | Bug | Workaround |
| :--- | :--- | :--- |
| OPC-BUG-001 | `StringIterator` produces 16 phantom `E_POINTER` errors per iterator | `is_known_iterator_bug()` filter + `trace!`-level logging |

### 3.2 Downstream: `opc-cli` (Consumer)

**Boundary:** `opc-cli` → `dyn OpcProvider`.

*   The CLI crate depends on the `OpcProvider` trait, never on `OpcDaWrapper` directly in its core logic.
*   Tests use `MockOpcProvider` (via `test-support` feature).
*   `friendly_com_hint()` is called by the CLI to enrich error messages displayed in the TUI status bar.

---

## 4. Required Test Coverage

### Unit Tests (existing in `helpers.rs`)

- [x] `friendly_com_hint` returns correct hint for known HRESULT codes.
- [x] `friendly_com_hint` returns `None` for unknown errors.
- [x] `filetime_to_string` returns `"N/A"` for zero FILETIME.
- [x] `filetime_to_string` produces valid date string for non-zero FILETIME.
- [x] `is_known_iterator_bug` returns `true` for `E_POINTER` code.
- [x] `is_known_iterator_bug` returns `false` for other error codes.
- [x] `opc_value_to_variant` correctly converts `Int` variant.
- [x] `variant_to_string` roundtrips through `VT_I4` and `VT_R4`.
- [x] `variant_to_string` handles `VT_EMPTY` and `VT_NULL`.
- [x] `variant_to_string` handles `VT_CY` (currency).
- [x] `variant_to_string` returns `(VT ...)` for unknown variant types.

### Unit Tests (recommended additions)

- [ ] `quality_to_string` returns `"Good"` for `0xC0`.
- [ ] `quality_to_string` returns `"Bad"` for `0x00`.
- [ ] `quality_to_string` returns `"Uncertain"` for `0x40`.
- [ ] `quality_to_string` returns `"Unknown(…)"` for unrecognized bitmask.

### Mock-Based Tests (in `opc-cli`)

- [x] `MockOpcProvider` returns expected server list.
- [x] `MockOpcProvider` returns expected browse results.
- [x] `MockOpcProvider` returns expected tag values.
- [x] `MockOpcProvider` simulates error conditions for UI error handling.

### Doc Tests

- [x] `friendly_com_hint` — runnable doctest in `helpers.rs`.
- [x] `ComGuard` — `no_run` compile-check doctest in `com_guard.rs`.

### Integration / Manual Tests

- [ ] `list_servers("localhost")` returns non-empty list on a machine with OPC servers installed.
- [ ] `browse_tags` correctly discovers tags on a flat-namespace server.
- [ ] `browse_tags` correctly discovers tags on a hierarchical-namespace server.
- [ ] `browse_tags` respects `max_tags` cap.
- [ ] `browse_tags` populates `tags_sink` incrementally (observable via progress counter).
- [ ] `read_tag_values` returns correct value/quality/timestamp for known tags.
- [ ] `read_tag_values` gracefully handles tags that fail `add_items`.
- [ ] `write_tag_value` returns success for a valid write to a simulation tag.
- [ ] `write_tag_value` returns error (with hint) when writing to a read-only tag.
- [ ] `opc_value_to_variant` correctly converts all `OpcValue` variants.

