# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.2] - 2026-02-22

### Changed
- Updated `friendly_com_hint` E_POINTER text — removed stale iterator blame.
- Fixed broken `architecture.md`/`spec.md` links in README for crates.io.
- Added 3 missing HRESULT mappings (`OPC_E_BADTYPE`, `OPC_E_UNKNOWNITEMID`, `OPC_E_INVALIDITEMID`) to documentation.

## [0.1.1] - 2026-02-22

### Fixed
- **OPC-BUG-001**: Eliminated `StringIterator` phantom `E_POINTER` errors at the source (cache zeroing + null-PWSTR skip).
- Removed `is_known_iterator_bug()` workaround — no longer needed.

### Changed
- Write rejection log downgraded from `error!` to `warn!` (handled failure).
- Added `operation` context field to OPC group cleanup warnings.
- Added `info!`-level success logs to `list_servers`, `browse_tags`, `read_tag_values`.

## [0.1.0] - 2026-02-22

### Added

- **`OpcProvider` trait** — async trait with `list_servers`, `browse_tags`, `read_tag_values`, and `write_tag_value`.
- **`OpcDaWrapper`** — native OPC DA backend via `windows-rs` COM calls. Generic over `ServerConnector` for testability.
- **`ComGuard`** — RAII guard for COM MTA initialization/teardown.
- **`friendly_com_hint()`** — maps 11 known COM/DCOM HRESULT codes to actionable user hints.
- **`format_hresult()`** — formats `HRESULT` as `0xHHHHHHHH: <hint>` for consistent error messages.
- **`TagValue` / `OpcValue` / `WriteResult`** — core data types for read/write operations.
- **`MockOpcProvider`** — optional mock via `test-support` feature flag for downstream testing.
- **`VT_ERROR` handling** — `variant_to_string` now correctly parses and displays error-type VARIANTs.
- **Resource cleanup guarantees** — OPC groups are always removed via `remove_group`, even on error paths.
- **1-to-1 read contract** — `read_tag_values` returns exactly one `TagValue` per requested tag, with error sentinels for failed items.
- **SafeArray display** — array VARIANTs show up to 20 element values instead of opaque `Array[N]`.
- **Comprehensive variant support** — VT_EMPTY, VT_NULL, VT_I2, VT_I4, VT_R4, VT_R8, VT_CY, VT_DATE, VT_BSTR, VT_ERROR, VT_BOOL, VT_I1, VT_UI1, VT_UI2, VT_UI4, VT_I8, VT_UI8, and VT_ARRAY composites.

### Fixed

- Silent tag dropping when `add_items` partially fails during read operations.
- Resource leaks when group creation succeeds but subsequent operations fail.
- Opaque HRESULT formatting in user-facing error messages.
