# opc-da-client

[![Crates.io](https://img.shields.io/crates/v/opc-da-client.svg)](https://crates.io/crates/opc-da-client)
[![Docs.rs](https://docs.rs/opc-da-client/badge.svg)](https://docs.rs/opc-da-client)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> Backend-agnostic OPC DA client library for Rust — async, trait-based, with transparent COM management.

## Overview

`opc-da-client` provides a high-performance, asynchronous Rust client for communicating with OPC Data Access (OPC DA 2.05a) servers on Windows.

OPC DA is deeply coupled to Windows COM/DCOM, which poses significant architectural hurdles in modern systems: strict Multi-Threaded Apartment (MTA) threading requirements, thread affinity, raw memory allocations (`CoTaskMemAlloc`), and cryptic HRESULT error codes. 

`opc-da-client` solves these challenges by isolating all Win32 COM and DCOM interactions behind a **pure-Rust connector facade** and a dedicated MTA background worker thread. Callers interact exclusively with safe, strongly-typed asynchronous Rust traits and domain models without writing a single line of `unsafe` code.

> [!IMPORTANT]
> ### 🚀 Complete 0.3.0 Architectural Modernization
> Version 0.3.0 is a ground-up architectural refactoring of `opc-da-client`, modernizing the library to native Rust 2024 standards:
> - **Pure-Rust Tier 2 SPI Connector (`opc_da_client::connector::*`)**: Strict isolation of low-level Win32 COM and FFI types behind pure-Rust trait interfaces (`ServerConnector`, `ConnectedServer`, `ConnectedGroup`) with associated `type ItemIterator`. Enables 100% offline unit and integration mock testing on any platform (Linux, macOS, Windows) without requiring Windows COM runtimes or the `opc-da-backend` feature flag.
> - **Native Rust 2024 AFIT Traits**: Replaced legacy `#[async_trait]` macros across all segregated role traits (`ServerDiscovery`, `TagBrowser`, `TagReader`, `TagWriter`), returning native `impl Future<Output = ...> + Send` with zero heap allocation indirection.
> - **Compile-Time Typestate Client (`OpcDaClient<C, State>`)**: Typestate transitions from `Unbound` (gateway for multi-server discovery) to `Bound` (dedicated session with infallible endpoint routing), eliminating endpoint mismatches at compile time.
> - **Active Group Caching & Transparent Auto-Recovery**: Automatically pools active OPC groups and item handles on repeated read cycles (reducing DCOM round-trip overhead by >75%), with transparent single-attempt invalidation and auto-recovery on server-side group errors (`0xC0040001`).
> - **Eager Server Liveness Probe (`connect_eager`)**: Actively probes server responsiveness on connection via high-priority `Ping` dispatch to fail fast on dead servers before entering cyclic polling loops.
> - **Zero-Allocation Batch Operations**: Inherent typed numeric scalar accessors (`read_f64`, `read_i32`, `read_f32`, etc.), polymorphic `IntoTags` and `IntoWriteBatch` accepting static slices, arrays, or vectors without channel heap allocations.


## Features

- **Compile-Time Typestate Client (`OpcDaClient<C, State>`)**: Zero-cost typestates `Unbound` (gateway for discovery) and `Bound` (session for reading/writing), guaranteeing infallible endpoint access during active sessions via `.endpoint(&self)`.
- **Fluent Client API & Direct Connect**: Ergonomic `OpcDaClient::builder()`, direct local shortcut `OpcDaClient::connect(server)`, remote DCOM shortcut `OpcDaClient::connect_remote(host, server)`, and typestate builder `build_bound()`.
- **Eager Server Liveness Probe (`connect_eager`)**: Actively probes remote server responsiveness on connection via `ConnectedServer::ping()` and high-priority `ComRequest::Ping`, detecting unreachable servers upfront before entering cyclic polling loops.
- **Zero-Allocation Batch Reads (`TagBatch` & `IntoTags`)**: Bound `read_tags` and `read_tag` accept static slices (`&["Tag1", "Tag2"]`), fixed-size arrays (`["Tag1", "Tag2"]`), single tag strings, or owned vectors (`Vec<String>`) with zero intermediate allocations.
- **Inherent Typed Numeric Accessors**: Read individual scalar tags directly on `OpcDaClient<Bound>` without manual `Variant` unpacking (`read_f32`, `read_i64`, `read_u32`, `read_u64`, `read_f64`, `read_i32`, `read_bool`, `read_string`).
- **High-Productivity Typed Getters (`TagValues`)**: Safely unwrap typed values on collections (`values.get_f64("Tag")?`, `get_f32`, `get_i32`, `get_i64`, `get_u32`, `get_u64`, `get_bool`, `get_str`) or use generic extraction (`values.get_as::<f64>("Tag")?`) with case-insensitive lookups, preserved diagnostics, and lenient numeric coercion.
- **Active Group Caching & Auto-Retry**: Automatically pools active OPC groups and item handles on repeated read cycles, reducing DCOM round-trip overhead by >75%. Automatically evicts stale groups and retries once upon server-side group invalidations.
- **Native Zero-Allocation Batch Writes (`WriteBatch` & `IntoWriteBatch`)**: Perform single or multiple tag writes in a single COM atomic `SyncIO::Write` operation via `client.write_tag(...)` or bound `client.write_tags(...)` accepting arrays (`[("Tag", val), ...]`), slices (`&[...]`), or vectors without channel heap allocations.
- **Non-Blocking Subscription Streams**: Stream periodic tag readings via `client.subscribe(tags, interval)` returning an asynchronous Tokio `mpsc::Receiver<TagValues>` with RAII drop cancellation.
- **Hardened DCOM Support (KB5004442)**: Enforces `RPC_C_AUTHN_LEVEL_PKT_INTEGRITY` on DCOM proxy blankets with configurable `with_legacy_dcom(true)`. *(Full remote DCOM is on the roadmap and not yet supported in 0.3.0; for remote communication, OPC UA is recommended).*
- **Native Rust 2024 Async Traits**: Built on `tokio` with native async trait methods (`impl Future<Output = ...> + Send`), completely eliminating `async-trait` heap allocations while providing segregated role traits (`ServerDiscovery`, `TagBrowser`, `TagReader`, `TagWriter`) and composite `OpcProvider` for straightforward test mocking.
- **Structured Server Discovery & UNC Endpoints**: Enumerate servers with rich catalog metadata (`OpcServerInfo`, `ProgID`, `CLSID`, user-friendly title) via `list_server_details`. Full support for UNC paths (`\\host\server`) via `OpcServerEndpoint` with automatic localhost normalization.
- **Pure-Rust Tier 2 SPI Connector (`opc_da_client::connector::*`)**: Strict isolation of low-level Win32 COM and FFI types behind pure-Rust trait interfaces (`ServerConnector`, `ConnectedServer`, `ConnectedGroup`) with associated `type ItemIterator`, compiling and enabling full offline test mocking on any platform without requiring Windows COM runtimes or the `opc-da-backend` feature flag.
- **Transparent COM & Thread Management**: Automatically spawns and manages a dedicated MTA worker thread, maintaining strict thread affinity, connection pooling with synchronized eviction on reconnects, deterministic RAII thread join on `Drop`, and RAII group teardown (`GroupGuard`) ensuring clean server cleanup across all return paths and panics.
- **Strongly-Typed Domain Models & Tag Outcomes**: `TagValue` encapsulates reading outcomes as `Result<OpcValue, OpcError>`, quality (`OpcQuality`), and UTC timestamp, with ergonomic accessors (`value()`, `error()`, `outcome()`).
- **Lossless Error Taxonomy**: Structured `ConversionError::TagNotRequested` and `ConversionError::TagNoValue` variants preserve tag identities on collection extraction failures without dropping error diagnostics.
- **Zero-Allocation Display Adapters**: `DisplayOptionOpcValue` and `DisplayOptionTimestamp` adapters with extension traits `OpcValueOptionExt` and `SystemTimeOptionExt` enable zero-allocation formatted streaming with width-padded table alignment.
- **Canonical Display Formatting**: `TagValue` implements `std::fmt::Display` rendering `"{tag_id} = {value} [{quality}] @ {timestamp}"` for clean, single-line logging and diagnostics.
- **16-Bit Quality Decomposition**: Zero-allocation `OpcQuality` struct decomposes raw OPC DA quality words into major status, substatus, and limit states with rich, human-readable diagnostics.
- **Native Windows Backend**: Implemented natively with `windows-rs` — eliminates heavy legacy C++ binaries and external OPC crate dependencies.
- **Context-Rich Error Handling**: Domain-specific `OpcError` via `thiserror` with inherent `.friendly_hint()` method for actionable HRESULT troubleshooting, native `From` conversions for standard channel and sync errors, and RAII unmanaged memory management.
- **Thread-Safe Tag Collection & Cancellation**: `TagCollector` encapsulates bounded accumulation (`max_tags`), lock-free atomic length monitoring, and cooperative cancellation tokens to eliminate worker thread starvation.
- **First-Class Test Support & Role Mocks**: Includes pure-Rust mock implementations (`MockServerConnector`, `MockConnectedServer`, `MockConnectedGroup`, `MockState`) and granular role mocks (`MockServerDiscovery`, `MockTagBrowser`, `MockTagReader`, `MockTagWriter`, as well as composite `MockOpcProvider`) via the `test-support` feature flag.

## Feature Flags

| Flag | Default | Description |
|:---|:---:|:---|
| `opc-da-backend` | ✅ Yes | Compiles the native Windows COM backend (`OpcDaClient` and `ComConnector`). |
| `test-support` | ❌ No | Enables `mockall` support and exports the `MockOpcProvider` mock struct for downstream unit tests. |
| `dev-diagnostics` | ❌ No | Compiles verbose `TRACE`-level argument dumps into backend methods for low-level protocol debugging. |

## Installation

> [!NOTE]
> **Active Development Notice (0.3.0-dev):**
> Version 0.3.0 is the upcoming release currently under active development on the `dev` branch. For the current published stable release on crates.io, use `0.2.0`. To build against the latest 0.3.0 modernization features (such as pure-Rust Tier 2 SPI connector, eager ping, and typestate sessions), reference the repository directly via Git.

Add `opc-da-client` to your `Cargo.toml`:

```toml
[dependencies]
# Target 0.3.0 release (from git during active development):
opc-da-client = { git = "https://github.com/wends155/opc-cli.git", branch = "dev" }

# Once 0.3.0 is published to crates.io:
# opc-da-client = "0.3.0"
```

To enable pure-Rust test mocks for offline unit testing without Windows COM runtimes:

```toml
[dev-dependencies]
opc-da-client = { git = "https://github.com/wends155/opc-cli.git", branch = "dev", features = ["test-support"] }

# Once 0.3.0 is published to crates.io:
# opc-da-client = { version = "0.3.0", features = ["test-support"] }
```

## Prerequisites

- **Operating System**: Windows (COM/DCOM is a Windows-exclusive API).
- **OPC Core Components**: The OPC DA Core Components redistributables must be installed and registered on the machine to resolve OPC server CLSIDs and ProgIDs.
- **DCOM Security (Windows KB5004442)**: Modern Windows updates enforce RPC packet integrity authentication (`RPC_C_AUTHN_LEVEL_PKT_INTEGRITY`) for DCOM activations. `opc-da-client` automatically secures proxy blankets with packet integrity. For legacy environments (e.g. Windows 7 SP1 / Server 2008 R2), call `.with_legacy_dcom(true)` on the builder.

## Usage Examples

### Quick Start: Fluent Connection & Zero-Allocation Reads

Directly connect to an OPC DA server and read tags with zero-allocation slicing and typed extraction:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcResult};

#[tokio::main]
async fn main() -> OpcResult<()> {
    // 1. Connect directly to a local or remote OPC server
    let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;

    // 2. Read tag batch with zero intermediate allocation (accepts arrays, slices, or Vec<String>)
    let values = client.read_tags(["Random.Int4", "Random.Real8", "Random.String"]).await?;

    // 3. Extract strongly typed values with lenient numeric coercion
    let count: i32 = values.get_i32("Random.Int4")?;
    let temp: f64 = values.get_f64("Random.Real8")?;
    let status: &str = values.get_str("Random.String")?;
    println!("Int: {count}, Float: {temp}, Status: {status}");
    Ok(())
}
```

### Eager Server Connection & Liveness Verification

Probe remote server responsiveness immediately on connection via explicit DCOM liveness ping (`ConnectedServer::ping()`), catching unreachable servers upfront:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcResult};

#[tokio::main]
async fn main() -> OpcResult<()> {
    // 1. Bind to server
    let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;

    // 2. Actively probe server liveness via high-priority ComRequest::Ping
    client.connect_eager().await?;
    println!("✓ Successfully connected and verified server liveness");
    Ok(())
}
```

### Inherent Typed Numeric Reads

Directly read individual scalar tags on active bound sessions with automatic type validation without manual `OpcValue` variant extraction:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcResult};

#[tokio::main]
async fn main() -> OpcResult<()> {
    let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;

    // Direct scalar reads on bound session:
    let temp_f32: f32 = client.read_f32("Random.Real4").await?;
    let temp_f64: f64 = client.read_f64("Random.Real8").await?;
    let count_i32: i32 = client.read_i32("Random.Int4").await?;
    let count_i64: i64 = client.read_i64("Random.Int8").await?;
    let uint_32: u32 = client.read_u32("Random.UInt4").await?;
    let uint_64: u64 = client.read_u64("Random.UInt8").await?;
    let flag: bool = client.read_bool("Random.Boolean").await?;
    let text: String = client.read_string("Random.String").await?;

    println!("Temp: {temp_f32}°C (f64: {temp_f64}), Count: {count_i32}, u64: {uint_64}, Flag: {flag}");
    Ok(())
}
```

### Native Batch Writes

Write multiple typed values atomically in a single DCOM roundtrip using arrays, slices, or vectors:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcResult, OpcValue};

#[tokio::main]
async fn main() -> OpcResult<()> {
    let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;

    // Single COM group and single atomic SyncIO::Write roundtrip
    let results = client.write_tags([
        ("Bucket Brigade.Int4", OpcValue::Int(100)),
        ("Bucket Brigade.Real8", OpcValue::Float(99.5)),
    ]).await?;
    for res in results {
        if res.is_success() {
            println!("✓ Wrote tag '{}'", res.tag_id);
        } else {
            println!("✗ Failed tag '{}': {:?}", res.tag_id, res.error());
        }
    }
    Ok(())
}
```

### Streaming Subscriptions (Layer 2)

Stream periodic tag updates using an asynchronous Tokio channel with RAII task cancellation:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcResult};
use std::time::Duration;

#[tokio::main]
async fn main() -> OpcResult<()> {
    let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;

    // Starts background polling task with active group caching (>75% latency reduction)
    let mut rx = client.subscribe(["Random.Int4", "Random.Real8"], Duration::from_millis(500));

    // Receive periodic updates; dropping `rx` cancels the background polling task
    if let Some(values) = rx.recv().await {
        println!("Received {} tag updates", values.len());
        println!("  Int4 = {}", values.get_i32("Random.Int4")?);
    }
    Ok(())
}
```

### Connecting & Listing Servers

Enumerate available OPC DA servers registered on a local or remote host:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcProvider, OpcResult};

#[tokio::main]
async fn main() -> OpcResult<()> {
    let client = OpcDaClient::builder().build()?;
    let servers = client.list_servers("localhost").await?;

    println!("Available Servers:");
    for server in servers {
        println!("  - {}", server);
    }
    Ok(())
}
```

### Structured Server Enumeration & Direct CLSID Connection

Query rich server details from the COM Component Categories catalog and connect directly via CLSID:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcProvider, OpcResult, ServerIdentifier};

#[tokio::main]
async fn main() -> OpcResult<()> {
    let client = OpcDaClient::builder().build()?;

    // Query structured server information (ProgID, CLSID, and human-readable title)
    let server_details = client.list_server_details("localhost").await?;
    for info in server_details {
        println!("Server: {} ({})", info.display_name(), info.prog_id);
        println!("  ↳ CLSID: {:?}", info.clsid);
        println!("  ↳ Endpoint: {}", info.endpoint());
    }

    // Connect directly via 128-bit CLSID string without requiring ProgID lookup
    let direct_id = ServerIdentifier::from("{28E68F9A-8D75-11D1-8DC3-3C302A000000}");
    assert!(direct_id.is_clsid());
    Ok(())
}
```

### Remote DCOM Server Connection (Experimental / Roadmap)

> [!WARNING]
> **DCOM Implementation Status (0.3.0):**
> Full Remote DCOM operation is on the roadmap ([`long_term_todo.md`](../long_term_todo.md) Phase 5) and is **NOT yet supported for production in 0.3.0**.
>
> **For remote industrial connectivity, it is strongly recommended to use OPC UA** (via an OPC UA gateway or wrapper like Kepware, Matrikon OPC UA Tunneller, or Prosys OPC) rather than legacy DCOM. OPC UA communicates over standard TCP/IP (port 4840), traverses firewalls cleanly, and eliminates the brittle Windows security, RPC port exhaustion, and DCOM hardening (KB5004442) challenges inherent to remote COM.
>
> When using OPC DA, running `opc-da-client` locally on the server host delivers rock-solid, zero-configuration reliability.

#### What Works in 0.3.0 vs What Does Not Work
| Feature | Status | Notes |
| :--- | :--- | :--- |
| **Local OPC DA Connections** | ✅ **Supported** | 100% stable, hardened, fully tested local COM operation on the same machine. |
| **Remote Catalog Discovery** | ✅ **Supported** | Queries `OPCEnum` (`IOPCServerList`/`IOPCServerList2`) on remote hosts via DCOM. |
| **Direct CLSID Remote Connect** | ⚠️ **Experimental** | Works via explicit bracketed CLSID (`\\host\{CLSID}`) if DCOM security/firewall is configured. |
| **Remote ProgID Resolution** | ❌ **Not in 0.3.0** | ProgID resolution queries local registry; fails with `CO_E_CLASSSTRING` if not locally installed. |
| **Remote Enumerator Blanketing** | ❌ **Not in 0.3.0** | `BrowseOPCItemIDs` enumerator (`IEnumString`) lacks proxy blanketing. |

When targeting a remote host over DCOM using direct CLSID:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcResult};

#[tokio::main]
async fn main() -> OpcResult<()> {
    // Direct CLSID connection bypasses local ProgID registry lookup:
    let client = OpcDaClient::connect(r"\\192.168.1.50\{F8582CF2-88FB-11D0-B850-00C0F0104305}")?;
    client.connect_eager().await?;

    let val = client.read_f64("Random.Real8").await?;
    println!("Value: {val}");
    Ok(())
}
```

### Reading Tags with Typed Values & Quality

Read current tag values, inspect decomposed quality states, and extract strongly-typed values:

```rust,no_run
use opc_da_client::{
    OpcDaClient, OpcProvider, OpcResult, OpcValue, OpcValueOptionExt, SystemTimeOptionExt,
};

#[tokio::main]
async fn main() -> OpcResult<()> {
    let client = OpcDaClient::builder()
        .server("Matrikon.OPC.Simulation.1")
        .build_bound()?;
    let tags = vec![
        "Random.Int4".to_string(),
        "Random.Real8".to_string(),
        "Random.String".to_string(),
    ];

    let values = client.read_tags(tags).await?;

    for v in values {
        // Direct Display rendering: "Tag1 = 42.5 [Good] @ 2026-09-04 10:00:00"
        println!("{v}");

        // Zero-allocation field formatting for UI tables or logs with column width padding
        println!(
            "Tag: {:<25} | Value: {:<15} | Quality: {:<12} | Timestamp: {}",
            v.tag_id,
            v.value().display(),
            v.quality,
            v.timestamp.display()
        );

        // 16-bit quality inspection
        if !v.quality.is_good() {
            println!("  ↳ Substatus: {:?}, Limit: {:?}", v.quality.substatus(), v.quality.limit());
        }

        // Lossless pattern matching on typed domain values
        match v.value() {
            Some(OpcValue::Int(i)) => println!("  ↳ Decoded Integer: {}", i),
            Some(OpcValue::UInt(u)) => println!("  ↳ Decoded Unsigned: {}", u),
            Some(OpcValue::Float(f)) => println!("  ↳ Decoded Float: {}", f),
            Some(OpcValue::Bool(b)) => println!("  ↳ Decoded Boolean: {}", b),
            Some(OpcValue::String(s)) => println!("  ↳ Decoded String: {}", s),
            Some(OpcValue::Empty) => println!("  ↳ Uninitialized Variant (VT_EMPTY)"),
            Some(OpcValue::Null) => println!("  ↳ Null Variant (VT_NULL)"),
            None => println!("  ↳ Read failed or tag was rejected by server"),
        }
    }
    Ok(())
}
```

### Writing Values

Write typed values (`Int`, `Float`, `Bool`, `String`, or raw Rust primitives) to OPC tags. `opc-da-client` provides two write patterns depending on whether you are working with a bound session or an unbound gateway:

#### Option A: Ergonomic Bound Session (`client.write_tag` / `client.write_tags`)

When using a bound client session (`OpcDaClient::connect(server)?`), the server endpoint is bound at connection time. You do not repeat the server argument on each call, and methods accept native Rust primitives directly (`42_i32`, `3.14_f64`, `true`, `"active"`) via `impl Into<OpcValue>`:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcResult, OpcValue};

#[tokio::main]
async fn main() -> OpcResult<()> {
    // 1. Bound Session: Server is bound at connection time
    let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;

    // Highly ergonomic: write single tag with native Rust primitive:
    let result = client.write_tag("Bucket Brigade.Int4", 42_i32).await?;
    match result.status {
        Ok(()) => println!("✓ Write succeeded"),
        Err(e) => println!("✗ Write failed: {e}"),
    }

    // Zero-allocation batch write accepting array of (tag, OpcValue) pairs:
    let batch_results = client
        .write_tags([
            ("Bucket Brigade.Int4", OpcValue::Int(42)),
            ("Bucket Brigade.Real8", OpcValue::Float(3.14159)),
        ])
        .await?;
    println!("Wrote {} tags", batch_results.len());

    Ok(())
}
```

#### Option B: Unbound Gateway Client & `TagWriter` Trait (`client.write_tag_value`)

When managing multiple servers through an unbound gateway client (`OpcDaClient::builder().build()?`) or interacting with generic code via `&impl TagWriter`, specify the target server explicitly per call:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcResult, OpcValue};

#[tokio::main]
async fn main() -> OpcResult<()> {
    // 2. Unbound Gateway: Reusable across multiple servers
    let client = OpcDaClient::builder().build()?;
    let server = "Matrikon.OPC.Simulation.1";

    // Unbound client requires explicit server endpoint per call:
    let result = client
        .write_tag_value(server, "Bucket Brigade.Int4", OpcValue::Int(42))
        .await?;

    match result.status {
        Ok(()) => println!("✓ Write succeeded"),
        Err(e) => println!("✗ Write failed: {e}"),
    }
    Ok(())
}
```

### Browsing the Address Space

Recursively discover available tags in the server namespace with progress reporting and partial-result harvesting:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcProvider, OpcResult, TagCollector};

#[tokio::main]
async fn main() -> OpcResult<()> {
    let client = OpcDaClient::builder().build()?;
    let server = "Matrikon.OPC.Simulation.1";

    let collector = TagCollector::new(1000);

    let discovered_tags = client
        .browse_tags(server, collector)
        .await?;

    println!("Discovered {} tags", discovered_tags.len());
    Ok(())
}
```

### Mocking in Unit Tests

Verify downstream business logic on any platform without requiring Windows COM runtimes:

#### Granular Role Mocking with `MockTagReader`

Under `#[cfg(feature = "test-support")]`, tests can mock individual segregated role traits rather than the entire client:

```rust
use opc_da_client::{MockTagReader, OpcQuality, OpcResult, OpcValue, TagReader, TagValue};

async fn read_temperature(reader: &impl TagReader) -> OpcResult<TagValue> {
    reader.read_tag_value("SimulatedServer", "Sensor.Temp").await
}

#[tokio::main]
async fn main() -> OpcResult<()> {
    let mut mock = MockTagReader::new();
    mock.expect_read_tag_value()
        .times(1)
        .returning(|_server, tag| {
            Ok(TagValue::new(
                tag,
                Some(OpcValue::Float(98.6)),
                OpcQuality::GOOD,
                Some(std::time::SystemTime::UNIX_EPOCH),
            ))
        });

    let val = read_temperature(&mock).await?;
    assert_eq!(val.display_value(), "98.6");
    assert!(val.is_good());
    Ok(())
}
```

#### Composite Mocking with `MockOpcProvider`

```rust
use opc_da_client::{
    MockOpcProvider, OpcProvider, OpcQuality, OpcResult, OpcValue, TagReader, TagValue,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> OpcResult<()> {
    let mut mock = MockOpcProvider::new();
    mock.expect_read_tag_values()
        .times(1)
        .returning(|_server, tags| {
            Ok(tags
                .into_iter()
                .map(|tag| {
                    TagValue::new(
                        tag,
                        Some(OpcValue::Float(98.6)),
                        OpcQuality::GOOD,
                        Some(std::time::SystemTime::UNIX_EPOCH),
                    )
                })
                .collect())
        });

    let provider = Arc::new(mock);
    let values = provider
        .read_tag_values("SimulatedServer", vec!["Sensor.Temp".into()].into())
        .await?;

    assert_eq!(values.len(), 1);
    assert_eq!(values[0].display_value(), "98.6");
    assert!(values[0].is_good());
    Ok(())
}
```

## Migration Guide (0.2.x → 0.3.0)

Version `0.3.0` modernizes client ergonomics, standardizes batch write syntax, decouples the Tier 2 SPI into pure Rust, and eliminates method shadowing between bound client sessions and role traits.

### Summary of Breaking & Deprecated Changes

| Old API (0.2.x) | New Recommended API (0.3.0) | Status | Details |
|:---|:---|:---:|:---|
| `client.write(tag, val)` | `client.write_tag(tag, val)` | ⚠️ Deprecated | Disambiguates single-tag write on `Bound` client. |
| `client.write_batch(writes)` | `client.write_tags(writes)` | ⚠️ Deprecated | Accepts `impl IntoWriteBatch` (`[("T", v)]`, `&[...]`, `Vec<...>`). |
| `client.read_tag_values(tags)` on `Bound` | `client.read_tags(tags)` | ❌ Removed on `Bound` | Eliminates compiler collision (`E0592`) with `TagReader::read_tag_values`. |
| `client.read_tag_value(tag)` on `Bound` | `client.read_tag(tag)` | ❌ Removed on `Bound` | Eliminates compiler collision (`E0592`) with `TagReader::read_tag_value`. |
| `use opc_da_client::com::connector::*` | `use opc_da_client::connector::*` | ⚠️ Re-exported | Pure SPI moved to `opc_da_client::connector::*` (no Windows COM dependency). |
| `ConnectedServer::browse_opc_item_ids` returning `StringIterator` | Returns `Self::ItemIterator: Iterator<Item = OpcResult<String>>` | 🔄 Refactored | Decoupled from concrete Win32 BSTR iterator via associated type. |
| `MockOpcProvider` (monolithic only) | `MockTagReader`, `MockTagWriter`, `MockTagBrowser`, `MockServerDiscovery` | ✨ Added | Granular segregated role mocks available under `test-support`. |
| `ConversionError::Other` on missing tag | `ConversionError::TagNotRequested(tag)` / `TagNoValue(tag)` | ✨ Added | Lossless error taxonomy preserving tag identification on lookup failure. |

### Upgrading Tag Writes

In 0.2.x, batch writes used `write_batch` and single writes used `write`. In 0.3.0, the API has been harmonized to `write_tags` and `write_tag`:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcResult, OpcValue};

#[tokio::main]
async fn main() -> OpcResult<()> {
    let client = OpcDaClient::connect("Matrikon.OPC.Simulation.1")?;

    // 0.2.x (Deprecated):
    // client.write("Tag1", OpcValue::Int(10)).await?;
    // client.write_batch(vec![("Tag1".into(), OpcValue::Int(10))]).await?;

    // 0.3.0 (Recommended):
    client.write_tag("Tag1", OpcValue::Int(10)).await?;
    // write_tags accepts arrays, slices, and vectors with zero heap overhead:
    client.write_tags([
        ("Tag1", OpcValue::Int(10)),
        ("Tag2", OpcValue::Float(20.5)),
    ]).await?;

    Ok(())
}
```

### Upgrading Tag Reads on Bound Sessions

In 0.2.x, `OpcDaClient<C, Bound>` exposed 1-argument `read_tag_values` and `read_tag_value`. When `TagReader` was imported, Rust's method resolution encountered `E0592` (duplicate method definitions). In 0.3.0:
- Use `client.read_tags(tags)` for batch reads on bound sessions.
- Use `client.read_tag(tag)` for single-tag reads on bound sessions.
- Use `client.read_f32(tag)`, `client.read_i64(tag)`, `client.read_u32(tag)`, `client.read_u64(tag)`, `client.read_f64(tag)`, `client.read_i32(tag)`, `client.read_bool(tag)`, `client.read_string(tag)` for direct typed numeric scalar reads.
- Inherent 2-argument `client.read_tag_values(server, tags)` and `client.read_tag_value(server, tag)` on `OpcDaClient<C, State>` cleanly forward to the `TagReader` role trait.

### Migrating Tier 2 SPI Imports

If your application or test suite directly references the Tier 2 Service Provider Interface (SPI):

```rust
// 0.2.x:
// use opc_da_client::com::connector::{ServerConnector, ConnectedServer, ConnectedGroup};

// 0.3.0:
use opc_da_client::connector::{ServerConnector, ConnectedServer, ConnectedGroup};
```

The pure SPI traits and mock doubles in `opc_da_client::connector::*` can now be compiled and tested on any platform (macOS, Linux, Windows) without enabling `feature = "opc-da-backend"` and without linking Windows SDK headers.

### Granular Test Mocking with Role Mocks

Instead of mocking the entire composite `MockOpcProvider`, unit tests can now mock only the role traits they consume (`MockTagReader`, `MockTagWriter`, `MockTagBrowser`, `MockServerDiscovery`):

```rust
use opc_da_client::{MockTagWriter, OpcResult, OpcValue, TagWriter, WriteResult};

async fn send_command(writer: &impl TagWriter) -> OpcResult<WriteResult> {
    writer.write_tag_value("Server", "Tag1", OpcValue::Int(10)).await
}

#[tokio::main]
async fn main() -> OpcResult<()> {
    let mut mock_writer = MockTagWriter::new();
    mock_writer
        .expect_write_tag_value()
        .returning(|_server, tag, _val| Ok(WriteResult::success(tag)));

    let res = send_command(&mock_writer).await?;
    assert!(res.is_success());
    Ok(())
}
```

### Deprecation Schedule

| Feature / Method | Deprecated In | Removal Target | Replacement |
|:---|:---:|:---:|:---|
| `OpcDaClient::write` | 0.3.0 | 0.4.0 / 1.0.0 | `OpcDaClient::write_tag` |
| `OpcDaClient::write_batch` | 0.3.0 | 0.4.0 / 1.0.0 | `OpcDaClient::write_tags` |
| `OpcDaClient::list_servers_on` | 0.2.1 | 0.4.0 / 1.0.0 | `ServerDiscovery::list_servers` |
| `TagWriter::write_tag_values` | 0.2.0 | 0.4.0 / 1.0.0 | `TagWriter::write_tag_batch` |
| `opc_da_client::com::connector::*` re-exports | 0.3.0 | 0.4.0 / 1.0.0 | `opc_da_client::connector::*` |

Deprecated items will trigger compiler warnings starting in `0.3.0` and will remain backward-compatible throughout the `0.3.x` release lifecycle before being removed in `0.4.0`.

## API Surface

| Type / Trait | Kind | Purpose |
|:---|:---|:---|
| `OpcProvider` | `pub trait` | Composite async trait for OPC DA operations (`list_servers`, `list_server_details`, `browse_tags`, `read_tag_values`, `read_tag_value`, `write_tag_value`, `write_tag_batch`). |
| `ServerDiscovery` | `pub trait` | Segregated role trait for server discovery (`list_servers`, `list_server_details`). |
| `TagBrowser` | `pub trait` | Segregated role trait for namespace navigation (`browse_tags`). |
| `TagReader` | `pub trait` | Segregated role trait for reading tag values (`read_tag_values`, `read_tag_value`). |
| `TagWriter` | `pub trait` | Segregated role trait for writing tag values (`write_tag_value`, `write_tag_batch`, deprecated `write_tag_values`). |
| `OpcDaClient<C, State>` | `pub struct` | Primary client facade parameterized by state (`Unbound` gateway vs `Bound` session) with inherent session methods (`read_tag`, `read_tags`, `write_tag`, `write_tags`, `subscribe`). |
| `Unbound` | `pub struct` | Typestate marker representing an unbound multi-server gateway. |
| `Bound` | `pub struct` | Typestate marker representing a server-bound active session with infallible `endpoint(&self)`. |
| `OpcDaClientBuilder` | `pub struct` | Fluent builder for configuring host, server, timeout, and legacy DCOM mode (`build()` for `Unbound`, `build_bound()` for `Bound`). |
| `TagBatch` | `pub enum` | Zero-allocation polymorphic container for tag identifiers (`Static`, `StaticSingle`, `Shared`, `Owned`, `OwnedSingle`). |
| `IntoTags` | `pub trait` | Universal conversion trait converting static string slices, arrays, single strings, and owned vectors into `TagBatch`. |
| `TagValues` | `pub struct` | Collection of read tag values providing case-insensitive lookups, generic extraction (`get_as<T>`), and typed getters (`get_f64`, `get_f32`, `get_i32`, `get_i64`, `get_u32`, `get_u64`, `get_bool`, `get_str`). |
| `TagExtractError` | `pub enum` | Domain error enum returned by `TagValues` getters (`NotRequested`, `ReadFailed`, `NoValue`, `TypeMismatch`). Preserves root COM error provenance. |
| `ConversionError` | `pub enum` | Lossless tag value conversion errors (`TagNotRequested`, `TagNoValue`, `TypeMismatch`, `Other`) preserving tag identifiers. |
| `ServerIdentifier` | `pub enum` | Strongly-typed server identifier (`ProgId` vs `Clsid`) with automatic GUID syntax parsing. |
| `OpcServerInfo` | `pub struct` | Rich catalog metadata record (`prog_id`, `clsid`, `user_type`, `host`) with `display_name()` and `endpoint()`. |
| `OpcServerEndpoint` | `pub struct` | Endpoint binding target `host` with `identifier: ServerIdentifier`. Formats as UNC path (`\\host\server`) and implements `FromStr`. |
| `OpcServerRegistration` | `pub struct` | Detailed Windows registry diagnostics (`clsid`, `prog_id`, `binary_path`, `server_type`). |
| `OpcServerType` | `pub enum` | Execution model classification (`LocalServer32` executable vs `InprocServer32` DLL). |
| `inspect_local_registration` | `pub fn` | Diagnostic helper inspecting `HKCR\CLSID\{...}` across native and WOW64 registry views. |
| `TagValue` | `pub struct` | Canonical read result holding encapsulated `outcome: Result<OpcValue, OpcError>`, `quality: OpcQuality`, and `timestamp: Option<SystemTime>`. |
| `DisplayOptionOpcValue` | `pub struct` | Zero-allocation `Display` adapter streaming inner `OpcValue` or fallback directly into formatter. |
| `DisplayOptionTimestamp` | `pub struct` | Zero-allocation `Display` adapter streaming formatted timestamp or fallback directly into formatter. |
| `OpcValueOptionExt` | `pub trait` | Extension trait providing `.display()` and `.display_or("fallback")` for `Option<OpcValue>`. |
| `SystemTimeOptionExt` | `pub trait` | Extension trait providing `.display()` and `.display_or("fallback")` for `Option<SystemTime>`. |
| `OpcValue` | `pub enum` | Strongly-typed OPC value representation (`Int`, `UInt`, `Float`, `Bool`, `String`, `Empty`, `Null`). |
| `OpcQuality` | `pub struct` | Zero-allocation decomposed 16-bit OPC DA quality word (`major`, `substatus`, `limit`, `raw`). |
| `ParseQualityError` | `pub struct` | Error returned when parsing an invalid quality string via `FromStr`, with `.raw()` string accessor. |
| `WriteResult` | `pub struct` | Tag write operation result (`tag_id`, `status: Result<(), OpcError>`, `is_success`, `is_error`, `error`). |
| `WriteBatch` | `pub enum` | Zero-allocation polymorphic container for tag write payloads (`Single`, `Shared`, `Owned`). |
| `IntoWriteBatch` | `pub trait` | Universal conversion trait converting single pairs, arrays, slices, and vectors into `WriteBatch`. |
| `WriteBatchIter` | `pub enum` | Zero-allocation borrowed iterator yielding `(&str, &OpcValue)` for COM marshaling. |
| `TagCollector` | `pub struct` | Thread-safe, bounded container encapsulating thread-safe tag accumulation, atomic progress reporting, and cooperative cancellation token. |
| `ClientGroupHandle` | `pub struct` | Type-safe opaque handle wrapper for client-side group identification. |
| `ServerGroupHandle` | `pub struct` | Type-safe opaque handle wrapper for server-side group identification. |
| `ClientItemHandle` | `pub struct` | Type-safe opaque handle wrapper for client-side item identification. |
| `ServerItemHandle` | `pub struct` | Type-safe opaque handle wrapper for server-side item identification. |
| `BrowseType` | `pub enum` | Tag namespace browsing filter (`Branch`, `Leaf`, `Flat`). |
| `BrowseDirection` | `pub enum` | Tag namespace traversal direction (`Up`, `Down`, `To`). |
| `OpcError` | `pub enum` | Domain error enum covering connection, group, item, type, and COM HRESULT failures. |
| `OpcError::friendly_hint` | `pub fn` | Inherent method translating Win32 COM and OPC HRESULT codes into actionable human-readable explanations. |
| `OpcError::connection_failed` | `pub fn` | Inherent constructor producing an `OpcError::Connection` indicating CLSID resolution failure for a ProgID. |
| `OpcError::is_connection_error` | `pub fn` | Predicate determining whether an error was caused by transport/connection failure for reconnection logic. |
| `ServerConnector` | `pub trait` | Pure Tier 2 SPI connector trait (`connect_endpoint`). In `opc_da_client::connector`. |
| `ServerCatalogDiscovery` | `pub trait` | Pure Tier 2 SPI catalog trait (`enumerate_servers`, `enumerate_server_details`). In `opc_da_client::connector`. |
| `ServerBackend` | `pub trait` | Composite Tier 2 SPI trait (`ServerConnector + ServerCatalogDiscovery`). In `opc_da_client::connector`. |
| `ConnectedServer` | `pub trait` | Pure Tier 2 SPI active server trait with associated `type ItemIterator` and `ping()`. In `opc_da_client::connector`. |
| `ConnectedGroup` | `pub trait` | Pure Tier 2 SPI active group trait (`add_items`, `read`, `write`). In `opc_da_client::connector`. |
| `MockOpcProvider` | `pub struct` | Pure-Rust mock implementation of `OpcProvider` generated via `mockall` (under `feature = "test-support"`). |
| `MockServerDiscovery` | `pub struct` | Pure-Rust mock implementation of `ServerDiscovery` generated via `mockall` (under `feature = "test-support"`). |
| `MockTagBrowser` | `pub struct` | Pure-Rust mock implementation of `TagBrowser` generated via `mockall` (under `feature = "test-support"`). |
| `MockTagReader` | `pub struct` | Pure-Rust mock implementation of `TagReader` generated via `mockall` (under `feature = "test-support"`). |
| `MockTagWriter` | `pub struct` | Pure-Rust mock implementation of `TagWriter` generated via `mockall` (under `feature = "test-support"`). |
| `MockServerConnector` | `pub struct` | Pure-Rust mock implementation of `ServerConnector` providing simulated server enumeration and tag browsing (under `feature = "test-support"`). In `connector::mock`. |
| `MockConnectedServer` | `pub struct` | Pure-Rust mock implementation of `ConnectedServer` with in-memory namespace, `ping()` simulation, and group registration (under `feature = "test-support"`). In `connector::mock`. |
| `MockConnectedGroup` | `pub struct` | Pure-Rust mock implementation of `ConnectedGroup` with configurable handlers for item registration and read/write I/O (under `feature = "test-support"`). In `connector::mock`. |
| `MockState` | `pub struct` | Pure-Rust mock observability state tracking add/remove counts and configurable failure flags (under `feature = "test-support"`). In `connector::mock`. |
| `MockOpcDaClient` | `pub type` | Type alias for an `OpcDaClient` instantiated with `MockServerConnector` (under `feature = "test-support"` and `feature = "opc-da-backend"`). |

<!-- custom:start -->
## Architecture

The crate is architected in three decoupled layers:

1. **Public Domain API (`provider.rs`, `types.rs`)**: Exposes the high-level async trait (`OpcProvider`), canonical data models (`TagValue`, `OpcValue`, `WriteResult`, `TagCollector`, `ServerIdentifier`, `OpcServerInfo`, `OpcServerEndpoint`), pure protocol types (`GroupHandle`, `ItemHandle`, `BrowseType`, `BrowseDirection`), and zero-allocation quality word (`OpcQuality`).
2. **COM Worker & Client Runtime (`com::client`, `com::worker`)**: The asynchronous client communicates via Tokio channels with a dedicated MTA background thread. The worker thread maintains connection pooling, proxy caching (keyed by `ServerIdentifier`), and automatic recovery on stale connections.
3. **Pure-Rust Connector Facade & Subsystem (`com::connector`, `com::discovery`, `com::variant`, `raw::`)**: COM servers and groups are accessed strictly via pure-Rust trait interfaces (`ConnectedServer`, `ConnectedGroup`), while `com::discovery` manages multi-tier catalog adapters (`IOPCServerList`/`IOPCServerList2`) and local registry inspection, completely isolating Win32 COM pointers, apartments, and `VARIANT` structures from domain code.

See [architecture.md](./architecture.md) for in-depth architectural specifications and diagrams, and [spec.md](./spec.md) for behavioral contracts.

### COM Threading Model

Windows COM requires per-thread initialization and strict apartment affinity. `opc-da-client` manages this transparently:
- **Dedicated Worker Thread**: All COM API calls execute exclusively on a dedicated background thread initialized in Multi-Threaded Apartment (MTA) mode.
- **Zero Manual Initialization**: Calling applications do not need to call `CoInitializeEx` or configure apartment state.
- **Panic Isolation**: Panics on the COM thread are caught and converted into structured `OpcError::Internal` results, keeping the calling application alive.

## License

This project is licensed under the [MIT License](https://opensource.org/licenses/MIT).
<!-- custom:end -->
