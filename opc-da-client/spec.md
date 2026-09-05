# 📋 spec.md — opc-da-client

> **Behavioral Source of Truth** for the `opc-da-client` library crate.
> Defines *what* each module should do — independent of current implementation.
>
> Last verified against: eaba614

---

## 1. Module / Component Contracts

### 1.1 `provider` — Core Trait & Data Types

**Purpose:** Define the async trait that all OPC DA backends must implement, plus the canonical data model for tag values.

#### Public API

##### `trait OpcProvider: Send + Sync`

All methods use `#[async_trait]`.

| Method | Signature | Description |
| :--- | :--- | :--- |
| `list_servers` | `async fn list_servers(&self, host: &str) -> OpcResult<Vec<String>>` | Enumerate OPC DA servers available on `host`. |
| `list_server_details` | `async fn list_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>>` | Enumerate OPC DA servers on `host` with rich metadata (`ProgID`, `CLSID`, user-readable name). Default implementation synthesizes records wrapping `list_servers`. |
| `browse_tags` | `async fn browse_tags(&self, server: &str, collector: TagCollector) -> OpcResult<Vec<String>>` | Recursively discover tags on `server`, pushing each to `collector` as found. |
| `read_tag_values` | `async fn read_tag_values(&self, server: &str, tag_ids: Vec<String>) -> OpcResult<Vec<TagValue>>` | Read current value, quality, and timestamp for the given tag IDs. |
| `write_tag_value` | `async fn write_tag_value(&self, server: &str, tag_id: &str, value: OpcValue) -> OpcResult<WriteResult>` | Write a typed value to a single tag on `server`. |

**Error Conditions:**

| Method | Error Condition | Meaning |
| :--- | :--- | :--- |
| `list_servers` | COM init failure | Windows COM subsystem unavailable. |
| `list_servers` | Registry enumeration failure | OPC Core Components not installed or registry corrupt. |
| `list_server_details` | COM init / catalog failure | Same as `list_servers`. |
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
*   `list_server_details` default implementation synthesizes `OpcServerInfo` with `GUID::zeroed()` and `user_type: None`, ensuring full backward compatibility.
*   `browse_tags` **never** collects more than `collector.max_tags()` items.
*   `browse_tags` pushes tags to `collector` incrementally; on timeout the caller can harvest partial results.
*   `browse_tags` updates `collector` length atomically and lock-free for each discovered tag.
*   `read_tag_values` returns a `TagValue` entry for all requested tags, preserving the original array length and order. Items that fail to be added to the group receive quality `OpcQuality::BAD_CONFIG_ERROR` with `value: None` and `timestamp: None`, and items that fail reading receive `OpcQuality::BAD_COMM_FAILURE` with `value: None` and `timestamp: None`.
*   `write_tag_value` returns `Ok(WriteResult)` in all non-fatal cases; per-tag success or error is reported inside `WriteResult.status` as a strongly-typed `Result<(), OpcError>`.


---

##### `struct TagValue`

**Purpose:** Canonical representation of a single OPC DA tag read result.

| Field | Type | Required | Description | Constraints |
| :--- | :--- | :--- | :--- | :--- |
| `tag_id` | `String` | Yes | Fully qualified tag identifier. | Non-empty. |
| `value` | `Option<OpcValue>` | Yes | Decoded typed value, or `None` on read failure. | `display_value()` formats to string (`"Error"` if `None`). |
| `quality` | `OpcQuality` | Yes | Decomposed 16-bit OPC DA quality word. | `Copy`, `Display` formats rich human-readable status. |
| `timestamp` | `Option<std::time::SystemTime>` | Yes | Last-change timestamp (UTC-based), or `None`. | `formatted_timestamp()` formats to local time string. |

**Methods & Traits:**
* `display_value(&self) -> String`: Returns formatted value string or `"Error"`.
* `formatted_timestamp(&self) -> String`: Returns local formatted time string or `"N/A"`.
* `is_good(&self) -> bool`: Returns `true` if quality is good and value is present.
* `is_error(&self) -> bool`: Returns `true` if quality is bad or value is absent.
* `Display`: Canonical formatting rendering `"{tag_id} = {value} [{quality}] @ {timestamp}"`.

**Derives:** `Debug`, `Clone`, `PartialEq`.

---

##### `Display Adapters & Extension Traits`

**Purpose:** Provide zero-allocation, ergonomic formatting helpers for destructured or standalone option fields.

| Type / Trait | Target | Default Fallback | Description |
| :--- | :--- | :--- | :--- |
| `struct DisplayOptionOpcValue<'a>` | `Option<&'a OpcValue>` | Custom | Zero-allocation `Display` adapter streaming inner value or fallback directly into formatter. |
| `struct DisplayOptionTimestamp<'a>` | `Option<SystemTime>` | Custom | Zero-allocation `Display` adapter streaming local datetime or fallback directly into formatter. |
| `trait OpcValueOptionExt` | `Option<OpcValue>`, `Option<&OpcValue>` | `"Error"` | Extends options with `.display_or(fallback)` and `.display()`. |
| `trait SystemTimeOptionExt` | `Option<SystemTime>` | `"N/A"` | Extends timestamp options with `.display_or(fallback)` and `.display()`. |

---

##### `enum OpcValue`

**Purpose:** Typed representation of a value to be written to or read from an OPC DA tag.

| Variant | Data Type | Description | COM VT Type |
| :--- | :--- | :--- | :--- |
| `String(String)` | `String` | Raw string value. | `VT_BSTR` |
| `Int(i32)` | `i32` | 32-bit signed integer. | `VT_I4` |
| `Float(f64)` | `f64` | 64-bit float. | `VT_R8` |
| `Bool(bool)` | `bool` | Boolean value. | `VT_BOOL` |
| `Empty` | N/A | Empty variant (uninitialized). | `VT_EMPTY` |
| `Null` | N/A | Explicitly null variant. | `VT_NULL` |

**Derives:** `Debug`, `Clone`, `PartialEq`.

---

##### `struct WriteResult`

**Purpose:** Canonical representation of an OPC DA tag write result.

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `tag_id` | `String` | Yes | The tag identifier that was written to. |
| `status` | `Result<(), OpcError>` | Yes | Outcome of the write operation: `Ok(())` on success, or `Err(OpcError)` on failure. |

**Methods:**
* `success(tag_id)`: Constructs a successful write result.
* `failure(tag_id, error)`: Constructs a failed write result with domain error.
* `is_success()`: Returns `true` if write succeeded.
* `is_error()`: Returns `true` if write failed.
* `error()`: Returns `Option<&OpcError>`.

**Derives:** `Debug`, `Clone`, `PartialEq`.


##### `struct TagCollector`

**Purpose:** Thread-safe, bounded container encapsulating thread-safe tag accumulation, lock-free progress reporting, and cooperative cancellation token for OPC tag browsing.

| Method | Signature | Description |
| :--- | :--- | :--- |
| `new` | `pub fn new(max_tags: usize) -> Self` | Creates a collector bounded by `max_tags` (clamped to `[1, MAX_CAPACITY]`). |
| `unbounded` | `pub fn unbounded() -> Self` | Creates an unbounded collector (`max_tags = usize::MAX`). |
| `max_tags` | `pub fn max_tags(&self) -> usize` | Returns the configured capacity bound. |
| `len` | `pub fn len(&self) -> usize` | Returns the current count of collected tags without locking. |
| `is_empty` | `pub fn is_empty(&self) -> bool` | Returns `true` if no tags have been collected. |
| `is_full` | `pub fn is_full(&self) -> bool` | Returns `true` if current count has reached or exceeded `max_tags`. |
| `cancel` | `pub fn cancel(&self)` | Signals cooperative cancellation token across all clones. |
| `is_cancelled` | `pub fn is_cancelled(&self) -> bool` | Checks if cancellation has been signalled. |
| `snapshot` | `pub fn snapshot(&self) -> Vec<String>` | Returns a cloned copy of currently collected tags under lock. |
| `harvest` | `pub fn harvest(&self) -> Vec<String>` | Drains and returns all collected tags, resetting count to 0. |
| `push` | `pub fn push(&self, tag: String) -> bool` | Pushes a tag into collector if not full or cancelled. Returns `true` if added. |

**Invariants:**
* `Clone` performs a shallow reference-counted clone sharing the inner synchronization state.
* `len()` and `is_cancelled()` are completely lock-free (`AtomicUsize` and `AtomicBool`).
* `push()` will reject additions once `max_tags` is reached or if cancelled.

**Derives:** `Debug`, `Clone`, `Default`.

---

##### `MockOpcProvider` *(feature = `test-support`)*

**Purpose:** Auto-generated mock of `OpcProvider` via `mockall`, exported when the `test-support` feature is enabled.

**Invariants:**
*   Provides `expect_*` methods for each trait method.
*   Must be fully compatible with `#[tokio::test]` async test harnesses.

---

##### `enum ServerIdentifier`

**Purpose:** Strongly-typed identifier referencing an OPC DA server either by its Programmatic Identifier (`ProgID`) or directly by its 128-bit COM Class ID (`CLSID`).

| Variant | Inner Type | Description |
| :--- | :--- | :--- |
| `ProgId(String)` | `String` | Human-readable Programmatic Identifier (e.g. `"Matrikon.OPC.Simulation.1"`). |
| `Clsid(windows::core::GUID)` | `GUID` | Direct 128-bit Windows COM Class ID. |

**Conversions & Methods:**
* `is_prog_id(&self) -> bool`: Returns `true` if this is a ProgID variant.
* `is_clsid(&self) -> bool`: Returns `true` if this is a CLSID variant.
* `From<&str>` and `From<String>`: Automatically checks if the string matches 128-bit GUID hex syntax (with or without `{}` braces). If valid GUID syntax, coerces directly into `ServerIdentifier::Clsid`; otherwise stores as `ServerIdentifier::ProgId`.
* `From<windows::core::GUID>`: Converts directly to `ServerIdentifier::Clsid`.
* `Display`: Formats `ProgId` as string literal; formats `Clsid` as canonical `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.

**Derives:** `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`.

---

##### `struct OpcServerInfo`

**Purpose:** Canonical structured record for an enumerated OPC DA server with rich catalog metadata.

| Field | Type | Description |
| :--- | :--- | :--- |
| `prog_id` | `String` | Programmatic Identifier of the server. |
| `clsid` | `windows::core::GUID` | 128-bit COM Class ID. |
| `user_type` | `Option<String>` | Human-readable server title/description from catalog metadata, or `None` if unassigned. |
| `host` | `Option<String>` | Target host machine (`None` for localhost). |

**Methods:**
* `display_name(&self) -> &str`: Returns `user_type` if present and non-empty, otherwise falls back to `prog_id`.
* `endpoint(&self) -> OpcServerEndpoint`: Builds an `OpcServerEndpoint` targeting this server.

**Derives:** `Debug`, `Clone`, `PartialEq`, `Eq`.

---

##### `struct OpcServerEndpoint`

**Purpose:** Connection endpoint binding an optional host machine to a `ServerIdentifier`.

| Field | Type | Description |
| :--- | :--- | :--- |
| `host` | `Option<String>` | Target machine hostname or IP address (`None` for localhost). |
| `identifier` | `ServerIdentifier` | Strongly-typed server identifier (ProgID or direct CLSID). |

**Methods:**
* `local(identifier: impl Into<ServerIdentifier>) -> Self`: Creates local endpoint.
* `remote(host: impl Into<String>, identifier: impl Into<ServerIdentifier>) -> Self`: Creates remote endpoint.
* `Display`: Formats as `"{host}/{identifier}"` or `"{identifier}"`.

**Derives:** `Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`.

---

### 1.2 `errors` — Canonical Error Handling & Hints

**Purpose:** Define domain-specific error types (`OpcError`), result alias (`OpcResult<T>`), and inherent diagnostics.

#### Public API

##### `OpcError::friendly_hint(&self) -> Option<&'static str>`

**Description:** Inspects the `OpcError` instance for underlying Win32 COM/OPC HRESULT failure codes and returns an actionable plain-English diagnostic hint.

**Inputs:** Receiver `&self` on `OpcError`.
**Output:** `Some(&'static str)` if the variant is `OpcError::Com` and matches a recognized HRESULT; `None` otherwise.

**Known Mappings:**

| HRESULT | Symbolic Name | Hint |
| :--- | :--- | :--- |
| `0x80040112` | `CLASS_E_NOTLICENSED` | Server license does not permit OPC client connections |
| `0x80080005` | `CO_E_SERVER_EXEC_FAILURE` | Server process failed to start — check if it is installed and running |
| `0x80070005` | `E_ACCESSDENIED` | Access denied — DCOM launch/activation permissions not configured for this user |
| `0x800706BA` | `RPC_S_SERVER_UNAVAILABLE` | RPC server unavailable — the target host may be offline or blocking RPC |
| `0x800706BE` | `RPC_S_CALL_FAILED` | RPC call failed — network or remote server connection dropped |
| `0x800706BF` | `RPC_S_SERVER_TOO_BUSY` | RPC server is too busy to complete this operation |
| `0x800706F4` | `RPC_S_CALL_FAILED_DNE` | COM marshalling error — try restarting the OPC server |
| `0x80040154` | `REGDB_E_CLASSNOTREG` | Server is not registered on this machine |
| `0x80004003` | `E_POINTER` | Invalid pointer (E_POINTER) |
| `0xC0040004` | `OPC_E_BADRIGHTS` | Server rejected write — the item may be read-only (OPC_E_BADRIGHTS) |
| `0xC0040006` | `OPC_E_BADTYPE` | Data type mismatch — server cannot convert the written value (OPC_E_BADTYPE) |
| `0xC0040007` | `OPC_E_UNKNOWNITEMID` | Item ID not found in server address space (OPC_E_UNKNOWNITEMID) |
| `0xC0040008` | `OPC_E_INVALIDITEMID` | Item ID syntax is invalid for this server (OPC_E_INVALIDITEMID) |

**Invariants:**
*   Pure method — no side effects, no I/O, no panics.
*   Returns `None` for all non-`Com` variants (`ConnectFailed`, `GroupAddFailed`, `ItemAddFailed`, `TypeMismatch`, `Internal`).

##### `OpcError::connection_failed(server: impl std::fmt::Display, err: impl std::fmt::Display) -> Self`

**Description:** Constructs an [`OpcError::Connection`] variant indicating failure to resolve a server name or ProgID string to a CLSID.

**Inputs:** `server` (server ProgID / identifier), `err` (underlying failure or HRESULT).
**Output:** `OpcError::Connection(format!("Failed to resolve ProgID '{server}' to CLSID: {err}"))`.

##### `Standard Error Conversions (From implementations)`

`OpcError` implements `From` for standard synchronization and channel failure types to allow native `?` error propagation without manual `map_err`:

| Source Error | Target Variant | Formatted Message |
| :--- | :--- | :--- |
| `std::sync::mpsc::RecvError` | `OpcError::Internal` | `"COM worker init channel disconnected: {err}"` |
| `tokio::sync::oneshot::error::RecvError` | `OpcError::Internal` | `"COM worker shut down during request: {err}"` |
| `tokio::sync::mpsc::error::SendError<T>` | `OpcError::Internal` | `"COM worker channel closed (worker stopped): {err}"` |
| `std::sync::PoisonError<T>` | `OpcError::Internal` | `"Synchronization lock poisoned: {err}"` |

---

#### Internal Utilities (crate-visible only, documented for completeness)

##### `com::variant` Module

| Function | Signature | Purpose |
| :--- | :--- | :--- |
| `variant_to_string` | `fn(variant: &VARIANT) -> String` | Formats a COM VARIANT as a display string. Handles VT_EMPTY, VT_NULL, VT_I2, VT_I4, VT_R4, VT_R8, VT_CY, VT_DATE, VT_BSTR, VT_ERROR, VT_BOOL, VT_I1, VT_UI1, VT_UI2, VT_UI4, VT_I8, VT_UI8, and VT_ARRAY composites. |
| `variant_to_opc_value` | `fn(variant: &VARIANT) -> OpcValue` | Converts a COM `VARIANT` into a strongly-typed domain `OpcValue`. |
| `opc_value_to_variant` | `fn(value: &OpcValue) -> VARIANT` | Converts a domain `OpcValue` to a COM `VARIANT`. |

##### `raw::hresult` Module

| Function / Constant | Signature | Purpose |
| :--- | :--- | :--- |
| `friendly_hresult_hint` | `fn(hr: HRESULT) -> Option<&'static str>` | Maps Win32 COM `HRESULT` to diagnostic hint. |
| `format_hresult` | `fn(hr: HRESULT) -> String` | Formats `HRESULT` as `0xHHHHHHHH: <hint>` or `0xHHHHHHHH`. |
| `is_connection_hresult` | `fn(hr: HRESULT) -> bool` | Detects connection/transport drops (`RPC_S_*`, `CO_E_SERVER_EXEC_FAILURE`). |

##### `errors` Telemetry Module

| Item | Signature / Type | Purpose |
| :--- | :--- | :--- |
| `enum OpcOperation` | `pub(crate) enum OpcOperation` | Canonical strongly-typed operation identifiers for all OPC DA client operations (`Connect`, `ListServers`, `ListServerDetails`, `InspectRegistration`, `BrowseTags`, `ReadTagValues`, `WriteTagValue`, `DispatchOperation`, `DispatchConnectionError`, `DispatchReconnect`, `DispatchRetriedOperation`). Implements `Display` formatting to snake_case/kebab keys. |
| `log_opc_err!` | `macro_rules! log_opc_err` | Emits a consolidated, structured `tracing::error!` event containing `operation`, `hresult`, `hint`, `chain`, and arbitrary contextual fields (`server`, `tag`, `value`, `depth`, `branch`, `host`, `clsid`). Eliminates duplicate double logging. |
| `log_opc_error` | `fn(error: &OpcError, operation: &str)` | Legacy compatibility wrapper delegating directly to `log_opc_err!`. |

---

### 1.3 `com::client` & `com::worker` — Default OPC DA Client & Apartment Worker

**Purpose:** Concrete `OpcProvider` implementation backed by the consolidated `com` subsystem. Handles COM MTA initialization, server connection, namespace browsing, structured catalog discovery, and synchronous I/O reads.

> [!NOTE]
> Only compiled when feature `opc-da-backend` is enabled (default).

#### Public API

##### `struct OpcDaClient<C = ComConnector>`

| Method | Signature | Description |
| :--- | :--- | :--- |
| `new(connector: C)` | `fn new(connector: C) -> OpcResult<Self>` | Constructs a new wrapper, launching the dedicated COM worker thread. |
| `default()` | `fn default() -> Self` | Constructs an `OpcDaClient<ComConnector>` with default native COM connector. |

Implements `OpcProvider` for all five trait methods (`list_servers`, `list_server_details`, `browse_tags`, `read_tag_values`, `write_tag_value`) by dispatching to the `ComWorker`.

**Invariants:**
*   All COM work runs on a dedicated, long-lived `ComWorker` thread, avoiding repeated initialization overhead and solving COM thread-affinity constraints.
*   Connections are pooled and cached automatically inside the worker, keyed by `ServerIdentifier` (supporting both ProgID and direct CLSID caching).
*   Stale connections are transparently evicted and retried during request dispatch.
*   GUID filtering: zeroed GUIDs are skipped during server enumeration.
*   OPC groups created by `read_tag_values` and `write_tag_value` are **always** managed by `GroupGuard`, guaranteeing deterministic invocation of `remove_group(handle, true)` on `Drop` across all return paths, early returns with `?`, and thread panics.

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
) -> OpcResult<()>
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

##### `struct GroupGuard<'a, S: ConnectedServer>` (Internal RAII Cleanup)

**Purpose:** Provide an automatic RAII drop guard for temporary COM groups created during `read_tag_values` and `write_tag_value` executions on the `ComWorker` thread.

| Method | Signature | Description |
| :--- | :--- | :--- |
| `new(server: &'a S, handle: GroupHandle)` | `pub(crate) fn new(server: &'a S, handle: GroupHandle) -> Self` | Wraps server reference and group handle with `disarmed = false`. |
| `handle(&self)` | `pub(crate) fn handle(&self) -> GroupHandle` | Returns the inner group handle. |
| `disarm(&mut self)` | `pub(crate) fn disarm(&mut self)` | Disarms automatic cleanup on drop. |

**Drop behavior:** When dropped, if not disarmed, calls `self.server.remove_group(self.handle, true)`. Any server cleanup errors are logged as warnings without panicking.

**Invariants:**
*   Constructed immediately upon successful `add_group` return.
*   Guarantees group destruction on all function exits (early `?` propagation, empty item slices, error returns, and panics).

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
- `OpcError::Connection(String)`: Target host/server connection failure.
- `OpcError::Server(String, u32)`: Server-specific error reported via status code.
- `OpcError::Conversion(String)`: Data type conversion failure.
- `OpcError::InvalidState(String)`: Invalid operation sequence or unexpected server state.
- `OpcError::NotImplemented(String)`: Unsupported optional COM interface or feature.
- `OpcError::Internal(String)`: Channel, worker thread, or internal invariant failure.

---

### 1.7 `com::discovery` — Catalog Adapter & Registry Inspection

**Purpose:** Provide structured server enumeration and local Windows registry diagnostics:

#### Public API

##### `struct OpcServerRegistration`

**Purpose:** Detailed Windows registry configuration for an installed OPC DA server class.

| Field | Type | Description |
| :--- | :--- | :--- |
| `clsid` | `windows::core::GUID` | 128-bit COM Class ID. |
| `prog_id` | `String` | Programmatic Identifier. |
| `version_independent_prog_id` | `Option<String>` | Version-independent ProgID, or `None` if unassigned. |
| `binary_path` | `std::path::PathBuf` | Resolved executable or DLL file path on disk. |
| `server_type` | `OpcServerType` | Execution model classification (`LocalServer32` vs `InprocServer32`). |

**Derives:** `Debug`, `Clone`, `PartialEq`, `Eq`.

---

##### `enum OpcServerType`

**Purpose:** Execution model classification of an installed COM server.

| Variant | Description |
| :--- | :--- |
| `LocalServer32` | Out-of-process executable server (`.exe`). Formats as `"LocalServer32 (Executable)"`. |
| `InprocServer32` | In-process DLL server (`.dll`). Formats as `"InprocServer32 (DLL)"`. |

**Derives:** `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`. Implements `Display`.

---

##### `inspect_local_registration(clsid: &GUID, host: Option<&str>) -> OpcResult<OpcServerRegistration>`

**Description:** Inspects the local machine Windows registry for an OPC DA server's registration details by querying `HKCR\CLSID\{...}` across both native and 32-bit (`KEY_WOW64_32KEY`) views.

**Inputs:**
* `clsid`: Reference to the 128-bit COM Class ID.
* `host`: Target host machine. If `Some` and not localhost/127.0.0.1, returns [`OpcError::NotImplemented`].

**Returns:**
* `Ok(OpcServerRegistration)` with resolved binary path and server execution type.

**Errors:**
* [`OpcError::NotImplemented`] if `host` is a remote machine.
* [`OpcError::Server`] if the CLSID is missing or neither `LocalServer32` nor `InprocServer32` registry keys exist.

---

#### Internal: `OpcServerListCatalog`

**Purpose:** Adapter combining `IOPCServerList` and `IOPCServerList2`.
* Uses `IOPCServerList::EnumClassesOfCategories` to enumerate category classes, bypassing the `IOPCEnumGUID` vs standard `IEnumGUID` COM vtable layout mismatch.
* Employs a resilient 3-tier fallback to extract server details:
  1. `IOPCServerList2::GetClassDetails` (v2 interface with version-independent ProgID)
  2. `IOPCServerList::GetClassDetails` (v1 interface with ProgID and user type)
  3. `guid_to_progid` fallback (resolving from registry via COM runtime)

---

### 1.8 `com::connector` — Pure-Rust Connector Facade & Reusable Mocks

**Purpose:** Pure-Rust facade traits and DTOs that completely decouple `ComWorker` and consumers from raw Win32 COM / FFI types:
* `ServerConnector`: Discovers servers via `enumerate_servers()` and `enumerate_server_details(host: &str) -> OpcResult<Vec<OpcServerInfo>>`, and connects via `connect(name)` and `connect_identifier(&ServerIdentifier)`. Implemented by `ComConnector` and `MockServerConnector`.
* `ConnectedServer`: Introspects server namespace and adds/removes groups using `GroupConfig` and `CreatedGroup`. Implemented by `ComServer` and `MockConnectedServer`. Supports in-memory tag browsing via `StringIterator::from_vec`.
* `ConnectedGroup`: Pure-Rust facade over OPC DA groups:
  - `add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>>`
  - `read(&self, source: DataSource, server_handles: &[ItemHandle]) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>`
  - `write(&self, server_handles: &[ItemHandle], values: &[OpcValue]) -> OpcResult<Vec<Result<(), OpcError>>>`
* Server connection helpers:
  - `connect_server_identifier(identifier: &ServerIdentifier) -> OpcResult<IOPCServer>`: Directly calls `CoCreateInstance` when `ServerIdentifier::Clsid`, bypassing `CLSIDFromProgID`. When `ServerIdentifier::ProgId`, resolves ProgID to CLSID via registry.
  - `connect_server(server_name: &str) -> OpcResult<IOPCServer>`: Convenience wrapper delegating to `connect_server_identifier(&ServerIdentifier::from(server_name))`.
  - `guid_to_progid(guid: &GUID) -> OpcResult<String>`: Converts a COM GUID to its registered ProgID string using `RemotePointer<u16>::into_string(self)` with guaranteed COM allocator cleanup via RAII on both success and error paths.
* `MockConnectedGroup`, `MockConnectedServer`, and `MockServerConnector`: Reusable pure-Rust mocks (under `#[cfg(any(test, feature = "test-support"))]` and exported at crate root under `test-support`) supporting pluggable closures, failure injection (`MockState`), tracking of cleanup invocations (`MockState::remove_group_count`), simulated structured server details (`server_details: Arc<Mutex<Vec<OpcServerInfo>>>`, `with_server_details`), bidirectional ProgID/detail sync, and simulated tag browsing without native COM allocators or unsafe blocks.

---

### 1.9 `raw` — Crate-Internal Low-Level Win32/COM FFI Subsystem

**Purpose:** Strict crate-internal isolation (`pub(crate) mod raw;`) for all raw Win32 bindings and FFI memory management:
- `raw::bindings`: Autogenerated Win32 COM bindings (`da`, `comn`).
- `raw::memory`: Unsafe memory wrappers (`RemoteArray`, `RemotePointer`, `LocalPointer`) managing `CoTaskMemAlloc` / `CoTaskMemFree`. `RemotePointer` and `RemoteArray` are strictly move-only types (`Clone` prohibited) to prevent double-free heap corruptions on unmanaged memory. `RemotePointer<u16>::into_string(self) -> OpcResult<String>` consumes ownership by value and safely converts null-terminated UTF-16 wide strings into `String`, automatically invoking `CoTaskMemFree` on drop.
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
| `test-support` | ❌ No | Enables `mockall` and exports `MockOpcProvider`, `MockServerConnector`, `MockConnectedServer`, and `MockConnectedGroup`. |
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
| String iteration | `StringIterator::new()` (native COM), `StringIterator::from_vec()` (in-memory simulation) | `IEnumString::Next` (native COM) |

**Error Handling at Boundary:**
*   All COM errors return canonical `OpcError::Com { source }`.
*   Friendly hints (`err.friendly_hint()`) and formatted HRESULTs (`raw::hresult::format_hresult`) are available for error reporting.
*   `E_POINTER` errors from `StringIterator` are handled internally by the iterator (null-PWSTR skip + `debug!` log).

**Known Upstream Bugs:**

| ID | Bug | Workaround |
| :--- | :--- | :--- |
| OPC-BUG-001 | `StringIterator` produces 16 phantom `E_POINTER` errors per iterator | **FIXED**: cache zeroing + null-PWSTR skip in `StringIterator::next()` |

### 3.2 Downstream: `opc-cli` (Consumer)

**Boundary:** `opc-cli` → `dyn OpcProvider`.

*   The CLI crate depends on the `OpcProvider` trait, never on `OpcDaClient` directly in its core logic.
*   Tests use `MockOpcProvider` (via `test-support` feature).
*   `e.friendly_hint()` is called by the CLI to enrich error messages displayed in the TUI status bar.

---

## 4. Required Test Coverage

### Unit Tests (in `errors.rs` & `raw/hresult.rs`)

- [x] `OpcError::friendly_hint` returns correct hint for known HRESULT codes.
- [x] `OpcError::friendly_hint` returns `None` for unknown errors and non-COM error variants.
- [x] `raw::hresult::format_hresult` returns `0xHHHHHHHH: <hint>` for known codes.
- [x] `raw::hresult::format_hresult` returns `0xHHHHHHHH` for unknown codes.
- [x] `raw::hresult::is_connection_hresult` accurately identifies RPC and transport errors.
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
- [x] `test_worker_list_server_details` — structured server listing dispatch & response receiving.
- [x] `test_worker_write_tag_value` — write path dispatch & `WriteResult`.
- [x] `test_worker_write_tag_value_failure` — write rejection error propagation & `OpcError::Com`.
- [x] `test_connection_cache_reuse` — server connection pooling across requests (`connect_count == 1`).
- [x] `test_stale_connection_eviction` — auto-eviction & transparent reconnect on COM/RPC error (`connect_count == 2`).
- [x] `test_worker_panic_propagation` — worker thread panic safety & error propagation to caller.
- [x] `test_drop_during_active_request` — graceful worker thread shutdown.
- [x] `test_worker_init_failure` — initialization error handling.
- [x] `test_worker_read_tag_values_mismatched_lengths` — error resilience on uneven responses.
- [x] `test_worker_read_tag_values_quality_decoding` — mock-driven integration test verifying end-to-end multi-quality decoding and item rejection error mapping.
- [x] `test_group_guard_cleanup_on_drop` — verifies `GroupGuard` automatically invokes `remove_group` upon drop.
- [x] `test_group_guard_disarm_prevents_cleanup` — verifies disarming `GroupGuard` suppresses drop cleanup.
- [x] `test_worker_handle_read_error_cleans_group` — negative unit test asserting group handle cleanup when item addition fails.
- [x] `test_worker_tracing_instrumentation_execution` — verifies worker operations execute successfully under active tracing instrumentation.
- [x] `test_worker_channel_drop_error_propagation` — verifies `ComWorker` returns `OpcError::Internal` when worker request channel receiver is closed.

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
- [x] `test_server_identifier_conversions_and_display` — validates `ServerIdentifier` conversions from `&str`, `String`, `GUID`, GUID hex syntax auto-detection, and `Display` formatting.
- [x] `test_opc_server_info_display_name_and_endpoint` — validates `OpcServerInfo` display name fallback and endpoint generation.

### Provider & TagCollector Unit Tests (in `provider.rs`)

- [x] `test_tag_value_display` — verifies canonical Display implementation formatting.
- [x] `test_tag_value_helpers_success` — verifies `is_good()`, `is_error()`, `display_value()`, and `formatted_timestamp()`.
- [x] `test_tag_value_helpers_failure` — verifies error state predicates and fallback values.
- [x] `test_opc_value_display` — validates Display for all OpcValue variants.
- [x] `test_tag_value_destructuring_ergonomics` — verifies destructuring pattern matching with zero-allocation adapters.
- [x] `test_opc_value_option_ext_some` and `test_opc_value_option_ext_none` — validates `OpcValueOptionExt` display formatting.
- [x] `test_system_time_option_ext_some` and `test_system_time_option_ext_none_and_epoch` — validates `SystemTimeOptionExt` display formatting.
- [x] `test_tag_collector_lifecycle` — verifies initialization, pushing, length tracking, snapshot, and harvest draining.
- [x] `test_tag_collector_capacity_cap` — validates `max_tags` enforcement and overflow push rejection.
- [x] `test_tag_collector_unbounded` — verifies unbounded collector construction and growth.
- [x] `test_tag_collector_cancellation` — validates cooperative cancellation flag and rejection of post-cancellation pushes.
- [x] `test_tag_collector_multithreaded` — validates concurrent multi-threaded push contention and atomic count integrity across 8 threads.
- [x] `test_provider_default_list_server_details` — validates default `list_server_details` synthesis from `list_servers`.

### Connector & Client Unit Tests (in `com/connector.rs` and `com/client.rs`)

- [x] `test_string_iterator_from_vec` — verifies in-memory `StringIterator` collection and equality without COM interfaces.
- [x] `test_mock_connector_browse` — verifies `MockConnectedServer::browse_opc_item_ids` returns in-memory simulated tags.
- [x] `test_mock_group_defaults` and `test_mock_group_custom_handlers` — verifies mock group default results and custom read handlers.
- [x] `test_mock_server_add_group_and_eviction` — verifies group handle generation and connection drop error injection.
- [x] `test_group_item_def_and_state_cloning` — verifies DTO clone and display behavior.
- [x] `test_guid_to_progid_zeroed_guid_returns_com_error` — verifies structured COM error preservation on zeroed GUID.
- [x] `test_mock_server_connector_server_details` — verifies `MockServerConnector::with_server_details` and `enumerate_server_details`.
- [x] `test_client_list_server_details` — verifies `OpcDaClient::list_server_details` dispatch through worker against mock connector.
- [x] `test_worker_browse_tags_success` — verifies `ComWorker` tag discovery over hierarchical namespace using `MockServerConnector`.
- [x] `test_worker_browse_tags_cancelled` — verifies `ComWorker` immediate return when `TagCollector` is cancelled prior to execution.
- [x] `test_worker_browse_tags_capacity_cap` — verifies `ComWorker` tag accumulation halts when `TagCollector` capacity is reached.
- [x] `test_worker_browse_tags_flat_organization` — verifies fast leaf browsing when server namespace organization is flat.

### Discovery & Registry Inspection Unit Tests (in `com/discovery.rs`)

- [x] `test_inspect_local_registration_remote_rejected` — verifies `inspect_local_registration` cleanly rejects remote machine addresses with `OpcError::NotImplemented`.
- [x] `test_sanitize_binary_path_quoted` — verifies `sanitize_binary_path` strips surrounding double quotes from registry image paths.
- [x] `test_sanitize_binary_path_unquoted_with_flag` — verifies `sanitize_binary_path` strips trailing CLI flags (`-Embedding`, `/automation`).
- [x] `test_opc_server_type_display` — verifies `OpcServerType` Display formatting (`LocalServer32 (Executable)` vs `InprocServer32 (DLL)`).

### COM VARIANT Unit Tests (in `com/variant.rs`)

- [x] `test_opc_value_to_variant_int`, `test_opc_value_to_variant_float`, `test_opc_value_to_variant_bool_true`, `test_opc_value_to_variant_bool_false`, `test_opc_value_to_variant_string` — verifies typed `OpcValue` to COM `VARIANT` conversions.
- [x] `test_variant_roundtrip` — validates lossless roundtrip conversions across all basic types (`Int`, `Float`, `Bool`, `String`, `Empty`, `Null`).
- [x] `test_variant_to_string_cy` — validates 64-bit fixed-point Currency (`VT_CY`) scaling and formatting.
- [x] `test_variant_to_string_empty` and `test_variant_to_string_null` — validates Empty and Null variant rendering.
- [x] `test_variant_to_string_i2_and_r4` — validates 16-bit integer and single-precision float formatting.
- [x] `test_variant_to_string_unknown_vt` — verifies fallback formatting for unrecognized VARENUM types.
- [x] `test_variant_to_string_safearray_i4` — validates 1-D SafeArray traversal and formatting.
- [x] `test_variant_to_string_vt_error_known` and `test_variant_to_string_vt_error_unknown` — validates `VT_ERROR` HRESULT diagnostic mapping.

### Error & Diagnostic Unit Tests (in `errors.rs`)

- [x] `test_opc_error_friendly_hint` — verifies `friendly_hint` returns `None` for non-COM errors and expected text for known COM errors.
- [x] `test_friendly_hint_known_codes` — verifies HRESULT hints for known codes (`RPC_S_CALL_FAILED_DNE`, `REGDB_E_CLASSNOTREG`, `OPC_E_BADRIGHTS`, `OPC_E_BADTYPE`, `OPC_E_UNKNOWNITEMID`, `OPC_E_INVALIDITEMID`).
- [x] `test_friendly_hint_unknown_code` — verifies `None` on unknown or internal error codes.
- [x] `test_opc_operation_display` — validates canonical string formatting across all `OpcOperation` enum variants.
- [x] `test_log_opc_err_macro` — validates structured key-value emission and diagnostic capture via `log_opc_err!`.
- [x] `test_channel_error_conversions_and_lock_poison` — verifies `From` conversions for `mpsc::RecvError`, `oneshot::RecvError`, `SendError`, and `PoisonError` to `OpcError::Internal`, and `OpcError::connection_failed`.

### Raw Memory Safety Unit Tests (in `raw/memory.rs`)

- [x] `test_remote_array_safety_and_invariants` — verifies zero-allocation remote array creation, safe move-only drop semantics, and heap integrity without `Clone`.
- [x] `test_remote_pointer_into_string_raii_safety` — verifies `RemotePointer<u16>::into_string` converts valid UTF-16, rejects null pointers with `OpcError::Com`, and automatically cleans up unmanaged COM memory via `CoTaskMemFree`.

### Mock-Based Tests (in `opc-cli`)

- [x] `MockOpcProvider` returns expected server list.
- [x] `MockOpcProvider` returns expected browse results.
- [x] `MockOpcProvider` returns expected tag values.
- [x] `MockOpcProvider` simulates error conditions for UI error handling.
- [x] `test_destructure_tag_value_ergonomics` — verifies destructuring of `TagValue` with `v.value.display()` and `v.timestamp.display()`.
- [x] `test_browse_tags_collector_timeout_and_cancellation` — verifies cooperative cancellation and partial harvesting on timeout in TUI task.

### Doc Tests

- [x] `OpcError::friendly_hint` — runnable doctest in `errors.rs`.
- [x] `OpcError::connection_failed` — runnable doctest in `errors.rs`.
- [x] `OpcResult`, `OpcError` — runnable doctests in `errors.rs`.
- [x] `TagValue`, `OpcValue`, `WriteResult`, `DisplayOption*`, `OpcValueOptionExt`, `SystemTimeOptionExt` — runnable doctests in `provider.rs`.
- [x] `TagCollector` methods (`new`, `unbounded`, `max_tags`, `len`, `is_empty`, `is_full`, `cancel`, `is_cancelled`, `snapshot`, `harvest`, `push`) — runnable doctests in `provider.rs`.
- [x] `OpcProvider` trait methods (`list_servers`, `browse_tags`, `read_tag_values`, `write_tag_value`) — runnable doctests in `provider.rs` backed by `MockOpcProvider` assertions.
- [x] `GroupHandle`, `ItemHandle`, `OpcQuality`, `BrowseType`, `BrowseDirection` — runnable doctests in `types.rs`.
- [x] `OpcDaClient::new` — doctest in `com/client.rs`.
- [x] `ComGuard` — internal-only ignored doctest in `com/guard.rs`.
- [x] Quick Start — runnable doctest in `lib.rs`.
- [x] Usage Examples (Listing, Reading, Writing, Browsing) — compiled doctests in `README.md`.
- [x] Mocking in Unit Tests — runnable doctest in `lib.rs` / `README.md` under `--all-features`.

### Integration / Manual Tests

- [ ] `list_servers("localhost")` returns non-empty list on a machine with OPC servers installed.
- [x] `browse_tags` correctly discovers tags on a flat-namespace server (verified via `test_worker_browse_tags_flat_organization`).
- [x] `browse_tags` correctly discovers tags on a hierarchical-namespace server (verified via `test_worker_browse_tags_success`).
- [x] `browse_tags` respects `max_tags` cap (verified via `test_worker_browse_tags_capacity_cap`).
- [x] `browse_tags` populates `TagCollector` incrementally (observable via lock-free len counter) (verified via `test_worker_browse_tags_success` and `test_tag_collector_lifecycle`).
- [ ] `read_tag_values` returns correct value/quality/timestamp for known tags.
- [ ] `read_tag_values` gracefully handles tags that fail `add_items`.
- [ ] `write_tag_value` returns success for a valid write to a simulation tag.
- [ ] `write_tag_value` returns error (with hint) when writing to a read-only tag.
- [ ] `opc_value_to_variant` correctly converts all `OpcValue` variants.

