# opc-da-client

[![Crates.io](https://img.shields.io/crates/v/opc-da-client.svg)](https://crates.io/crates/opc-da-client)
[![Docs.rs](https://docs.rs/opc-da-client/badge.svg)](https://docs.rs/opc-da-client)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> Backend-agnostic OPC DA client library for Rust — async, trait-based, with transparent COM management.

## Overview

`opc-da-client` provides a high-performance, asynchronous Rust client for communicating with OPC Data Access (OPC DA 2.05a) servers on Windows.

OPC DA is deeply coupled to Windows COM/DCOM, which poses significant architectural hurdles in modern systems: strict Multi-Threaded Apartment (MTA) threading requirements, thread affinity, raw memory allocations (`CoTaskMemAlloc`), and cryptic HRESULT error codes. 

`opc-da-client` solves these challenges by isolating all Win32 COM and DCOM interactions behind a **pure-Rust connector facade** and a dedicated MTA background worker thread. Callers interact exclusively with safe, strongly-typed asynchronous Rust traits and domain models without writing a single line of `unsafe` code.

## Features

- **Async/Await Trait Abstraction**: Built on `tokio` and `async-trait`, using the canonical `OpcProvider` trait for zero-cost abstraction, backend flexibility, and straightforward test mocking.
- **Pure-Rust Connector Facade**: Strict isolation of low-level Win32 COM and FFI types behind the `ConnectedServer` and `ConnectedGroup` traits, keeping raw COM types and unsafe memory handling strictly internal.
- **Transparent COM & Thread Management**: Automatically spawns and manages a dedicated MTA worker thread, maintaining strict thread affinity and connection pooling with auto-recovery for stale proxies.
- **Strongly-Typed Domain Models**: `TagValue` uses `Option<OpcValue>` and `Option<std::time::SystemTime>` ensuring lossless, zero-allocation typed access on read results and writes.
- **Zero-Allocation Display Adapters**: `DisplayOptionOpcValue` and `DisplayOptionTimestamp` adapters with extension traits `OpcValueOptionExt` and `SystemTimeOptionExt` enable zero-allocation formatted streaming with width-padded table alignment.
- **Canonical Display Formatting**: `TagValue` implements `std::fmt::Display` rendering `"{tag_id} = {value} [{quality}] @ {timestamp}"` for clean, single-line logging and diagnostics.
- **16-Bit Quality Decomposition**: Zero-allocation `OpcQuality` struct decomposes raw OPC DA quality words into major status, substatus, and limit states with rich, human-readable diagnostics.
- **Native Windows Backend**: Implemented natively with `windows-rs` — eliminates heavy legacy C++ binaries and external OPC crate dependencies.
- **Context-Rich Error Handling**: Domain-specific `OpcError` via `thiserror` paired with `friendly_com_hint()` for actionable HRESULT troubleshooting.
- **Thread-Safe Tag Collection & Cancellation**: `TagCollector` encapsulates bounded accumulation (`max_tags`), lock-free atomic length monitoring, and cooperative cancellation tokens to eliminate worker thread starvation.
- **First-Class Test Support**: Includes pure-Rust mock implementations and an optional `MockOpcProvider` via the `test-support` feature flag.

## Feature Flags

| Flag | Default | Description |
|:---|:---:|:---|
| `opc-da-backend` | ✅ Yes | Compiles the native Windows COM backend (`OpcDaClient` and `ComConnector`). |
| `test-support` | ❌ No | Enables `mockall` support and exports the `MockOpcProvider` mock struct for downstream unit tests. |
| `dev-diagnostics` | ❌ No | Compiles verbose `TRACE`-level argument dumps into backend methods for low-level protocol debugging. |

## Installation

Add `opc-da-client` to your `Cargo.toml`:

```toml
[dependencies]
opc-da-client = "0.2.0"
```

To enable test mocks for unit testing your own crates:

```toml
[dev-dependencies]
opc-da-client = { version = "0.2.0", features = ["test-support"] }
```

## Prerequisites

- **Operating System**: Windows (COM/DCOM is a Windows-exclusive API).
- **OPC Core Components**: The OPC DA Core Components redistributables must be installed and registered on the machine to resolve OPC server CLSIDs and ProgIDs.
- **DCOM Security**: If communicating with remote OPC servers over the network, appropriate DCOM launch, activation, and access permissions must be configured via `dcomcnfg`.

## Usage Examples

### Connecting & Listing Servers

Enumerate available OPC DA servers registered on a local or remote host:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcProvider, OpcResult};

#[tokio::main]
async fn main() -> OpcResult<()> {
    let client = OpcDaClient::default();
    let servers = client.list_servers("localhost").await?;

    println!("Available Servers:");
    for server in servers {
        println!("  - {}", server);
    }
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
    let client = OpcDaClient::default();
    let server = "Matrikon.OPC.Simulation.1";
    let tags = vec![
        "Random.Int4".to_string(),
        "Random.Real8".to_string(),
        "Random.String".to_string(),
    ];

    let values = client.read_tag_values(server, tags).await?;

    for v in values {
        // Direct Display rendering: "Tag1 = 42.5 [Good] @ 2026-09-04 10:00:00"
        println!("{v}");

        // Zero-allocation field formatting for UI tables or logs with column width padding
        println!(
            "Tag: {:<25} | Value: {:<15} | Quality: {:<12} | Timestamp: {}",
            v.tag_id,
            v.value.display(),
            v.quality,
            v.timestamp.display()
        );

        // 16-bit quality inspection
        if !v.quality.is_good() {
            println!("  ↳ Substatus: {:?}, Limit: {:?}", v.quality.substatus, v.quality.limit);
        }

        // Lossless pattern matching on typed domain values
        match v.value {
            Some(OpcValue::Int(i)) => println!("  ↳ Decoded Integer: {}", i),
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

### Writing a Value

Write typed values (`Int`, `Float`, `Bool`, `String`) to an individual OPC tag:

```rust,no_run
use opc_da_client::{OpcDaClient, OpcProvider, OpcResult, OpcValue};

#[tokio::main]
async fn main() -> OpcResult<()> {
    let client = OpcDaClient::default();
    let server = "Matrikon.OPC.Simulation.1";

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
    let client = OpcDaClient::default();
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

```rust,ignore
use opc_da_client::{MockOpcProvider, OpcProvider, OpcQuality, OpcResult, OpcValue, TagValue};
use std::sync::Arc;

#[tokio::test]
async fn test_telemetry_service_with_mock() -> OpcResult<()> {
    let mut mock = MockOpcProvider::new();
    mock.expect_read_tag_values()
        .times(1)
        .returning(|_server, tags| {
            Ok(tags
                .into_iter()
                .map(|tag| TagValue {
                    tag_id: tag,
                    value: Some(OpcValue::Float(98.6)),
                    quality: OpcQuality::GOOD,
                    timestamp: Some(std::time::SystemTime::UNIX_EPOCH),
                })
                .collect())
        });

    let provider: Arc<dyn OpcProvider> = Arc::new(mock);
    let values = provider
        .read_tag_values("SimulatedServer", vec!["Sensor.Temp".into()])
        .await?;

    assert_eq!(values.len(), 1);
    assert_eq!(values[0].display_value(), "98.6");
    assert!(values[0].is_good());
    Ok(())
}
```

## API Surface

| Type / Trait | Kind | Purpose |
|:---|:---|:---|
| `OpcProvider` | `pub trait` | Async trait for OPC DA operations (`list_servers`, `browse_tags`, `read_tag_values`, `write_tag_value`). |
| `OpcDaClient` | `pub struct` | Primary client implementation using Windows COM through a dedicated worker thread. |
| `TagValue` | `pub struct` | Canonical read result (`tag_id`, `Option<OpcValue>`, `OpcQuality`, `Option<SystemTime>`) with `Display` and display helpers. |
| `DisplayOptionOpcValue` | `pub struct` | Zero-allocation `Display` adapter streaming inner `OpcValue` or fallback directly into formatter. |
| `DisplayOptionTimestamp` | `pub struct` | Zero-allocation `Display` adapter streaming formatted timestamp or fallback directly into formatter. |
| `OpcValueOptionExt` | `pub trait` | Extension trait providing `.display()` and `.display_or("fallback")` for `Option<OpcValue>`. |
| `SystemTimeOptionExt` | `pub trait` | Extension trait providing `.display()` and `.display_or("fallback")` for `Option<SystemTime>`. |
| `OpcValue` | `pub enum` | Strongly-typed OPC value representation (`Int`, `Float`, `Bool`, `String`, `Empty`, `Null`). |
| `OpcQuality` | `pub struct` | Zero-allocation decomposed 16-bit OPC DA quality word (`major`, `substatus`, `limit`, `raw`). |
| `WriteResult` | `pub struct` | Tag write operation result (`tag_id`, `status: Result<(), OpcError>`, `is_success`, `is_error`, `error`). |
| `TagCollector` | `pub struct` | Thread-safe, bounded container encapsulating thread-safe tag accumulation, atomic progress reporting, and cooperative cancellation token. |
| `OpcError` | `pub enum` | Domain error enum covering connection, group, item, type, and COM HRESULT failures. |
| `friendly_com_hint` | `pub fn` | Translates raw Win32/OPC HRESULT codes into actionable human-readable explanations. |

## Architecture

The crate is architected in three decoupled layers:

1. **Public Domain API (`provider.rs`, `types.rs`)**: Exposes the high-level async trait (`OpcProvider`), canonical data models (`TagValue`, `OpcValue`, `WriteResult`), and zero-allocation quality word (`OpcQuality`).
2. **COM Worker & Client Runtime (`com::client`, `com::worker`)**: The asynchronous client communicates via Tokio channels with a dedicated MTA background thread. The worker thread maintains connection pooling, proxy caching, and automatic recovery on stale connections.
3. **Pure-Rust Connector Facade (`com::connector`, `raw::`)**: COM servers and groups are accessed strictly via pure-Rust trait interfaces (`ConnectedServer`, `ConnectedGroup`), completely isolating Win32 COM pointers, apartments, and `VARIANT` structures from domain code.

See [architecture.md](./architecture.md) for in-depth architectural specifications and diagrams, and [spec.md](./spec.md) for behavioral contracts.

### COM Threading Model

Windows COM requires per-thread initialization and strict apartment affinity. `opc-da-client` manages this transparently:
- **Dedicated Worker Thread**: All COM API calls execute exclusively on a dedicated background thread initialized in Multi-Threaded Apartment (MTA) mode.
- **Zero Manual Initialization**: Calling applications do not need to call `CoInitializeEx` or configure apartment state.
- **Panic Isolation**: Panics on the COM thread are caught and converted into structured `OpcError::Internal` results, keeping the calling application alive.

## License

This project is licensed under the [MIT License](https://opensource.org/licenses/MIT).
