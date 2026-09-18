# OPC DA Client CLI

[![Crates.io](https://img.shields.io/crates/v/opc-cli.svg)](https://crates.io/crates/opc-cli)
[![Docs.rs](https://docs.rs/opc-cli/badge.svg)](https://docs.rs/opc-cli)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> Interactive TUI and native Windows COM client library for browsing, reading, and writing OPC DA tags

## Overview

`opc-cli` provides a modern, asynchronous Terminal User Interface (TUI) and high-performance native Windows COM client library for inspecting, browsing, reading, and writing OPC Data Access (OPC DA 2.05a) industrial automation tags on Windows.

OPC Data Access is deeply tied to legacy Windows COM and DCOM runtimes, which present significant hurdles in contemporary control systems: strict Multi-Threaded Apartment (MTA) affinity, thread safety constraints, cryptic Win32 HRESULT errors, and mandatory Windows DCOM hardening (KB5004442).

This project resolves these challenges by isolating all low-level COM and DCOM interactions behind a dedicated MTA background worker thread and pure-Rust trait abstractions. Operators and engineers can interactively navigate complex PLC tag trees, inspect decomposed 16-bit OPC qualities, and diagnose field telemetry via an ergonomic, zero-overhead terminal dashboard.

## Installation

### Prerequisites

- **Windows OS**: Windows 10+ / Server 2016+ (or Windows 7 SP1 / Server 2008 R2 SP1 with legacy bundle).
- **OPC Core Components**: Must be installed on the system to resolve OPC ProgIDs and browse server catalogs.
- **Rust 1.93+**: Required when compiling from source (Rust Edition 2024).

### Building from Source

Install directly via Cargo:

```powershell
cargo install --path opc-cli
```

Or build the optimized release binary locally:

```powershell
cargo build --release --bin opc-cli
```

The compiled binary will be located at `target/release/opc-cli.exe`.

### Pre-built Releases & Packaging

The repository supports two automated release packaging targets:

#### 1. Modern Release (Windows 10+ / Server 2016+)

```powershell
make package
# OR
pwsh -File scripts/package.ps1 package
```
Output: `dist/opc-cli-x64/` and `dist/opc-cli-x64.zip`

#### 2. Legacy Release (Windows 7 SP1 / Server 2008 R2 SP1)

For deployment to offline, air-gapped industrial environments running Windows 7 / Server 2008 R2 (NT 6.1):

```powershell
make package-win7
# OR
pwsh -File scripts/package.ps1 package-win7
```
Output: `dist/opc-cli-win7-x64/` and `dist/opc-cli-win7-x64.zip`

**Legacy Bundle Contents:**
- `opc-cli.exe`: PE-patched executable linked with static CRT (`+crt-static`). Replaces missing `GetSystemTimePreciseAsFileTime` imports with native `GetSystemTimeAsFileTime`.
- `api-ms-win-core-synch-l1-2-0.dll`: `#![no_std]` polyfill for `WaitOnAddress` and `Sleep` re-export.
- `api-ms-win-core-winrt-error-l1-1-0.dll`: `#![no_std]` no-op stubs for WinRT error APIs.
- `bcryptprimitives.dll`: `#![no_std]` polyfill routing `ProcessPrng` to `RtlGenRandom` (`advapi32.dll`).
- `redist/`: Included OPC Core Components redistributable MSI (if placed in `vendor/redist/`).

## Usage / Quick Start

Launch the interactive TUI directly:

```powershell
# Run the interactive TUI
cargo run --bin opc-cli

# Run with debug logging enabled (default is info)
cargo run --bin opc-cli -- -v

# Run with verbose trace logging enabled (captures detailed argument dumps)
cargo run --bin opc-cli -- -vv
```

### Keyboard Controls

| Key | Action | Screen |
| :--- | :--- | :--- |
| `Enter` | Navigate forward / Confirm input | All |
| `Esc` | Navigate back | All |
| `Space` | Toggle tag selection | Tag List |
| `s` | Enter search/filter mode | Tag List |
| `Tab` / `Shift+Tab` | Cycle through search matches | Tag List (search) |
| `w` | Enter write mode for selected tag | Tag Values |
| `↑` / `↓` | Navigate lists | All lists |
| `PgUp` / `PgDn` | Page through lists (20 items) | All lists |
| `q` / `Q` | Quit application | Home |

## Features / Feature Flags

- **Server Discovery & UNC Endpoints**: Enumerate OPC DA servers on local hosts and remote OPCEnum catalogs via UNC syntax (`\\host\server` or `\\host\{CLSID}`).
- **Typestate Client, Liveness Ping & COM Isolation**: Zero-cost compile-time `Unbound` (gateway) and `Bound` (session) typestates with direct `connect` / `build_bound` shortcuts, eager liveness probe (`connect_eager`), dedicated MTA apartment worker thread, and automatic Windows KB5004442 packet integrity security blanketing.
- **Hierarchical Browsing**: Recursive exploration of complex server namespaces with cooperative cancellation and partial-result harvesting on timeout.
- **Real-time Monitoring & Active Group Caching**: Live tag value updates with 1-second auto-refresh backed by active OPC group pooling (>75% lower DCOM RPC latency) and auto-recovery on group invalidations.
- **Zero-Allocation Batch Reads & Typed Values**: Universal `IntoTags` tag batches (`read_tags`, `read_tag`), inherent numeric accessors (`read_f32`, `read_i64`, `read_u32`, `read_u64`), rich `TagValues` collection with generic typed extraction (`get_as<T>`), numeric getters, and encapsulated `TagValue` read outcomes.
- **Single & Batch Write Support (`WriteBatch`)**: Native atomic zero-allocation batch writes (`write_tags`) via polymorphic `WriteBatch` (`Single`, `Shared`, `Owned`) and individual typed tag writes (`write_tag`).
- **Streaming Subscriptions**: Non-blocking Layer 2 subscription streams yielding `TagValues` updates over Tokio `mpsc` channels with automatic drop cancellation.
- **Search & Filter**: Substring search with `Tab`/`Shift+Tab` cycling through matches.
- **Rich Error Hints & Lossless Diagnostics**: Human-readable explanations for cryptic Windows COM/DCOM HRESULT codes and structured tag attribution errors.
- **Transparent COM Management**: COM initialization, MTA apartment affinity, stale proxy eviction, and deterministic thread teardown handled automatically by a dedicated background worker thread.
- **Mockable Backend & Pure-Rust SPI**: Segregated role mocks (`MockTagReader`, etc.) and pure-Rust Tier 2 SPI connector (`opc_da_client::connector::*`) for testing on any operating system without Windows COM runtimes.

### Feature Flags

| Flag | Package | Default | Description |
|:---|:---|:---:|:---|
| `dev-diagnostics` | `opc-cli`, `opc-da-client` | ❌ No | Compiles verbose `TRACE`-level argument dumps into backend methods for low-level protocol debugging. |
| `opc-da-backend` | `opc-da-client` | ✅ Yes | Compiles the native Windows COM backend (`OpcDaClient` and `ComConnector`). |
| `test-support` | `opc-da-client` | ❌ No | Enables `mockall` role mocks (`MockOpcProvider`, `MockTagReader`, etc.) for downstream test suites. |

### Remote OPC DA (DCOM) Status & Architecture Guidance

> [!WARNING]
> **DCOM Implementation Status (0.3.0):**
> Full end-to-end Remote DCOM operation is on the roadmap ([`long_term_todo.md`](long_term_todo.md) Phase 5) and is **NOT yet supported for production in 0.3.0**.
>
> **For remote industrial connectivity today, it is strongly recommended to use OPC UA** (via an OPC UA gateway or wrapper like Kepware, Matrikon OPC UA Tunneller, or Prosys OPC) rather than legacy DCOM. OPC UA communicates over standard TCP/IP (port 4840), traverses firewalls natively, and eliminates the brittle Windows security, RPC port exhaustion, and DCOM hardening (KB5004442) challenges inherent to remote COM.
>
> When using OPC DA, running `opc-cli` directly on the Windows machine hosting the OPC DA server (local COM activation) is the recommended and fully supported deployment model.

#### What Works Today (0.3.0)
- **Local OPC DA Connections**: 100% stable, hardened, fully tested local COM operation on the same machine.
- **Remote Catalog Discovery**: Queries `OPCEnum` (`IOPCServerList`/`IOPCServerList2`) on remote hosts via DCOM `CoCreateInstanceEx` to enumerate available servers and CLSIDs.
- **Direct CLSID Remote Activation**: Low-level library activation via `CoCreateInstanceEx` (`CLSCTX_REMOTE_SERVER`) when given an explicit bracketed CLSID (e.g. `\\host\{CLSID}`), applying Windows KB5004442 `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY` packet integrity and proxy blanketing across primary server and group interfaces.

#### What Does NOT Work Yet (Roadmap — Phase 5)
- **Remote ProgID Resolution**: Connecting via remote ProgID (e.g. `\\host\Matrikon.OPC.Simulation.1`) queries the *local* registry for the ProgID-to-CLSID mapping. If the OPC server is not installed locally, resolution fails with `CO_E_CLASSSTRING` (`0x800401F3`). *(Phase 5 will add dynamic remote ProgID resolution via `IOPCServerList::CLSIDFromProgID` on remote `OPCEnum`)*.
- **Interactive TUI Remote Browsing**: In `opc-cli`, remote host server listing works, but selecting a server drops the host context and attempts to browse/monitor tags on `localhost`. *(Phase 5 will propagate UNC host endpoints across all TUI screens)*.
- **Remote Enumerator Proxy Blanketing**: `BrowseOPCItemIDs` returns an `IEnumString` that currently lacks DCOM proxy blanketing, which can trigger `E_ACCESSDENIED` (`0x80070005`) on hardened Windows systems.
- **Custom Windows Credentials**: No CLI flags or dialogs to supply alternative domain/user credentials (`COAUTHIDENTITY`) for cross-machine authentication.

## Architecture

The project is structured as a Cargo workspace with two core crates:

- **`opc-cli`**: The interactive TUI application built with `ratatui` + `crossterm`.
- **`opc-da-client`**: A high-performance native Windows COM/DCOM library (using `windows-rs`) featuring a fluent client API, zero-allocation tag batches, active group caching, native batch write, non-blocking subscription streams, and Windows KB5004442 packet integrity hardening. Abstracts OPC DA communication through the async `OpcProvider` trait, generic over `ServerBackend` for seamless test mocking.

See **[architecture.md](architecture.md)** for workspace architecture, and **[opc-da-client architecture.md](opc-da-client/architecture.md)** for the library design, state machine, and data flow diagrams.

## Contributing

Contributions are welcome! Before submitting pull requests, ensure your changes pass the universal verification gate:

```powershell
# Run the full quality verification gate (format → clippy → test)
pwsh -File scripts/verify.ps1
```

All contributions must adhere to the project's zero-exit and strict clippy standards (`-D warnings`).

## License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

<!-- custom:start -->
Simply copy the extracted `dist/opc-cli-win7-x64/` folder to a USB drive and run on the target machine without installing Visual C++ redistributables or Windows updates.

## 🙏 Acknowledgments

- [**rust_opc**](https://github.com/Ronbb/rust_opc) by Wang Ruobiao — original OPC DA Rust bindings and COM interface generation pipeline.
- [**OPC Foundation**](https://opcfoundation.org/) — OPC Data Access specification and IDL interface definitions.
- [**windows-rs**](https://github.com/microsoft/windows-rs) by Microsoft — Windows API bindings for Rust.
- [**ratatui**](https://github.com/ratatui/ratatui) — terminal user interface framework.
<!-- custom:end -->
