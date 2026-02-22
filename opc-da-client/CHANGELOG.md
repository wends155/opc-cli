# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-02-23

### Added
- Natively integrated and froze raw `windows-bindgen` outputs into an internal `bindings/` module, dropping required build-time codegen dependencies.

### Changed
- **Major Architectural Refactor**: Completely merged and eliminated the external `opc_da`, `opc_da_bindings`, and `opc_comn_bindings` crate dependencies. The `opc-da-client` is now entirely self-contained for OPC COM communication natively.
  - **Pros vs v0.1.3**: 
    - **Build Velocity**: Eliminates the intermediate `windows-bindgen` code-generation step entirely.
    - **Security & Maintenance**: Severs reliance on abandoned upstream crates. All `unsafe` COM pointers are now directly subject to workspace native `clippy` gating.
    - **Testability**: Enabled the injection of `MockServerConnector`, allowing CI pipelines to organically test application logic without requiring a physical Windows DCOM environment.
- Extracted the primary `OpcDaWrapper` logic and stabilized its initialization footprint beneath the unified `OpcDaClient` identifier.
- Synchronized architectural documentation, replacing obsolete `anyhow::Result` references with the active `OpcResult` type.
  - **Context:** The library migrated away from `anyhow` (an application-level, type-erased error) to `thiserror` (`OpcError`) to adhere to standard Rust library best practices. 
  - **Compatibility:** Because `OpcError` implements `std::error::Error`, downstream consumers (like `opc-cli`) using `anyhow` can still perfectly propagate `OpcResult` using the standard `?` operator.

### Fixed
- Patched integration doc-tests within `README.md` with `no_run` attributes.
  - **Context:** `cargo test --doc` natively executes all Rust codeblocks found in markdown files. Because the `README.md` snippets demonstrate live COM initializations and connections to the Matrikon OPC Simulation server, running them in raw CI pipelines (where the simulator isn't installed) caused systemic test failures. `no_run` ensures the code is still statically analyzed for compilation correctness but safely skips the runtime execution.

## [0.1.3] - 2026-02-22

### Fixed
- **docs.rs build failure**: Added `[package.metadata.docs.rs]` with `default-target = "x86_64-pc-windows-msvc"` so documentation builds correctly on crates.io.

### Changed
- Enabled `all-features = true` for docs.rs builds, making `MockOpcProvider` visible in the rendered API docs.
- Synced all documentation with OPC_FLAT browse optimization from v0.1.2 audit.

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
- **`OpcDaClient`** — native OPC DA backend via `windows-rs` COM calls. Generic over `ServerConnector` for testability.
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
