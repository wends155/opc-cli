# 📋 spec.md — opc-da-client

> **Behavioral Source of Truth** for the `opc-da-client` library crate.
> Defines *what* each module should do — independent of current implementation.
>
> Last verified against: a1ea491

---

## 1. Module / Component Contracts

### 1.1 `provider` — Core Trait & Data Types

**Purpose:** Define the async trait that all OPC DA backends must implement, plus the canonical data model for tag values.

#### Public API

##### Segregated Role Traits & Composite `OpcProvider`

All methods use `#[async_trait]`.

###### `trait ServerDiscovery: Send + Sync`
| Method | Signature | Description |
| :--- | :--- | :--- |
| `list_servers` | `async fn list_servers(&self, host: &str) -> OpcResult<Vec<String>>` | Enumerate OPC DA servers available on `host`. |
| `list_server_details` | `async fn list_server_details(&self, host: &str) -> OpcResult<Vec<OpcServerInfo>>` | Enumerate OPC DA servers on `host` with rich metadata (`ProgID`, `CLSID`, user-readable name). Default implementation synthesizes records wrapping `list_servers`. |

###### `trait TagBrowser: Send + Sync`
| Method | Signature | Description |
| :--- | :--- | :--- |
| `browse_tags` | `async fn browse_tags(&self, server: &str, collector: TagCollector) -> OpcResult<Vec<String>>` | Recursively discover tags on `server`, pushing each to `collector` as found. |

###### `trait TagReader: Send + Sync`
| Method | Signature | Description |
| :--- | :--- | :--- |
| `read_tag_values` | `async fn read_tag_values(&self, server: &str, tag_ids: TagBatch) -> OpcResult<TagValues>` | Read current value, quality, and timestamp for the given tag IDs, returning a rich `TagValues` collection. |
| `read_tag_value` | `async fn read_tag_value(&self, server: &str, tag_id: &str) -> OpcResult<TagValue>` | Convenience helper to read a single tag on `server`. Default implementation delegates to `read_tag_values`. |

###### `trait TagWriter: Send + Sync`
| Method | Signature | Description |
| :--- | :--- | :--- |
| `write_tag_value` | `async fn write_tag_value(&self, server: &str, tag_id: &str, value: OpcValue) -> OpcResult<WriteResult>` | Write a typed value to a single tag on `server`. |
| `write_tag_values` | `async fn write_tag_values(&self, server: &str, writes: &[(String, OpcValue)]) -> OpcResult<Vec<WriteResult>>` | Convenience helper to write multiple tags sequentially on `server`. Default implementation iterates over `write_tag_value`. |

###### `trait OpcProvider: ServerDiscovery + TagBrowser + TagReader + TagWriter + Send + Sync`
Composite marker trait representing the full OPC DA client capability set. A blanket implementation is provided for any type implementing all four segregated role traits.

**Error Conditions:**

| Method | Error Condition | Meaning |
| :--- | :--- | :--- |
| `list_servers` | Target host unreachable / RPC server unavailable | Return `OpcError::Connection` with hint |
| `list_servers` | Access denied / DCOM security failure | Return `OpcError::Com` (E_ACCESSDENIED) |
| `browse_tags` | Server ProgID not found | Return `OpcError::Server` with hint |
| `browse_tags` | Max depth exceeded | Stop recursion, return tags collected so far |
| `browse_tags` | Cancelled | Return tags collected so far (not an error) |
| `read_tag_values` | Server connection lost | Return `OpcError::Connection` with reconnect hint |
| `write_tag_value` | Tag read-only | Return `WriteResult` with `Err(OpcError::Com)` |
| `write_tag_value` | Tag not found | Return `WriteResult` with `Err(OpcError::Com)` |

**Invariants:**

*   All methods are `Send + Sync` safe; they are safe to call from an async context.
*   `list_servers` returns a **sorted, deduplicated** list of ProgID strings.
*   `list_server_details` default implementation synthesizes `OpcServerInfo` with `GUID::zeroed()` and `user_type: None`, ensuring full backward compatibility.
*   `browse_tags` **never** collects more than `collector.max_tags()` items.
*   `browse_tags` pushes tags to `collector` incrementally; on timeout the caller can harvest partial results.
*   `browse_tags` updates `collector` length atomically and lock-free for each discovered tag.
*   `read_tag_values` returns a `TagValues` collection with an entry for all requested tags, preserving the original array length and order. Items that fail receive error quality with `outcome: Err(OpcError)`.
*   `write_tag_value` returns `Ok(WriteResult)` in all non-fatal cases; per-tag success or error is reported inside `WriteResult.status` as a strongly-typed `Result<(), OpcError>`.

---

##### `struct TagValue`

**Purpose:** Canonical representation of an OPC DA tag value with quality, timestamp, and encapsulated outcome.

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `tag_id` | `String` | Yes | The fully-qualified tag identifier. |
| `outcome` | `Result<OpcValue, OpcError>` | Yes (Public) | Encapsulated read outcome (prevents incoherent states). |
| `quality` | `OpcQuality` | Yes | Decomposed 16-bit quality status. |
| `timestamp` | `Option<SystemTime>` | Yes | Timestamp of last change, or `None` if unavailable. |

**Methods:**
* `new(tag_id, value, quality, timestamp) -> Self`: Constructs a tag value with an `Ok(value.unwrap_or(Empty))` outcome.
* `success(tag_id, value, quality, timestamp) -> Self`: Constructs a successful tag value with an `Ok(value)` outcome.
* `with_error(tag_id, quality, error) -> Self`: Constructs an error tag value (`Err(error)`) with `None` timestamp.
* `outcome(&self) -> Result<&OpcValue, &OpcError>`: Accesses borrowed reference to inner outcome.
* `value(&self) -> Option<&OpcValue>`: Returns `Some(&OpcValue)` if successful.
* `error(&self) -> Option<&OpcError>`: Returns `Some(&OpcError)` if failed.
* `is_good(&self) -> bool`: Returns `true` if quality is good and outcome is `Ok`.
* `is_uncertain(&self) -> bool`: Returns `true` if quality is uncertain and outcome is `Ok`.
* `is_bad(&self) -> bool`: Returns `true` if quality is bad or outcome is `Err`.
* `is_error(&self) -> bool`: Returns `true` if outcome is `Err` (independent of quality).
* `into_result(self) -> TagResult`: Converts into strongly-typed `Result<TagSuccess, TagFailure>`.
* `to_result(&self) -> TagResult`: Converts borrowed reference into `TagResult`.
* `Default`: Yields empty tag ID, `Ok(OpcValue::Empty)`, default quality (`0x0000`), and `None` timestamp.
* `Display`: Canonical formatting rendering `"{tag_id} = {value} [{quality}] @ {timestamp}"`.

**Derives:** `Debug`, `Clone`, `PartialEq`, `Default`.

###### `struct TagSuccess`
Represents a successfully decoded tag read holding `tag_id: String`, `value: OpcValue`, `quality: OpcQuality`, and `timestamp: Option<SystemTime>`.

###### `struct TagFailure`
Represents a failed tag read holding `tag_id: String`, `quality: OpcQuality`, and `error: OpcError`.

###### `type TagResult = Result<TagSuccess, TagFailure>`
Strongly-typed result alias returned by `TagValue::into_result` and `TagValue::to_result`.

---

##### `enum TagBatch`

**Purpose:** Zero-allocation polymorphic container for passing tag identifiers into read operations.

| Variant | Inner Representation | Description |
| :--- | :--- | :--- |
| `InlineSingle(&'a str)` | `&'a str` | Single borrowed string slice without lifetime constraints. |
| `Borrowed(&'a [&'a str])` | `&'a [&'a str]` | Borrowed slice of string slices. |
| `Static(&'static [&'static str])` | `&'static [&'static str]` | Zero-allocation static literal tag slice. |
| `StaticSingle(&'static str)` | `&'static str` | Single static literal string slice. |
| `Shared(Arc<[String]>)` | `Arc<[String]>` | Shared reference-counted tag array. |
| `Owned(Vec<String>)` | `Vec<String>` | Owned vector of dynamic tags (from browse, config, or TUI). |
| `OwnedSingle(String)` | `String` | Single owned heap string. |

**Methods:**
* `len(&self) -> usize`: Returns tag count across all variants.
* `is_empty(&self) -> bool`: Returns `true` if empty.
* `iter_str(&self) -> TagBatchIter<'_>`: Zero-allocation string iterator projecting `&str` over all variants.
* `iter(&self) -> TagBatchIter<'_>`: Alias for `iter_str(&self)`.
* `into_vec(self) -> Vec<String>`: Converts into owned vector, reusing existing allocations where possible.

**Derives:** `Debug`, `Clone`, `PartialEq`, `Eq`.

---

##### `trait IntoTags: Send`

**Purpose:** Universal conversion trait providing zero-allocation ergonomics for callers passing tags into `OpcDaClient` inherent read and subscription methods.

Implemented for:
* `TagBatch` $\rightarrow$ `TagBatch`
* `&'static [&'static str]` $\rightarrow$ `TagBatch::Static`
* `&'static [&'static str; N]` $\rightarrow$ `TagBatch::Static`
* `[&'static str; N]` $\rightarrow$ `TagBatch::Owned`
* `&'static str` $\rightarrow$ `TagBatch::StaticSingle`
* `Vec<String>` $\rightarrow$ `TagBatch::Owned`
* `String` $\rightarrow$ `TagBatch::OwnedSingle`
* `Arc<[String]>` $\rightarrow$ `TagBatch::Shared`

---

##### `struct TagValues`

**Purpose:** Rich collection wrapping `Vec<TagValue>` with linear search, case-insensitive indexing, lenient typed extraction, numeric coercion, and per-item error preservation.

| Method | Signature | Description |
| :--- | :--- | :--- |
| `new(values: Vec<TagValue>)` | `pub fn new(values: Vec<TagValue>) -> Self` | Wraps a vector of tag values. |
| `get(&self, tag: &str)` | `pub fn get(&self, tag: &str) -> Option<&TagValue>` | Case-insensitive lookup of tag value. |
| `get_index(&self, index: usize)` | `pub fn get_index(&self, index: usize) -> Option<&TagValue>` | Zero-based index lookup of tag value. |
| `get_value(&self, tag: &str)` | `pub fn get_value(&self, tag: &str) -> Option<&OpcValue>` | Case-insensitive lookup of unwrapped OPC value. |
| `get_as<T>(&self, tag: &str)` | `pub fn get_as<T>(&self, tag: &str) -> Result<T, TagExtractError> where T: TryFrom<OpcValue, Error = &'static str> + Copy` | Generic typed extraction with lossless conversion. |
| `get_f64(&self, tag: &str)` | `pub fn get_f64(&self, tag: &str) -> Result<f64, TagExtractError>` | Lenient float extraction (coerces `Float`, `Int`). |
| `get_f32(&self, tag: &str)` | `pub fn get_f32(&self, tag: &str) -> Result<f32, TagExtractError>` | Single-precision float extraction via `get_as<f32>`. |
| `get_i32(&self, tag: &str)` | `pub fn get_i32(&self, tag: &str) -> Result<i32, TagExtractError>` | Lenient 32-bit integer extraction (coerces `Int`, whole-number `Float` without loss). |
| `get_i64(&self, tag: &str)` | `pub fn get_i64(&self, tag: &str) -> Result<i64, TagExtractError>` | 64-bit integer extraction via `get_as<i64>`. |
| `get_u32(&self, tag: &str)` | `pub fn get_u32(&self, tag: &str) -> Result<u32, TagExtractError>` | 32-bit unsigned integer extraction via `get_as<u32>`. |
| `get_u64(&self, tag: &str)` | `pub fn get_u64(&self, tag: &str) -> Result<u64, TagExtractError>` | 64-bit unsigned integer extraction via `get_as<u64>`. |
| `get_bool(&self, tag: &str)` | `pub fn get_bool(&self, tag: &str) -> Result<bool, TagExtractError>` | Boolean extraction on `Bool` values. |
| `get_str(&self, tag: &str)` | `pub fn get_str(&self, tag: &str) -> Result<&str, TagExtractError>` | String slice extraction on `String` values. |
| `len(&self)` | `pub fn len(&self) -> usize` | Returns number of contained tag values. |
| `is_empty(&self)` | `pub fn is_empty(&self) -> bool` | Returns `true` if collection is empty. |
| `as_slice(&self)` | `pub fn as_slice(&self) -> &[TagValue]` | Projects borrowed slice of inner tag values. |
| `into_vec(self)` | `pub fn into_vec(self) -> Vec<TagValue>` | Unwraps inner vector. |
| `iter(&self)` | `pub fn iter(&self) -> std::slice::Iter<'_, TagValue>` | Yields iterator over borrowed `&TagValue` items. |
| `iter_results(&self)` | `pub fn iter_results(&self) -> impl Iterator<Item = TagResult> + '_` | Yields iterator projecting each tag into strongly-typed `TagResult`. |
| `clear(&mut self)` | `pub fn clear(&mut self)` | Empties the collection. |
| `push(&mut self, value: TagValue)` | `pub fn push(&mut self, value: TagValue)` | Appends a `TagValue` to the end of the collection. |

**Traits:**
* `From<Vec<TagValue>>`: Infallible conversion from `Vec<TagValue>`.
* `From<TagValues> for Vec<TagValue>`: Infallible conversion into inner `Vec<TagValue>`.
* `FromIterator<TagValue>`: Collects iterator of `TagValue` into `TagValues`.
* `Deref<Target = [TagValue]>`: Dereferences to borrowed slice `[TagValue]`.
* `AsRef<[TagValue]>`: Infallible slice projection.
* `IntoIterator<Item = TagValue>`: Owning iteration over tag values.
* `IntoIterator for &TagValues`: Borrowed iteration over tag values.

**Derives:** `Debug`, `Clone`, `PartialEq`, `Default`.

---

##### `enum TagExtractError`

**Purpose:** Strongly-typed failure enum returned by `TagValues` extraction helpers.

| Variant | Description |
| :--- | :--- |
| `NotRequested(String)` | Tag was not included in this read batch. |
| `ReadFailed { tag: String, source: OpcError }` | Tag read failed server-side; preserves root COM/driver error. |
| `NoValue(String)` | Tag exists in batch but returned null, empty, or missing value. |
| `TypeMismatch { tag: String, value: String, expected: &'static str }` | Tag value could not be coerced into expected type without loss. |

**Traits:**
* `Display`, `std::error::Error`.
* `From<TagExtractError> for OpcError`: When converted to `OpcError`, `TagExtractError::ReadFailed { source, .. }` unwraps and preserves the root COM or transport `source: OpcError` directly, preventing loss of underlying HRESULT codes and diagnostic hints. Other variants convert to `OpcError::Conversion`.

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
| `Int(i64)` | `i64` | Signed integer (64-bit with adaptive 32-bit `VT_I4` coercion). | `VT_I4` / `VT_I8` |
| `UInt(u64)` | `u64` | Unsigned integer (64-bit with adaptive 32-bit `VT_UI4` coercion). | `VT_UI4` / `VT_UI8` |
| `Float(f64)` | `f64` | 64-bit float. | `VT_R8` |
| `Bool(bool)` | `bool` | Boolean value. | `VT_BOOL` |
| `Empty` | N/A | Empty variant (uninitialized). | `VT_EMPTY` |
| `Null` | N/A | Explicitly null variant. | `VT_NULL` |

**Methods & Conversions:**
* `as_int(&self) -> Option<i64>`: Returns `Some(i64)` if this value is [`OpcValue::Int`], or `None` otherwise.
* `as_uint(&self) -> Option<u64>`: Returns `Some(u64)` if this value is [`OpcValue::UInt`], or `None` otherwise.
* `as_float(&self) -> Option<f64>`: Returns `Some(f64)` if this value is [`OpcValue::Float`], or `None` otherwise.
* `as_bool(&self) -> Option<bool>`: Returns `Some(bool)` if this value is [`OpcValue::Bool`], or `None` otherwise.
* `as_str(&self) -> Option<&str>`: Returns borrowed string slice if this value is [`OpcValue::String`], or `None` otherwise.
* `is_empty(&self) -> bool`: Returns `true` if `self` is [`OpcValue::Empty`].
* `is_null(&self) -> bool`: Returns `true` if `self` is [`OpcValue::Null`].
* `From<i64>`, `From<u64>`, `From<i32>`, `From<u32>`, `From<i16>`, `From<u16>`, `From<i8>`, `From<u8>`, `From<f64>`, `From<f32>`, `From<bool>`, `From<String>`, `From<&str>`: Primitive lossless conversions into `OpcValue`.
* `std::str::FromStr`: Parses integer, float, boolean, or falls back to String.
* `Display`: Formats variant value as display string.

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
| `host` | `Option<String>` | Target machine hostname or IP address (`None` for localhost). Normalized upon construction. |
| `identifier` | `ServerIdentifier` | Strongly-typed server identifier (ProgID or direct CLSID). |

**Methods:**
* `local(identifier: impl Into<ServerIdentifier>) -> Self`: Creates a local endpoint (`host = None`).
* `remote(host: impl Into<String>, identifier: impl Into<ServerIdentifier>) -> Self`: Creates a remote endpoint, automatically normalizing localhost aliases to `None`.
* `host(&self) -> Option<&str>`: Borrows optional target host.
* `identifier(&self) -> &ServerIdentifier`: Borrows server identifier.
* `is_remote(&self) -> bool`: Returns `true` if target host is a non-empty remote host (not localhost).
* `into_parts(self) -> (Option<String>, ServerIdentifier)`: Deconstructs into `(host, identifier)` tuple.

**Traits:**
* `Display`: Formats remote endpoints as UNC path `r"\\{}\{}"` (e.g. `r"\\192.168.1.10\Matrikon.OPC.Simulation.1"`), and local endpoints as `"{}"`.
* `std::str::FromStr`: Parses endpoints formatted as UNC (`\\host\server` or `//host/server`) or local server identifiers (`server`). Normalizes localhost aliases to `None`.
* `From<&str>`: Parses endpoint string slice via `FromStr`.
* `From<String>`: Parses endpoint string via `FromStr`.

**Host Normalization Functions (`types/server.rs`):**
* `normalize_host_str(host: &str) -> Option<&str>`: Strips leading backslashes/slashes, trims whitespace, and maps localhost aliases (`"localhost"`, `"127.0.0.1"`, `"::1"`, `"."`, `""`) to `None`.
* `normalize_host(host: Option<String>) -> Option<String>`: Normalizes owned host string, returning `None` if local or empty.
* `is_remote_host(host: Option<&str>) -> bool`: Checks if an optional host string resolves to a remote host.

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

| Item | Signature / Type | Purpose |
| :--- | :--- | :--- |
| `variant_to_string` | `fn(variant: &VARIANT) -> String` | Formats a COM VARIANT as a display string. Handles VT_EMPTY, VT_NULL, VT_I2, VT_I4, VT_R4, VT_R8, VT_CY, VT_DATE, VT_BSTR, VT_ERROR, VT_BOOL, VT_I1, VT_UI1, VT_UI2, VT_UI4, VT_I8, VT_UI8, and VT_ARRAY composites. |
| `variant_to_opc_value` | `fn(variant: &VARIANT) -> OpcValue` | Converts a COM `VARIANT` into a strongly-typed domain `OpcValue`. |
| `opc_value_to_variant` | `fn(value: &OpcValue) -> VARIANT` | Converts a domain `OpcValue` to a COM `VARIANT`. |
| `struct ScopedVariant` | `pub struct ScopedVariant(pub VARIANT)` | RAII wrapper with `#[repr(transparent)]` layout and deterministic `Drop` invoking `VariantClear(&raw mut self.0)`. Provides `empty()`, `from_opc_value(&OpcValue)`, `into_inner(self)`, `as_raw_mut(&mut self)`, and `clear(&mut self)`. |
| `struct ItemStatesGuard<'a>` | `pub struct ItemStatesGuard<'a>(pub &'a mut [tagOPCITEMSTATE])` | RAII guard invoking `VariantClear` on element `i`'s `vDataValue` upon `Drop` if and only if `errors[i].is_ok()` (HRESULT `S_OK`) (REV-01), preventing double-free or clearing uninitialized variants. Implements `std::ops::Deref<Target = [tagOPCITEMSTATE]>`. |
| `struct ItemResultsBlobGuard` | `pub struct ItemResultsBlobGuard(*mut OPCITEMRESULT, usize)` | RAII guard wrapping `*mut OPCITEMRESULT` array allocated by `add_items`, deterministically freeing all `pBlob.pBlobData` memory via `CoTaskMemFree` (REV-23) before freeing the results array itself. |

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

##### `struct OpcDaClientBuilder<C = ComConnector>`

**Purpose:** Fluent builder for configuring and instantiating an `OpcDaClient` in either `Unbound` (gateway) or `Bound` (session) state.

| Method | Signature | Description |
| :--- | :--- | :--- |
| `host` | `pub fn host(mut self, host: impl Into<String>) -> Self` | Configures target host machine for remote DCOM. |
| `server` | `pub fn server(mut self, server: impl Into<ServerIdentifier>) -> Self` | Configures target OPC DA server identifier (ProgID or CLSID). |
| `timeout` | `pub fn timeout(mut self, timeout: Duration) -> Self` | Configures request timeout duration (default: 5s). |
| `with_legacy_dcom` | `pub fn with_legacy_dcom(mut self, legacy: bool) -> Self` | Configures DCOM packet authentication (`true` for `CONNECT` level on legacy NT 6.1 hosts; `false` for post-KB5004442 `PKT_INTEGRITY`). |
| `with_connector` | `pub fn with_connector<C2: ServerConnector + 'static>(self, connector: C2) -> OpcDaClientBuilder<C2>` | Transitions builder to custom or mock connector type. |
| `build_with_connector` | `pub fn build_with_connector(self, connector: C) -> OpcResult<OpcDaClient<C, Unbound>>` | Builds unbound client using an explicit connector instance. |
| `build` | `pub fn build(self) -> OpcResult<OpcDaClient<C, Unbound>>` | Builds unbound client gateway with default connector. |
| `build_bound` | `pub fn build_bound(self) -> OpcResult<OpcDaClient<C, Bound>>` | Builds server-bound client session. Returns `OpcError::InvalidConfiguration` if server identifier is unset. |

---

##### `struct OpcDaClient<C = ComConnector, State = Unbound>`

**Typestates:**
* `Unbound`: Compile-time typestate representing an unbound multi-server gateway. Suitable for catalog discovery, listing servers, and dynamic ad-hoc operations.
* `Bound`: Compile-time typestate representing a server-bound active session targeting a specific `OpcServerEndpoint`. Grants infallible access to `endpoint(&self)` and ergonomic session read/write methods.

**Constructors on `OpcDaClient<ComConnector, Unbound>`:**
| Constructor | Signature | Description |
| :--- | :--- | :--- |
| `builder()` | `fn builder() -> OpcDaClientBuilder<ComConnector>` | Creates a fluent builder with default native connector. |
| `connect(server)` | `fn connect(server: impl Into<ServerIdentifier>) -> OpcResult<OpcDaClient<ComConnector, Bound>>` | Connects directly to a local OPC server, returning a `Bound` client session. |
| `connect_remote(host, server)` | `fn connect_remote(host: impl Into<String>, server: impl Into<ServerIdentifier>) -> OpcResult<OpcDaClient<ComConnector, Bound>>` | Connects to a remote OPC server via remote DCOM, returning a `Bound` client session. |
| `connect_eager(server)` | `fn connect_eager(server: impl Into<ServerIdentifier>) -> OpcResult<OpcDaClient<ComConnector, Bound>>` | Connects and eagerly verifies server liveness by performing an immediate root browse ping before returning `Bound` client. |

**Constructors & Transitions on `OpcDaClient<C, Unbound>`:**
| Method | Signature | Description |
| :--- | :--- | :--- |
| `new(connector: C)` | `fn new(connector: C) -> OpcResult<Self>` | Constructs unbound client facade with custom/mock connector. |
| `default()` | `fn default() -> Self` | Constructs unbound client with default native connector. |
| `bind(self, endpoint)` | `pub fn bind<E: Into<OpcServerEndpoint>>(self, endpoint: E) -> OpcDaClient<C, Bound>` | Transitions unbound gateway into a bound session targeting `endpoint`. |
| `bind_remote(self, host, server)` | `pub fn bind_remote(self, host: impl Into<String>, server: impl Into<ServerIdentifier>) -> OpcDaClient<C, Bound>` | Convenience helper to bind to a remote host and server identifier. |

**Typestate Methods on `OpcDaClient<C, Bound>`:**
| Method | Signature | Description |
| :--- | :--- | :--- |
| `endpoint(&self)` | `pub fn endpoint(&self) -> &OpcServerEndpoint` | Infallibly borrows the bound server endpoint (guaranteed by `Bound` typestate invariant). |
| `server_id(&self)` | `pub fn server_id(&self) -> std::borrow::Cow<'_, str>` | Convenience getter returning the server ProgID or bracketed CLSID string. |
| `unbind(self)` | `pub fn unbind(self) -> (OpcDaClient<C, Unbound>, OpcServerEndpoint)` | Consumes bound session and returns an unbound gateway and the previous endpoint. |
| `read_tag(&self, tag: &str)` | `pub async fn read_tag(&self, tag: &str) -> OpcResult<TagValue>` | Reads a single tag and returns full `TagValue` with outcome, quality, and timestamp. |
| `read_tags(&self, tags: impl IntoTags)` | `pub async fn read_tags(&self, tags: impl IntoTags) -> OpcResult<TagValues>` | Reads a batch of tags and returns rich `TagValues` collection. |
| `write_tag(&self, tag: &str, value: impl Into<OpcValue>) -> OpcResult<WriteResult>` | `pub async fn write_tag(&self, tag: &str, value: impl Into<OpcValue>) -> OpcResult<WriteResult>` | Writes a single typed value to a tag. |
| `write_tags(&self, writes: Vec<(String, OpcValue)>) -> OpcResult<Vec<WriteResult>>` | `pub async fn write_tags(&self, writes: Vec<(String, OpcValue)>) -> OpcResult<Vec<WriteResult>>` | Writes multiple tags in a single native DCOM batch operation. |

**General & Compatibility Methods on `OpcDaClient<C, State>`:**
| Method | Signature | Description |
| :--- | :--- | :--- |
| `endpoint(&self)` | `pub fn endpoint(&self) -> Option<&OpcServerEndpoint>` | Returns `Some(&OpcServerEndpoint)` if bound, `None` if unbound. |
| `is_bound(&self)` | `pub fn is_bound(&self) -> bool` | Checks whether this client instance is bound to a server. |
| `read_tag_values(&self, tags: impl IntoTags)` | `async fn read_tag_values(&self, tags: impl IntoTags) -> OpcResult<TagValues>` | Zero-allocation batch read returning a rich `TagValues` collection. |
| `read_tag_value(&self, tag: &str)` | `async fn read_tag_value(&self, tag: &str) -> OpcResult<TagValue>` | Reads a single tag returning `TagValue`. |
| `read_f64(&self, tag: &str)` | `async fn read_f64(&self, tag: &str) -> OpcResult<f64>` | Reads a single tag and coerces value to `f64`. |
| `read_i32(&self, tag: &str)` | `async fn read_i32(&self, tag: &str) -> OpcResult<i32>` | Reads a single tag and coerces value to `i32`. |
| `read_bool(&self, tag: &str)` | `async fn read_bool(&self, tag: &str) -> OpcResult<bool>` | Reads a single tag and coerces value to `bool`. |
| `read_string(&self, tag: &str)` | `async fn read_string(&self, tag: &str) -> OpcResult<String>` | Reads a single tag as a `String`. |
| `write(&self, tag: &str, value: impl Into<OpcValue>) -> OpcResult<WriteResult>` | `async fn write(&self, tag: &str, value: impl Into<OpcValue>) -> OpcResult<WriteResult>` | Writes a single value to a tag, returning a `WriteResult`. |
| `write_batch(&self, writes: Vec<(String, OpcValue)>) -> OpcResult<Vec<WriteResult>>` | `async fn write_batch(&self, writes: Vec<(String, OpcValue)>) -> OpcResult<Vec<WriteResult>>` | Writes multiple tags in a single native DCOM batch operation. |
| `list_servers_on(&self, host: &str)` | `async fn list_servers_on(&self, host: &str) -> OpcResult<Vec<String>>` | *(Deprecated since 0.2.1)* Discovers OPC servers on a host. Prefer `ServerDiscovery::list_servers`. |
| `subscribe(&self, tags: impl IntoTags, interval: Duration)` | `fn subscribe(&self, tags: impl IntoTags, interval: Duration) -> tokio::sync::mpsc::Receiver<TagValues>` | Starts a Layer 2 non-blocking polling stream yielding `TagValues` periodically. Dropping the receiver cancels the background task. |

Implements `OpcProvider` for all five trait methods (`list_servers`, `list_server_details`, `browse_tags`, `read_tag_values`, `write_tag_value`) by dispatching to the `ComWorker`.

**Invariants:**
*   All COM work runs on a dedicated, long-lived `ComWorker` thread, avoiding repeated initialization overhead and solving COM thread-affinity constraints.
*   **Two-tier `catch_unwind` panic resilience:** Individual request handling is wrapped in `std::panic::catch_unwind` (tier 1) so driver panics return structured `OpcError::Internal` without terminating the worker thread. The outer thread loop is also protected (tier 2) to maintain client liveness.
*   **Active Group Caching:** Connection pool in `pool.rs` embeds `PooledServer<S>` which caches active OPC groups and item handles on identical tag sets, reducing round-trip RPC overhead during cyclic polling by over 75%. Tag set changes or connection drops transparently recreate or evict the cached group.
*   **Failure Cooldown Circuit Breaker:** Unresponsive remote endpoints trigger a 5-second cooldown in `failure_cooldowns` to avoid connection storm panics.
*   **Collision-Proof Group Naming:** Group names are generated using process ID and an atomic sequence counter (`generate_group_name`).
*   **Native Batch Writes:** `handle_write_batch` performs native multi-item writes in a single COM group transaction; `handle_write` delegates directly to it.
*   Stale connections are transparently evicted and retried during request dispatch.
*   GUID filtering: zeroed GUIDs are skipped during server enumeration.
*   OPC groups created by `read_tag_values` and `write_tag_value` are **always** managed by `GroupGuard`, guaranteeing deterministic invocation of `remove_group(handle, true)` on `Drop` across all return paths, early returns with `?`, and thread panics.
*   `StringIterator` implements `Drop` (REV-07), freeing any remaining cached COM BSTRs via `CoTaskMemFree` to eliminate unmanaged memory leaks.

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

##### `struct BrowsePositionGuard<'a, S: ConnectedServer>` (Internal RAII Position Guard)

**Purpose:** Provide an automatic RAII cursor restore guard for hierarchical namespace browsing in `handle_browse` and `browse_recursive`.

| Method | Signature | Description |
| :--- | :--- | :--- |
| `enter(server: &'a S, branch: &str)` | `pub(crate) fn enter(server: &'a S, branch: &str) -> OpcResult<Self>` | Navigates down into branch via `server.change_browse_position(BrowseDirection::Down, branch)`. On success, wraps server with `active: true`. |
| `disarm(&mut self)` | `pub(crate) fn disarm(&mut self)` | Disarms automatic cursor restore on drop. |

**Drop behavior:** When dropped, if `active`, navigates back up via `self.server.change_browse_position(BrowseDirection::Up, "")`. Any errors are logged as warnings without panicking.

**Invariants:**
*   Guarantees cursor position restoration on all exits from recursive branch traversal (success, early return, error propagation, or panic).

---

### 1.5 `types` — Canonical Protocol Types & Handles

**Purpose:** Provide canonical data structures and newtypes representing OPC DA concepts:

#### Public API

- `ClientGroupHandle`: Encapsulated opaque typestate newtype representing client-assigned group identifier, with constructor `new(u32)`, and accessors `as_raw(&self) -> u32`, `into_raw(self) -> u32`.
- `ServerGroupHandle`: Encapsulated opaque typestate newtype representing server-assigned group identifier, with constructor `new(u32)`, and accessors `as_raw(&self) -> u32`, `into_raw(self) -> u32`.
- `ClientItemHandle`: Encapsulated opaque typestate newtype representing client-assigned item identifier, with constructor `new(u32)`, and accessors `as_raw(&self) -> u32`, `into_raw(self) -> u32`.
- `ServerItemHandle`: Encapsulated opaque typestate newtype representing server-assigned item identifier, with constructor `new(u32)`, and accessors `as_raw(&self) -> u32`, `into_raw(self) -> u32`.
- `ItemHandle`: Legacy backward-compatible type alias for `ServerItemHandle`.
- `OpcQuality`: Fully decomposed, zero-allocation 16-bit OPC DA quality word with private fields and getter methods (`major()`, `substatus()`, `limit()`, `raw()`). Implements `From<u16>`, `From<OpcQuality> for u16`, `Display` (rich human-readable diagnostics), `std::str::FromStr` returning `Result<Self, ParseQualityError>`, and predicates (`is_good`, `is_bad`, `is_uncertain`, `is_limited`).
- `ParseQualityError`: Error struct returned when parsing an invalid quality string via `FromStr`. Implements `Display` and `std::error::Error`.
- `OpcValue`: Canonical domain value enum (`String(String)`, `Int(i64)`, `UInt(u64)`, `Float(f64)`, `Bool(bool)`, `Empty`, `Null`). Implements `std::str::FromStr`, `From` for primitive integer, float, boolean, and string types, and typed accessors (`as_str`, `as_int`, `as_uint`, `as_float`, `as_bool`, `is_empty`, `is_null`).
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

**Description:** Inspects the local machine Windows registry for an OPC DA server's registration details by querying `HKCR\CLSID\{...}` across both native and 32-bit (`KEY_WOW64_32KEY`) views. Registry key traversal is consolidated through a private `open_reg_key` helper wrapping `RegOpenKeyExW` with `KEY_READ`. Registry value reading incorporates two-phase dynamic buffer reallocation on `ERROR_MORE_DATA` (234) and resolves `REG_EXPAND_SZ` strings using `windows::Win32::System::Environment::ExpandEnvironmentStringsW` with safe slice bounds checking.

**Inputs:**
* `clsid`: Reference to the 128-bit COM Class ID.
* `host`: Target host machine. If `Some` and not localhost/127.0.0.1, returns [`OpcError::NotImplemented`].

**Returns:**
* `Ok(OpcServerRegistration)` with resolved binary path and server execution type.

**Errors:**
* [`OpcError::NotImplemented`] if `host` is a remote machine.
* [`OpcError::Com`] with `REGDB_E_CLASSNOTREG` (`0x80040154`) if the CLSID registry key does not exist.
* [`OpcError::Server`] if neither `LocalServer32` nor `InprocServer32` registry keys exist or if registry values cannot be parsed.

---

#### Internal: `OpcServerListCatalog`

**Purpose:** Adapter combining `IOPCServerList` and `IOPCServerList2` with remote DCOM activation support.
* Instantiates catalog via standard `CLSID_OPC_SERVER_LIST` (`{13486D51-4821-11D2-A494-3CB306C10000}`). For remote hosts, uses `CoCreateInstanceEx` with `COSERVERINFO` and applies security proxy blanketing (`apply_proxy_blanket`) enforcing KB5004442 `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY`.
* Uses `IOPCServerList::EnumClassesOfCategories` to enumerate category classes, bypassing the `IOPCEnumGUID` vs standard `IEnumGUID` COM vtable layout mismatch.
* Employs a resilient 3-tier fallback to extract server details:
  1. `IOPCServerList2::GetClassDetails` (v2 interface with version-independent ProgID)
  2. `IOPCServerList::GetClassDetails` (v1 interface with ProgID and user type)
  3. `guid_to_progid` fallback (resolving from registry via COM runtime)

##### `fn guid_to_progid(guid: &GUID) -> OpcResult<String>`

**Purpose:** Converts a COM GUID to its registered ProgID string using `RemotePointer<u16>::into_string(self)` with guaranteed COM allocator cleanup via RAII on both success and error paths.

---

### 1.8 `com::connector` — Pure-Rust Connector Facade & Modular Submodules

**Purpose:** Pure-Rust facade traits, DTOs, and concrete Win32 COM / mock implementations partitioned into cohesive single-responsibility submodules:

* `com::connector::traits`:
  - `ServerConnector`: Discovers servers via `enumerate_servers(host: &str) -> OpcResult<Vec<String>>` and `enumerate_server_details(host: &str) -> OpcResult<Vec<OpcServerInfo>>`, and connects via `connect_endpoint(&OpcServerEndpoint)` (primary required method), `connect_identifier(&ServerIdentifier)`, and `connect(name)`. Implemented by `ComConnector` and `MockServerConnector`.
  - `ConnectedServer`: Introspects server namespace and adds/removes groups using `GroupConfig`, `CreatedGroup`, `ServerGroupHandle`, and `GroupRemovalMode`. Implemented by `ComServer` and `MockConnectedServer`. Supports in-memory tag browsing via `StringIterator::from_vec`.
  - `ConnectedGroup`: Pure-Rust facade over OPC DA groups:
    - `add_items(&self, items: &[GroupItemDef]) -> OpcResult<Vec<GroupItemResult>>`
    - `read(&self, source: DataSource, server_handles: &[ServerItemHandle]) -> OpcResult<Vec<Result<GroupItemState, OpcError>>>`
    - `write(&self, items: &[ItemWrite]) -> OpcResult<Vec<Result<(), OpcError>>>`
  - DTOs: `GroupItemDef`, `GroupItemResult`, `GroupItemState`, `ItemWrite`, `DataSource`, `GroupConfig`, `CreatedGroup`, `GroupRemovalMode`.
* `com::connector::server`:
  - `ComConnector`: Connects to local and remote servers via `connect_server_endpoint` and enumerates servers via Component Categories catalog and `CLSID_OPC_SERVER_LIST`.
  - `ComServer`: Wraps native `IOPCServer` and `IOPCBrowseServerAddressSpace`, managing namespace queries and group creation.
  - `connect_server_endpoint(endpoint: &OpcServerEndpoint, legacy_dcom: bool) -> OpcResult<IOPCServer>`: For local servers, calls `CoCreateInstance` (directly using CLSID if `ServerIdentifier::Clsid`, or resolving ProgID). For remote servers, issues `CoCreateInstanceEx` with `COSERVERINFO` and `COAUTHINFO` (enforcing KB5004442 `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY` unless `legacy_dcom` is enabled), and applies `apply_proxy_blanket` on `IOPCServer` and child groups.
* `com::connector::group`:
  - `ComGroup`: Wraps native group COM interfaces (`IOPCItemMgt`, `IOPCSyncIO`, `IOPCGroupStateMgt`, etc.).
  - Protected by `ScopedVariant` on synchronous write paths and `ItemStatesGuard` on synchronous read paths, guaranteeing zero `VARIANT` memory leaks.
* `com::connector::mock`:
  - `MockConnectedGroup`, `MockConnectedServer`, and `MockServerConnector`: Reusable pure-Rust mocks (under `#[cfg(any(test, feature = "test-support"))]` and exported at crate root under `test-support`) supporting pluggable closures, failure injection (`MockState` with `add_group_count`, `remove_group_count`, `read_count`, `write_count`, `connect_count`), simulated structured server details (`server_details: Arc<Mutex<Vec<OpcServerInfo>>>`, `with_server_details`), bidirectional ProgID/detail sync, and simulated tag browsing without native COM allocators or unsafe blocks.
  - Mock handler type aliases: `MockAddItemsFn`, `MockReadFn`, `MockWriteFn`.
* `com::connector` (Facade):
  - Slim coordinator facade re-exporting all submodule items with zero blanket `#![allow(...)]` headers.
* Crate Root Re-Export:
  - `pub type MockOpcDaClient = com::client::OpcDaClient<com::connector::MockServerConnector>;` exported under `#[cfg(all(feature = "test-support", feature = "opc-da-backend"))]`.

---

### 1.9 `raw` — Crate-Internal Low-Level Win32/COM FFI Subsystem

**Purpose:** Strict crate-internal isolation (`pub(crate) mod raw;`) for all raw Win32 bindings and FFI memory management:
- `raw::bindings`: Autogenerated Win32 COM bindings (`da`, `comn`).
- `raw::memory`: Unsafe memory wrappers (`RemoteArray`, `RemotePointer`, `LocalPointer`, `CoTaskPwstr`) managing `CoTaskMemAlloc` / `CoTaskMemFree`. `RemotePointer::from_raw` is strictly `unsafe`. `CoTaskPwstr` manages RAII freeing of wide strings. Non-freeing borrows are handled via `decode_borrowed_pwstr`. `RemotePointer` and `RemoteArray` are strictly move-only types (`Clone` prohibited) to prevent double-free heap corruptions on unmanaged memory.
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

## 3. State Machines

### 3.1 TUI Screen Navigation (`CurrentScreen`)

The terminal user interface executes a hierarchical navigation state machine:

| Current State | Trigger / Event | Next State | Action / Transition Effect |
|:---|:---|:---|:---|
| `Home` | User enters host + `Enter` | `Loading` | Spawns `list_servers` background task via `start_fetch_servers`. |
| `Loading` | Server fetch resolves `Ok(servers)` | `ServerList` | Populates servers, selects first row, logs transition. |
| `Loading` | Server fetch resolves `Err` | `Home` | Displays friendly error message in status log. |
| `Loading` | `Esc` / `go_back` | `previous_screen` | Cancels active background task, signals collector cancellation, and restores previous screen. |
| `ServerList` | User highlights server + `Enter` | `Loading` | Spawns `browse_tags` task via `start_browse_tags`. |
| `ServerList` | `Esc` / `go_back` | `Home` | Clears server list and resets cursor selection. |
| `Loading` | Tag browse resolves `Ok(tags)` | `TagList` | Populates tags, pre-allocates selection flags, selects first tag. |
| `Loading` | Tag browse resolves `Err` | `ServerList` | Displays friendly error hint in status log. |
| `TagList` | User marks tags + `Enter` | `Loading` | Spawns `read_tag_values` task via `start_read_values`. |
| `TagList` | `Esc` / `go_back` | `ServerList` | Clears tags, preserves server list selection. |
| `Loading` | Tag read resolves `Ok(values)` | `TagValues` | Populates table, initializes table cursor, starts auto-refresh timer. |
| `Loading` | Tag read resolves `Err` | `TagList` | Restores prior tag list with error in status bar. |
| `TagValues` | User selects row + `w` | `WriteInput` | Enters write input prompt for highlighted tag via `enter_write_mode`. |
| `TagValues` | `Esc` / `go_back` | `TagList` | Clears live values and stops auto-refresh timer. |
| `WriteInput` | User inputs value + `Enter` | `Loading` | Spawns `write_tag_value` task with coerced `OpcValue` via `start_write_value`. |
| `WriteInput` | `Esc` / `go_back` | `TagValues` | Aborts write input, clears input buffer. |
| `Loading` | Write resolves `Ok` or `Err` | `TagValues` | Displays write result status and triggers immediate `start_read_values` refresh. |

### 3.2 COM Worker Thread Lifecycle (`ComWorker`)

The background COM worker manages thread affinity and Multi-Threaded Apartment initialization:

```mermaid
stateDiagram-v2
    [*] --> Uninitialized
    Uninitialized --> Spawning : ComWorker::start(connector)
    Spawning --> Running : CoInitializeEx(COINIT_MULTITHREADED) Ok / S_FALSE
    Spawning --> Closed : COM Initialization Error / Channel Drop
    state Running {
        [*] --> Idle
        Idle --> ProcessingRequest : recv(ComRequest)
        ProcessingRequest --> Idle : reply(Result) via oneshot
        ProcessingRequest --> Idle : catch_unwind (Driver Panic -> OpcError::Internal)
    }
    Running --> Closed : Client Sender Dropped (Disconnect)
    Running --> Closed : Outer Thread Loop Panic
    Closed --> [*] : CoUninitialize & Thread Exit
```

- **Panic Isolation Guarantee:** If a vendor COM server crashes or panics within a request handler, Tier 1 `std::panic::catch_unwind` catches the panic, constructs an `OpcError::Internal`, sends the failure to the client's `oneshot` channel, and keeps the worker loop running for subsequent operations.
- **Connection Cache:** Connected server instances are pooled and cached by `ServerIdentifier`. Stale connection errors (`RPC_S_*`) trigger transparent eviction and single-retry reconnection.
- **Persistent Group Cache:** Read operations on identical tag lists reuse active OPC groups, eliminating repeated `add_group`/`add_items` round-trips.

### 3.3 Context-Aware Input Coercion State Machine (`App::resolve_write_value`)

Downstream input parsing follows a 2-phase deterministic coercion machine:

1. **Boolean Inspection Phase:** The tag ID is looked up in the current `tag_values` buffer. If the tag's current value is known to be `Some(OpcValue::Bool(_))`:
   - Input `"1"` or `"true"` $\rightarrow$ `OpcValue::Bool(true)`.
   - Input `"0"` or `"false"` $\rightarrow$ `OpcValue::Bool(false)`.
2. **Canonical FromStr Fallback Phase:** If the tag is not boolean or string is not a boolean token, input is parsed via `OpcValue::from_str`:
   - Numeric integers $\rightarrow$ `OpcValue::Int(i32)`.
   - Floating-point decimals $\rightarrow$ `OpcValue::Float(f64)`.
   - Explicit `"true"` / `"false"` $\rightarrow$ `OpcValue::Bool(bool)`.
   - All other string tokens $\rightarrow$ `OpcValue::String(value_str.to_string())`.

---

## 4. Integration Points

### 4.1 Internal: `com` Subsystem

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

### 4.2 Downstream: `opc-cli` (Consumer)

**Boundary:** `opc-cli` → `dyn OpcProvider`.

*   The CLI crate depends on the `OpcProvider` trait, never on `OpcDaClient` directly in its core logic.
*   Tests use `MockOpcProvider` (via `test-support` feature).
*   `e.friendly_hint()` is called by the CLI to enrich error messages displayed in the TUI status bar.

---

## 5. Required Test Coverage

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
- [x] `test_format_guid_bracketed` — validates bracketed GUID uppercase string formatting matching COM registry conventions.
- [x] `test_opc_server_info_display_name_and_endpoint` — validates `OpcServerInfo` display name fallback and endpoint generation.
- [x] `test_tag_batch_into_tags_conversions` — validates `TagBatch` and `IntoTags` zero-allocation conversions across static slices, arrays, owned vectors, borrowed slices, and Arc slices.
- [x] `test_feature_independence_no_default_features` — validates `types.rs` compiles and tests pass independently without default features.
- [x] `test_tag_values_collection_and_lenient_coercion` — validates `TagValues` collection lookups, case-insensitive indexing, and lenient typed coercions (`get_f64`, `get_i32`, `get_bool`, `get_str`).
- [x] `test_tag_values_coercion_overflow_and_null_edge_cases` — validates coercion overflow handling, null/empty variants, and `ReadFailed` error preservation.

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

### Connector & Client Unit Tests (in `com/connector/` and `com/client.rs`)

- [x] `test_string_iterator_from_vec` — verifies in-memory `StringIterator` collection and equality without COM interfaces.
- [x] `test_string_iterator_drop_frees_unconsumed_cached_strings` — verifies COM `StringIterator` frees cached strings on drop.
- [x] `test_string_iterator_in_memory_drop` — verifies in-memory `StringIterator` cleans up cleanly without errors.
- [x] `test_mock_connector_browse` — verifies `MockConnectedServer::browse_opc_item_ids` returns in-memory simulated tags.
- [x] `test_mock_browse_branch_vs_leaf` — verifies mock server correctly distinguishes branch vs leaf tag paths.
- [x] `test_mock_connector_with_tag_values` — verifies pre-populating mock connector with known tag values.
- [x] `test_mock_group_defaults` and `test_mock_group_custom_handlers` — verifies mock group default results and custom read handlers.
- [x] `test_mock_server_add_group_and_eviction` — verifies group handle generation and connection drop error injection.
- [x] `test_group_item_def_and_state_cloning` — verifies DTO clone and display behavior.
- [x] `test_group_config_ephemeral_and_builders` — verifies ephemeral group configuration builder invariants.
- [x] `test_mock_server_connector_server_details` — verifies `MockServerConnector::with_server_details` and `enumerate_server_details`.
- [x] `test_mock_server_connector_type_aliases_and_dispatch` — verifies `MockAddItemsFn`, `MockReadFn`, and `MockWriteFn` custom handlers and default fallback.
- [x] `test_mock_state_observability_counters` — validates `MockState` counters for group additions, removals, reads, and writes.
- [x] `test_com_group_preconditions` — verifies `ComGroup::add_items`, `read`, and `write` precondition assertions (empty slices, length mismatch) returning `OpcError::InvalidState`.
- [x] `test_mock_opc_da_client_default` — verifies default initialization of mock client facade.
- [x] `test_provider_default_read_tag_value` — verifies default `read_tag_value` delegation in `OpcProvider`.
- [x] `test_client_list_server_details` — verifies `OpcDaClient::list_server_details` dispatch through worker against mock connector.
- [x] `test_client_builder_configuration_and_unbound_discovery` — verifies `OpcDaClientBuilder` parameter configuration and unbound server discovery.
- [x] `test_inherent_async_reads_and_writes_on_client` — validates `OpcDaClient` inherent async readers (`read_tag_values`, `read_f64`, etc.) and batch writers.
- [x] `test_remote_host_propagation_and_discovery` — validates remote host server discovery dispatch without prior server binding.
- [x] `test_client_subscribe_mpsc_polling_stream` — validates Layer 2 subscription stream emissions.
- [x] `test_subscribe_receiver_drop_cancellation` — validates automatic background task cancellation when receiver is dropped.

### COM Security Unit Tests (in `com/security.rs`)

- [x] `test_clsid_opc_server_list_constant` — verifies standard OPCEnum CLSID constant definition.
- [x] `test_authn_level_selection` — verifies dynamic authentication level selection (`PKT_INTEGRITY` vs `CONNECT`).

### COM RAII Guard Unit Tests (in `com/guard.rs`)

- [x] `com_guard_new_returns_opc_result` — static compile test asserting `ComGuard::new()` returns `OpcResult<ComGuard>`.
- [x] `test_group_guard_cleanup_on_drop` — verifies `GroupGuard` automatically invokes `remove_group` when dropped.
- [x] `test_group_guard_disarm_prevents_cleanup` — verifies `GroupGuard::disarm` prevents `remove_group` cleanup invocation.
- [x] `test_browse_position_guard_enter_and_drop` — verifies `BrowsePositionGuard::enter` navigates down and drop restores position by navigating up.
- [x] `test_browse_position_guard_disarm` — verifies `BrowsePositionGuard::disarm` prevents `BrowseDirection::Up` navigation on drop.

### COM Worker Subsystem Unit Tests (in `com/worker/`)

- [x] `test_worker_starts_and_stops` — verifies worker thread spawn, MTA initialization, and clean channel shutdown.
- [x] `test_worker_list_servers` — verifies `ComRequest::ListServers` dispatch and server list reply.
- [x] `test_worker_list_server_details` — verifies `ComRequest::ListServerDetails` dispatch with metadata attributes.
- [x] `test_worker_read_tag_values_mismatched_lengths` — verifies defensive check against server returning mismatched item result lengths.
- [x] `test_worker_write_tag_value` — verifies single tag writing success path via ephemeral group.
- [x] `test_worker_write_tag_value_failure` — verifies single tag writing failure mapping to `WriteResult`.
- [x] `test_connection_cache_reuse` — verifies connection caching by `ServerIdentifier` across repeated operations.
- [x] `test_dispatch_cache_hit_avoids_reconnect` — verifies connection cache hits bypass connector reconnection.
- [x] `test_dispatch_connection_error_evicts_and_reconnects` — verifies RPC failure triggers cache eviction, reconnect, and retry.
- [x] `test_dispatch_non_connection_error_does_not_evict` — verifies non-connection errors preserve cached connection.
- [x] `test_worker_active_group_caching_hit_miss_and_invalidation` — validates active OPC group caching hit/miss semantics and invalidation on tag set changes or connection drops.
- [x] `test_circuit_breaker_dual_phase_and_endpoint_isolation` — validates 5-second failure cooldown circuit breaker on unreachable host endpoints.
- [x] `test_handle_write_batch_partial_failures_and_ordering` (in `write.rs`) — validates native batch write execution with partial failure mapping.
- [x] `test_worker_native_write_batch_via_com_request` (in `tests.rs`) — verifies batch write request routing through COM worker channel.
- [x] `test_handle_write_success` (in `write.rs`) — verifies single tag write handler execution.
- [x] `test_handle_read_empty_tags_short_circuits` (in `read.rs`) — verifies empty tag batch read immediately short-circuits.
- [x] `test_handle_read_with_mock_server` (in `read.rs`) — verifies read request execution against mock server connector.
- [x] `test_handle_browse_harvests_tags` (in `browse.rs`) — verifies browse operation incrementally populates tag collector.
- [x] `test_handle_browse_cancelled_returns_harvest` (in `browse.rs`) — verifies browse cancellation returns harvested partial tags.
- [x] `test_collision_proof_group_name_concurrency` (in `worker.rs`) — verifies PID and atomic nonce concurrency in group naming.
- [x] `test_stale_connection_eviction` — verifies RPC failure triggers cache eviction, reconnect, and successful retry.
- [x] `test_worker_panic_propagation` — verifies worker thread panic detection on subsequent client requests.
- [x] `test_worker_thread_recovery_after_panic` — validates worker thread restarts cleanly and processes subsequent requests after a caught panic.
- [x] `test_drop_during_active_request` — verifies channel disconnection behavior when client worker is dropped.
- [x] `test_worker_init_failure` — verifies error propagation when thread COM initialization fails.
- [x] `test_worker_read_tag_values_quality_decoding` — validates in-place read decoding of Good, LocalOverride, CommFailure, and EGU limits.
- [x] `test_worker_com_init_failure_propagates_opc_error` — verifies custom initializer error propagation.
- [x] `test_worker_browse_tags_success` — verifies `ComWorker` tag discovery over hierarchical namespace using `MockServerConnector`.
- [x] `test_worker_browse_tags_cancelled` — verifies `ComWorker` immediate return when `TagCollector` is cancelled prior to execution.
- [x] `test_worker_browse_tags_capacity_cap` — verifies `ComWorker` tag accumulation halts when `TagCollector` capacity is reached.
- [x] `test_worker_browse_tags_flat_organization` — verifies fast leaf browsing when server namespace organization is flat.
- [x] `test_worker_tracing_instrumentation_execution` — validates worker tracing span activation and logging.
- [x] `test_group_guard_cleanup_on_drop` and `test_group_guard_disarm_prevents_cleanup` — verifies worker group guard drop cleanup.
- [x] `test_worker_handle_read_error_cleans_group` — validates group removal even when `read` returns an error.
- [x] `test_worker_channel_drop_error_propagation` — verifies channel drop error handling.

### Discovery & Registry Inspection Unit Tests (in `com/discovery.rs`)

- [x] `test_inspect_local_registration_remote_rejected` — verifies `inspect_local_registration` cleanly rejects remote machine addresses with `OpcError::NotImplemented`.
- [x] `test_sanitize_binary_path_quoted` — verifies `sanitize_binary_path` strips surrounding double quotes from registry image paths.
- [x] `test_sanitize_binary_path_unquoted_with_flag` — verifies `sanitize_binary_path` strips trailing CLI flags (`-Embedding`, `/automation`).
- [x] `test_opc_server_type_display` — verifies `OpcServerType` Display formatting (`LocalServer32 (Executable)` vs `InprocServer32 (DLL)`).
- [x] `test_open_reg_key_invalid` — verifies `open_reg_key` returns Windows error code when querying a non-existent registry subkey.
- [x] `test_expand_environment_string` — verifies `expand_environment_string` resolves embedded `%VAR%` tokens via `windows::Win32::System::Environment::ExpandEnvironmentStringsW`, preserves unassigned tokens and unmatched `%`, handles empty strings, and dynamically reallocates on oversized paths (> 512 wide chars).
- [x] `test_inspect_local_registration_nonexistent_returns_classnotreg` — verifies `inspect_local_registration` maps non-existent CLSIDs to canonical `OpcError::Com(REGDB_E_CLASSNOTREG)`.
- [x] `test_guid_to_progid_zeroed_guid_returns_com_error` — verifies structured COM error preservation on zeroed GUID.

### COM VARIANT Unit Tests (in `com/variant.rs`)

- [x] `test_opc_value_to_variant_int`, `test_opc_value_to_variant_float`, `test_opc_value_to_variant_bool_true`, `test_opc_value_to_variant_bool_false`, `test_opc_value_to_variant_string` — verifies typed `OpcValue` to COM `VARIANT` conversions.
- [x] `test_variant_roundtrip` — validates lossless roundtrip conversions across all basic types (`Int`, `Float`, `Bool`, `String`, `Empty`, `Null`).
- [x] `test_variant_to_string_cy` — validates 64-bit fixed-point Currency (`VT_CY`) scaling and formatting.
- [x] `test_variant_to_string_empty` and `test_variant_to_string_null` — validates Empty and Null variant rendering.
- [x] `test_variant_to_string_i2_and_r4` — validates 16-bit integer and single-precision float formatting.
- [x] `test_variant_to_string_unknown_vt` — verifies fallback formatting for unrecognized VARENUM types.
- [x] `test_variant_to_string_safearray_i4` — validates 1-D SafeArray traversal and formatting.
- [x] `test_safearray_unpacking_buffer_bounds_canary_protection` — validates bounds canary protection and defensive clamping during SafeArray buffer unpacking.
- [x] `test_variant_to_string_vt_error_known` and `test_variant_to_string_vt_error_unknown` — validates `VT_ERROR` HRESULT diagnostic mapping.
- [x] `test_scoped_variant_empty_and_as_raw_mut` — verifies `ScopedVariant::empty()` initializes with `VT_EMPTY` and `as_raw_mut` provides valid pointer.
- [x] `test_scoped_variant_drop_clears_bstr_and_resets_vt` — verifies `ScopedVariant` drop safely invokes `VariantClear`, freeing `BSTR` without memory leaks.
- [x] `test_scoped_variant_into_inner_disarm` — verifies `ScopedVariant::into_inner` disarms the RAII guard and preserves internal `VARIANT`.
- [x] `test_item_states_guard_drop_clears_variants` — verifies `ItemStatesGuard` drop iterates all `tagOPCITEMSTATE` elements and safely clears `vDataValue`.
- [x] `test_item_states_guard_partial_failure_s_false_uninitialized_safety` — verifies `ItemStatesGuard` safety when server returns `S_FALSE` or uninitialized VARIANTs.
- [x] `test_item_results_blob_guard_frees_blobs` — verifies `ItemResultsBlobGuard` safely cleans up unmanaged blob allocations on drop.

### Error & Diagnostic Unit Tests (in `errors.rs`)

- [x] `test_opc_error_friendly_hint` — verifies `friendly_hint` returns `None` for non-COM errors and expected text for known COM errors.
- [x] `test_friendly_hint_known_codes` — verifies HRESULT hints for known codes (`RPC_S_CALL_FAILED_DNE`, `REGDB_E_CLASSNOTREG`, `OPC_E_BADRIGHTS`, `OPC_E_BADTYPE`, `OPC_E_UNKNOWNITEMID`, `OPC_E_INVALIDITEMID`).
- [x] `test_friendly_hint_unknown_code` — verifies `None` on unknown or internal error codes.
- [x] `test_is_connection_error` — verifies `is_connection_error` classification for transport failure HRESULTs.
- [x] `test_com_error_display_formatting` — verifies `Display` formatting for `OpcError::Com` with HRESULT and friendly hint.
- [x] `test_opc_operation_display` — validates canonical string formatting across all `OpcOperation` enum variants.
- [x] `test_log_opc_err_macro` — validates structured key-value emission and diagnostic capture via `log_opc_err!`.
- [x] `test_channel_error_conversions_and_lock_poison` — verifies `From` conversions for `mpsc::RecvError`, `oneshot::RecvError`, `SendError`, and `PoisonError` to `OpcError::Internal`, and `OpcError::connection_failed`.

### Raw Memory Safety Unit Tests (in `raw/memory.rs` and `raw/bridge.rs`)

- [x] `test_remote_array_safety_and_invariants` — verifies zero-allocation remote array creation, safe move-only drop semantics, and heap integrity without `Clone`.
- [x] `test_remote_pointer_into_string_raii_safety` — verifies `RemotePointer<u16>::into_string` converts valid UTF-16, rejects null pointers with `OpcError::Com`, and automatically cleans up unmanaged COM memory via `CoTaskMemFree`.
- [x] `test_remote_pointer_copy_slice_empty_and_valid` — verifies safe slice copying from remote COM pointers.
- [x] `test_local_pointer_no_box_indirection` — verifies local COM pointer unmanaged allocation without Box indirection.
- [x] `test_bridge_borrowed_blob_no_double_free` — verifies `BlobGuard` safely borrows memory without double-freeing on drop.

### Library & Re-Export Unit Tests (in `lib.rs`)

- [x] `test_parse_quality_error_reexport` — verifies `ParseQualityError` is exposed at crate root and implements `std::error::Error`.
- [x] `test_opc_da_client_builder_reexport` — verifies `OpcDaClientBuilder` is exposed at crate root.

### Mock-Based Tests (in `opc-cli` — 49 Unit Tests)

- [x] `MockOpcProvider` returns expected server list.
- [x] `MockOpcProvider` returns expected browse results.
- [x] `MockOpcProvider` returns expected tag values.
- [x] `MockOpcProvider` simulates error conditions for UI error handling.
- [x] `test_destructure_tag_value_ergonomics` — verifies destructuring of `TagValue` with `v.value.display()` and `v.timestamp.display()`.
- [x] `test_browse_tags_collector_timeout_and_cancellation` — verifies cooperative cancellation and partial harvesting on timeout in TUI task.
- [x] `test_write_value_parsing_and_boolean_coercion` — validates context-aware boolean coercion and fallback string parsing in `App::resolve_write_value`.
- [x] **Screen State Transitions (12 tests)** — verifies state machine transitions across `Home`, `Loading`, `ServerList`, `TagList`, `TagValues`, `WriteInput`, and `Exiting`.
- [x] **TUI Keyboard Navigation & Selection (10 tests)** — verifies `Up`, `Down`, `PageUp`, `PageDown`, `Home`, `End`, and wrap-around cursor tracking in list and table widgets.
- [x] **Interactive Search & Filtering (5 tests)** — verifies substring search filter, live matches count, and `Tab`/`Shift+Tab` cycling.
- [x] **Async Polling Loops & Background Task Resiliency (5 tests)** — verifies background server enumeration, tag browsing timeout cancel, and 1-second auto-refresh polling loop.
- [x] **Deconstructed App Sub-States & Actions (10 tests)** — verifies `App::handle_key` returning `AppAction`, `DialogState` input buffering, `AutoRefresher` tick and toggle mechanics, and status bar telemetry counters (`error_count` vs `bad_quality_count`).
- [x] **Zero-Allocation Rendering (7 tests)** — validates `[Cell; 4]` stack array row generation and ANSI highlight styling in `ui.rs`.

### Doc Tests (78 Tests in `opc-da-client`: 77 Passed, 1 Ignored, 2 Compile-Fail)

- [x] `OpcError::friendly_hint`, `OpcError::connection_failed`, `OpcError::is_connection_error` — runnable doctests in `errors.rs`.
- [x] `OpcResult`, `OpcError` — runnable doctests in `errors.rs`.
- [x] `TagValue`, `TagValue::is_good`, `TagResult`, `OpcValue`, `WriteResult`, `DisplayOption*`, `OpcValueOptionExt`, `SystemTimeOptionExt` — runnable doctests in `types/collection.rs`, `types/value.rs`, `types/collector.rs`.
- [x] `TagBatch` methods (`len`, `is_empty`, `iter_str`, `into_vec`) — runnable doctests in `types/batch.rs`.
- [x] `TagValues` methods (`new`, `len`, `is_empty`, `get`, `get_value`, `get_as`, `get_f64`, `get_f32`, `get_i32`, `get_i64`, `get_u32`, `get_u64`, `get_bool`, `get_str`, `into_vec`, `as_slice`, `iter`) — runnable doctests in `types/collection.rs`.
- [x] `OpcServerEndpoint` methods (`local`, `remote`, `is_remote`) and host normalization functions (`normalize_host_str`, `normalize_host`, `is_remote_host`) — runnable doctests in `types/server.rs`.
- [x] `OpcDaClientBuilder` methods (`new`, `with_legacy_dcom`, `host`, `server`, `timeout`, `build`, `build_bound`) — runnable doctests in `com/client.rs`.
- [x] `OpcDaClient` constructors & inherent methods (`builder`, `connect`, `connect_remote`, `bind`, `bind_remote`, `read_tag_values`, `read_f64`, `read_i32`, `read_bool`, `read_string`, `write`, `write_batch`, `subscribe`) — runnable doctests in `com/client.rs`.
- [x] `OpcProvider` trait methods (`list_servers`, `browse_tags`, `read_tag_value`, `read_tag_values`, `write_tag_value`, `write_tag_values`) — runnable doctests in `provider.rs` backed by `MockOpcProvider` assertions.
- [x] `ServerGroupHandle`, `ServerItemHandle`, `OpcQuality`, `BrowseType`, `BrowseDirection` — runnable doctests in `types/handles.rs`, `types/quality.rs`, `types/browse.rs`.
- [x] `ServerGroupHandle` and `ServerItemHandle` compile-fail non-interchangeability doctests in `types/handles.rs`.
- [x] `ComGuard` — internal-only ignored doctest in `com/guard.rs`.
- [x] Quick Start & Usage Examples (Listing, Reading, Writing, Browsing, Typestates) — runnable doctests in `lib.rs` and `README.md`.

### Integration Test Suites (4 Suites in `opc-da-client/tests/`)

- [x] `batch_write_test` — validates multi-item atomic COM group batch write transactions, partial item error handling, and `WriteResult` status mapping.
- [x] `handle_type_safety_test` — validates opaque newtype wrappers `ServerGroupHandle`, `ServerItemHandle`, `ClientGroupHandle`, `ClientItemHandle` enforcing strict compile-time non-interchangeability.
- [x] `mock_contract_stability_test` — validates `MockOpcProvider` and `MockServerConnector` contract fidelity across all segregated role traits (`ServerDiscovery`, `TagBrowser`, `TagReader`, `TagWriter`).
- [x] `typestate_client_test` — validates compile-time `OpcDaClient<C, Unbound>` to `OpcDaClient<C, Bound>` state transitions via `bind`, `bind_remote`, and `unbind`, verifying infallible endpoint access on `Bound`.

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

