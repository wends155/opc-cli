# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed (Breaking)
- **Strongly-Typed Tag Quality**: `TagValue.quality` changed from `String` to strongly-typed `OpcQuality` struct decomposing the 16-bit OPC DA quality word into `QualityMajor`, `QualitySubstatus`, `QualityLimit`, and `raw: u16`. Prevents type erasure, avoids heap allocations on the polling hot-path, and enables idiomatic matching and inspection for industrial SCADA and gateway consumers.

### Added
- **16-Bit OPC DA Quality Decomposition (`opc-da-client`)**: Added `OpcQuality`, `QualityMajor`, `QualitySubstatus`, and `QualityLimit` in `types.rs` with `From<u16>`, `From<OpcQuality> for u16`, `From<&str>`, and rich `Display` implementation formatting substatus and limits (e.g. `"Good (Local Override)"`, `"Bad (Comm Failure)"`, `"Uncertain (EGU Exceeded) [High Limited]"`).
- **Mock Read Integration Test**: Added `test_worker_read_tag_values_quality_decoding` to `com::worker` verifying end-to-end multi-quality decoding and per-item rejection handling.

### Deprecated
- **`helpers::quality_to_string`**: Deprecated in favor of `OpcQuality::from(quality).to_string()` or direct inspection of `TagValue.quality`. Removal targeted for v0.4.0.

## [0.2.1] - 2026-08-12

### Fixed
- **docs.rs Build Failure**: Added `cargo-args = ["--bin", "opc-cli"]` to `[package.metadata.docs.rs]` in `opc-cli/Cargo.toml` so docs.rs explicitly targets the binary executable (`src/main.rs`) instead of defaulting to a non-existent `--lib` target.

## [0.2.0] - 2026-08-12

### Added
- **crates.io Publication**: Published `opc-cli` and `opc-da-client` to `crates.io`.
- **Standalone `windows-rs` Integration**: `opc-da-client` fully internalized raw `windows-bindgen` COM definitions into `src/bindings/`, eliminating external unmaintained bindings crates.
- **Windows 7 / Server 2008 R2 Polyfill Bundle**: Built legacy release packaging pipeline (`package-win7`) producing static PE binaries bundled with `#![no_std]` polyfills (`WaitOnAddress`, `RtlGenRandom`, WinRT error stubs).
- **Quality Pipeline & Automation**: Added PowerShell-driven automated quality verification (`verify.ps1`), release packager (`package.ps1`), and clean merge workflow (`Merge-ToMain.ps1`).

### Changed
- Synchronized workspace versions for `opc-cli` and `opc-da-client` at `0.2.0`.
- Internalized `ComGuard` into `opc-da-client` crate-private scope. COM MTA apartment initialization and thread affinity are managed transparently by a background worker thread.
- Enforced LF line endings across Rust source files with `.gitattributes` and `rustfmt.toml`.
- Replaced machine-specific MSVC linker overrides in `.cargo/config.toml` with standard Cargo MSVC toolchain auto-discovery.

### Fixed
- Fixed phantom `E_POINTER` error cascades from null PWSTR values in `StringIterator` (OPC-BUG-001).
- Prevented potential silent array truncation in OPC DA tag read/write handling.
