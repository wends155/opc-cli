# 📋 spec.md — opc-da-client

> **Behavioral Source of Truth** for the `opc-da-client` library crate.
> Defines *what* each module should do — independent of current implementation.
>
> Last verified against: b9a0292

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
*   `read_tag_values` returns a `TagValue` entry for all requested tags, preserving the original array length and order. Items that fail to be added to the group receive quality `OpcQuality::BAD_CONFIG_ERROR`, and items that fail reading receive `OpcQuality::BAD_COMM_FAILURE` with `value` set to `"Error"`.
*   `write_tag_value` returns `Ok(WriteResult)` in all non-fatal cases; per-tag success/error is reported inside `WriteResult`.


---

##### `struct TagValue`

**Purpose:** Canonical representation of a single OPC DA tag read result.

| Field | Type | Required | Description | Constraints |
| :--- | :--- | :--- | :--- | :--- |
| `tag_id` | `String` | Yes | Fully qualified tag identifier. | Non-empty. |
| `value` | `String` | Yes | Current value as a display string. | May be `"Empty"`, `"Null"`, or formatted number/string. |
| `quality` | `OpcQuality` | Yes | Decomposed 16-bit OPC DA quality word. | `Copy`, `Display` formats rich human-readable status. |
| `timestamp` | `String` | Yes | Last-change timestamp as local time. | Format `YYYY-MM-DD HH:MM:SS`, or `"N/A"` / `"Invalid"`. |

**Derives:** `Debug`, `Clone`, `PartialEq`, `Eq`.

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

##### `fn friendly_com_hint(error: &OpcError) -> Option<&'static str>`

**Description:** Inspects the `OpcError` instance for known COM/DCOM HRESULT patterns and returns a human-readable hint.

**Inputs:** An `OpcError` reference.
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
| `0x80004003` | Invalid pointer (E_POINTER) |
| `0xC0040004` | Server rejected write — the item may be read-only (OPC_E_BADRIGHTS) |
| `0xC0040006` | Data type mismatch — server cannot convert the written value (OPC_E_BADTYPE) |
| `0xC0040007` | Item ID not found in server address space (OPC_E_UNKNOWNITEMID) |
| `0xC0040008` | Item ID syntax is invalid for this server (OPC_E_INVALIDITEMID) |


**Invariants:**
*   Pure function — no side effects, no I/O, no panics.
*   Pattern matching is case-sensitive on the hex string.

---

##### `fn format_hresult(hr: windows::core::HRESULT) -> String`

**Description:** Formats a COM `HRESULT` for user-facing error messages, appending a friendly hint if one is mapped.

**Inputs:** A `windows::core::HRESULT` (passed by value).
**Output:** Format `0xHHHHHHHH: <hint>` if a hint exists, otherwise just `0xHHHHHHHH`.

**Invariants:**
*   Returns a consistently formatted upper-case hex string.

---

##### `fn log_opc_error(error: &OpcError, operation: &str)`

**Description:** Emits a structured `tracing::error!` event with machine-parseable fields for Windows COM/OPC errors.

**Inputs:** An `OpcError` reference, and the operation name slice (`operation`).

**Invariants:**
*   Purely side-effecting (writes to logs).
*   Extracts raw HRESULT code and friendly hint if available, logging them as structured named fields.

---

#### Internal API (crate-visible only, documented for completeness)


| Function | Signature | Purpose |
| :--- | :--- | :--- |
| `guid_to_progid` | `fn(guid: &GUID) -> Result<String>` | Converts a COM GUID to its registered ProgID string. |
| `variant_to_string` | `fn(variant: &VARIANT) -> String` | Formats a COM VARIANT as a display string. Handles VT_EMPTY, VT_NULL, VT_I2, VT_I4, VT_R4, VT_R8, VT_CY, VT_DATE, VT_BSTR, VT_ERROR, VT_BOOL, VT_I1, VT_UI1, VT_UI2, VT_UI4, VT_I8, VT_UI8, and VT_ARRAY composites. |
| `quality_to_string` | `fn(quality: u16) -> String` | Maps OPC quality bitmask to `"Good"` / `"Bad"` / `"Uncertain"`. |
| `filetime_to_string` | `fn(ft: &FILETIME) -> String` | Converts Win32 FILETIME to local `YYYY-MM-DD HH:MM:SS` string. |
| `opc_value_to_variant` | `fn(value: &OpcValue) -> VARIANT` | Converts an `OpcValue` to a COM `VARIANT`. |


---

### 1.3 `com::client` — Default OPC DA Client

**Purpose:** Concrete `OpcProvider` implementation backed by the consolidated `com` subsystem. Handles COM MTA initialization, server connection, namespace browsing, and synchronous I/O reads.

> [!NOTE]
> Only compiled when feature `opc-da-backend` is enabled (default).

#### Public API

##### `struct OpcDaClient<C = ComConnector>`

| Method | Signature | Description |
| :--- | :--- | :--- |
| `new(connector: C)` | `fn new(connector: C) -> OpcResult<Self>` | Constructs a new wrapper, launching the dedicated COM worker thread. |
| `default()` | `fn default() -> Self` | Constructs an `OpcDaClient<ComConnector>` with default native COM connector. |

Implements `OpcProvider` for all four trait methods by dispatching to the `ComWorker`.

**Invariants:**
*   All COM work runs on a dedicated, long-lived `ComWorker` thread, avoiding repeated initialization overhead and solving COM thread-affinity constraints.
*   Connections are pooled and cached automatically inside the worker, mapped by ProgID.
*   Stale connections are transparently evicted and retried during request dispatch.
*   GUID filtering: zeroed GUIDs are skipped during server enumeration.
*   Server list is sorted and deduplicated before returning.
*   OPC groups created by `read_tag_values` and `write_tag_value` are **always** removed via `remove_group` — even on error paths — to prevent resource leaks.

#### Internal: `browse_recursive`

**Signature:**
```rust
fn browse_recursive<S: ConnectedServer>(
    server: &S,
    tags: &mut Vec<String>,
    max_tags: usize,
    progress: &Arc<AtomicUsize>,
    tags_sink: &Arc<Mutex<Vec<String>>>,
    depth: usize,
) -> Result<()>
```

**Behavior:**
1.  Terminates if `depth > 50` (MAX_DEPTH) or `tags.len() >= max_tags`.
2.  Enumerates `OPC_BRANCH` items using type-safe `BrowseType::Branch`, descends into each via `change_browse_position(BrowseDirection::Down, ...)`.
3.  **Always** navigates back `BrowseDirection::Up` after recursing — even if recursion itself fails — to prevent position corruption. Failure to navigate `Up` is a hard error.
4.  Enumerates `OPC_LEAF` items using `BrowseType::Leaf` (soft-fail: errors logged and skipped).
5.  Converts browse names to fully-qualified item IDs via `get_item_id()`; falls back to browse name on failure.
6.  Each discovered tag is pushed to both `tags` and `tags_sink`, and `progress` is incremented.

#### Internal: OPC_FLAT Fast Path

Before calling `browse_recursive`, `browse_tags` attempts `browse_opc_item_ids(BrowseType::Flat, ...)` at root. If the server returns items, they are collected directly as fully-qualified IDs — skipping recursion and `get_item_id()` entirely. Falls back to `browse_recursive` on error, empty results, or first-item failure.

---

### 1.4 `com::guard` — RAII COM Initialization

**Purpose:** Provide a drop guard that ensures `CoUninitialize` is called exactly once per successful `CoInitializeEx`, even on early returns or panics.

#### Internal API

##### `struct ComGuard`

| Method | Signature | Description |
| :--- | :--- | :--- |
| `new()` | `fn new() -> OpcResult<Self>` | Initialize COM in Multi-Threaded Apartment (MTA) mode. Returns `Ok` on success or if already initialized (`S_FALSE`). |

**Drop behavior:** Calls `CoUninitialize` only if `CoInitializeEx` returned `Ok`.

**Error Conditions:**

| Error | Meaning |
| :--- | :--- |
| Fatal HRESULT from `CoInitializeEx` | Windows COM subsystem is unavailable or misconfigured. |

**Invariants:**
*   Must be used on the **same thread** that called `new()`.
*   `S_FALSE` (already initialized) is treated as success — the guard will still call `CoUninitialize` on drop.
*   The guard is **not** `Send` or `Sync` — it must remain on the thread that created it.

---

### 1.5 `types` — Canonical Protocol Types & Handles

**Purpose:** Provide canonical data structures and newtypes representing OPC DA concepts:

#### Public API

- `GroupHandle(pub u32)`: Opaque, type-safe newtype wrapper for server/client group handles.
- `ItemHandle(pub u32)`: Opaque, type-safe newtype wrapper for server/client item handles.
- `OpcQuality`: Fully decomposed, zero-allocation 16-bit OPC DA quality word (`major: QualityMajor`, `substatus: QualitySubstatus`, `limit: QualityLimit`, `raw: u16`). Implements `From<u16>`, `From<OpcQuality> for u16`, `Display` (rich human-readable diagnostics), `From<&str>`, and predicates (`is_good`, `is_bad`, `is_uncertain`, `is_limited`).
- `QualityMajor`: Major OPC DA quality status (`Good`, `Bad`, `Uncertain`, `Unknown(u8)`).
- `QualitySubstatus`: Detailed substatus reason code (all OPC DA 2.05a codes: `NonSpecific`, `ConfigurationError`, `NotConnected`, `DeviceFailure`, `SensorFailure`, `LastKnownValue`, `CommFailure`, `OutOfService`, `WaitingForInitialData`, `LastUsableValue`, `SensorCalNeeded`, `EguExceeded`, `SubNormal`, `LocalOverride`, and `Raw(u8)`).
- `QualityLimit`: Limit conditions on the tag value (`NotLimited`, `LowLimited`, `HighLimited`, `Constant`).
- `BrowseType`: Strongly-typed enum for namespace browsing (`Branch = 1`, `Leaf = 2`, `Flat = 3`). Implements zero-cost `From<BrowseType> for u32` and fallible `TryFrom<u32> for BrowseType`.
- `BrowseDirection`: Strongly-typed enum for address space cursor movement (`Up = 1`, `Down = 2`, `To = 3`). Implements zero-cost `From<BrowseDirection> for u32` and fallible `TryFrom<u32> for BrowseDirection`.
- `ServerStatus` / `ServerState`: Detailed server run-state and diagnostic types.
- `GroupState`: Metadata bounding an OPC group object.

---

### 1.6 `errors` — Canonical Error Subsystem

**Purpose:** Domain-specific error enumeration (`OpcError`) and `OpcResult<T>` type alias, replacing ad-hoc errors across the crate:

- `OpcError::Com { source: windows::core::Error }`: Propagated Windows COM failure with HRESULT.
- `OpcError::ConnectionFailed(String)`: Target host/server connection failure.
- `OpcError::ServerNotFound(String)`: Unregistered or missing ProgID.
- `OpcError::TagNotFound(String)`: Item ID missing from address space.
- `OpcError::InvalidState(String)`: Invalid operation sequence or unexpected server state.
- `OpcError::Conversion(String)`: Data type conversion failure.
- `OpcError::Internal(String)`: Channel, worker thread, or internal invariant failure.
- `OpcError::NotImplemented(String)`: Unsupported optional COM interface.

---

### 1.7 `com::connector` — Pure-Rust Connector Facade & Reusable Mocks

**Purpose:** Pure-Rust facade traits and DTOs that completely decouple `ComWorker` and consumers from raw Win32 COM / FFI types:
- `ServerConnector`: Discovers servers via `enumerate_servers()` and connects via `connect()`. Implemented by `ComConnector` and `MockServerConnector`.
- `ConnectedServer`: Introspects server namespace and adds/removes groups using `GroupConfig` and `CreatedGroup`. Implemented by `ComServer` and `MockConnectedServer`.
- `ConnectedGroup`: Pure-Rust facade over OPC DA groups:
  - `add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>>`
  - `read(&self, source: DataSource, server_handles: &[ItemHandle]) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>`
  - `write(&self, server_handles: &[ItemHandle], values: &[OpcValue]) -> OpcResult<Vec<Result<(), OpcError>>>`
- `MockConnectedGroup` & `MockConnectedServer`: Reusable pure-Rust mocks (under `#[cfg(test)]`) supporting pluggable closures without native COM allocators or unsafe blocks.

---

### 1.8 `raw` — Crate-Internal Low-Level Win32/COM FFI Subsystem

**Purpose:** Strict crate-internal isolation (`pub(crate) mod raw;`) for all raw Win32 bindings and FFI memory management:
- `raw::bindings`: Autogenerated Win32 COM bindings (`da`, `comn`).
- `raw::memory`: Unsafe memory wrappers (`RemoteArray`, `RemotePointer`, `LocalPointer`) managing `CoTaskMemAlloc` / `CoTaskMemFree`.
- `raw::bridge`: Dormant COM bridge structures (`ItemDef`, `ItemState`, etc.) preserved for binary compatibility and low-level Win32 conversions.
- **Invariant:** `raw` types must NEVER leak into the public API or domain types (`types.rs`).

---

## 2. Data Models

### `TagValue`

Defined in § 1.1. See table above.

### Feature Flags

| Flag | Default | Effect |
| :--- | :--- | :--- |
| `opc-da-backend` | ✅ Yes | Compiles the `com` subsystem module and exports `OpcDaClient` and `ComConnector`. |
| `test-support` | ❌ No | Enables `mockall` and exports `MockOpcProvider`. |
| `dev-diagnostics` | ❌ No | Compiles verbose TRACE-level operation argument dumps into backend methods. |

---

## 3. Integration Points

### 3.1 Internal: `com` Subsystem

**Boundary:** `OpcDaClient` → `com::worker::ComWorker` → `com::connector::ComServer` / `ComGroup`.

| Operation | COM Subsystem API Used | Underlying Windows COM Interface |
| :--- | :--- | :--- |
| Server enumeration | `ComConnector.enumerate_servers()` | `IOPCServerList::EnumClassesOfCategories` |
| Server connection | `ComConnector.connect()` | `CoCreateInstance::<IOPCServer>` |
| Namespace detection | `ComServer.query_organization()` | `IOPCBrowseServerAddressSpace::QueryOrganization` |
| Tag browsing | `ComServer.browse_opc_item_ids()`, `change_browse_position()`, `get_item_id()` | `IOPCBrowseServerAddressSpace` |
| Tag reading | `ComServer.add_group()`, group `read()`, `remove_group()` | `IOPCServer`, `IOPCItemMgt`, `IOPCSyncIO` |
| Tag writing | `ComServer.add_group()`, group `write()`, `remove_group()` | `IOPCServer`, `IOPCItemMgt`, `IOPCSyncIO` |
| String iteration | `StringIterator::new()` | `IEnumString::Next` |

**Error Handling at Boundary:**
*   All COM errors return canonical `OpcError::Com { source }`.
*   Friendly hints (`friendly_com_hint`) and formatted HRESULTs (`format_hresult`) are available for error reporting.
*   `E_POINTER` errors from `StringIterator` are handled internally by the iterator (null-PWSTR skip + `debug!` log).

**Known Upstream Bugs:**

| ID | Bug | Workaround |
| :--- | :--- | :--- |
| OPC-BUG-001 | `StringIterator` produces 16 phantom `E_POINTER` errors per iterator | **FIXED**: cache zeroing + null-PWSTR skip in `StringIterator::next()` |

### 3.2 Downstream: `opc-cli` (Consumer)

**Boundary:** `opc-cli` → `dyn OpcProvider`.

*   The CLI crate depends on the `OpcProvider` trait, never on `OpcDaClient` directly in its core logic.
*   Tests use `MockOpcProvider` (via `test-support` feature).
*   `friendly_com_hint()` is called by the CLI to enrich error messages displayed in the TUI status bar.

---

## 4. Required Test Coverage

### Unit Tests (in `helpers.rs`)

- [x] `friendly_com_hint` returns correct hint for known HRESULT codes.
- [x] `friendly_com_hint` returns `None` for unknown errors.
- [x] `format_hresult` returns `0xHHHHHHHH: <hint>` for known codes.
- [x] `format_hresult` returns `0xHHHHHHHH` for unknown codes.
- [x] `filetime_to_string` returns `"N/A"` for zero FILETIME.
- [x] `filetime_to_string` produces valid date string for non-zero FILETIME.
- [x] `StringIterator` skips null PWSTR entries without producing `E_POINTER`.
- [x] `StringIterator` handles empty enumeration (0 items).
- [x] `opc_value_to_variant` correctly converts `Int` variant.
- [x] `variant_to_string` roundtrips through `VT_I4` and `VT_R4`.
- [x] `variant_to_string` handles `VT_EMPTY` and `VT_NULL`.
- [x] `variant_to_string` handles `VT_CY` (currency).
- [x] `variant_to_string` handles `VT_ERROR` with known and unknown HRESULTs.
- [x] `variant_to_string` returns `(VT ...)` for unknown variant types.
- [x] `quality_to_string` returns `"Good"` for `0xC0`.
- [x] `quality_to_string` returns `"Bad"` for `0x00`.
- [x] `quality_to_string` returns `"Uncertain"` for `0x40`.
- [x] `quality_to_string` returns `"Unknown(…)"` for unrecognized bitmask.

### ComWorker Thread Dispatch Unit Tests (in `com/worker.rs`)

- [x] `test_worker_starts_and_stops` — worker thread start & stop.
- [x] `test_worker_list_servers` — server listing dispatch.
- [x] `test_worker_write_tag_value` — write path dispatch & `WriteResult`.
- [x] `test_connection_cache_reuse` — server connection pooling across requests (`connect_count == 1`).
- [x] `test_stale_connection_eviction` — auto-eviction & transparent reconnect on COM/RPC error (`connect_count == 2`).
- [x] `test_worker_panic_propagation` — worker thread panic safety & error propagation to caller.
- [x] `test_drop_during_active_request` — graceful worker thread shutdown.
- [x] `test_worker_init_failure` — initialization error handling.
- [x] `test_worker_read_tag_values_mismatched_lengths` — error resilience on uneven responses.
- [x] `test_worker_read_tag_values_quality_decoding` — mock-driven integration test verifying end-to-end multi-quality decoding and item rejection error mapping.

### Type-Safe Enum Unit Tests (in `types.rs`)

- [x] `browse_type_from_roundtrip` — validates `From<BrowseType> for u32` matches expected raw integers (1, 2, 3).
- [x] `browse_type_try_from_rejects_invalid` — validates `TryFrom<u32> for BrowseType` rejects 0, 4, 99.
- [x] `browse_direction_from_roundtrip` — validates `From<BrowseDirection> for u32` matches expected raw integers (1, 2, 3).
- [x] `browse_direction_try_from_rejects_invalid` — validates `TryFrom<u32> for BrowseDirection` rejects 0, 4, 99.
- [x] `test_opc_quality_good_standard` — validates standard Good (0x00C0) decoding and predicates.
- [x] `test_opc_quality_good_local_override` — validates Good with Local Override (0x00D8) decoding and Display.
- [x] `test_opc_quality_bad_comm_failure` — validates Bad with Comm Failure (0x0018) decoding and Display.
- [x] `test_opc_quality_uncertain_limits` — validates Uncertain with EGU Exceeded & High Limited (0x0056) decoding and Display.
- [x] `test_opc_quality_roundtrip_u16` — validates lossless roundtripping between u16 and OpcQuality.
- [x] `test_opc_quality_from_str` — validates string conversion helpers.

### Mock-Based Tests (in `opc-cli`)

- [x] `MockOpcProvider` returns expected server list.
- [x] `MockOpcProvider` returns expected browse results.
- [x] `MockOpcProvider` returns expected tag values.
- [x] `MockOpcProvider` simulates error conditions for UI error handling.

### Doc Tests

- [x] `friendly_com_hint` — runnable doctest in `helpers.rs`.
- [x] `ComGuard` — internal-only ignored doctest in `com/guard.rs`.

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

