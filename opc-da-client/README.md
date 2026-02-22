# opc-da-client

[![Crates.io](https://img.shields.io/crates/v/opc-da-client.svg)](https://crates.io/crates/opc-da-client)
[![Docs.rs](https://docs.rs/opc-da-client/badge.svg)](https://docs.rs/opc-da-client)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Backend-agnostic OPC DA client library for Rust — async, trait-based, with RAII COM guard.

## Features

- **Async/Await API**: Built for modern asynchronous Rust using `tokio` and `async-trait`.
- **Trait-Based Abstraction**: The `OpcProvider` trait allows for easy mocking and backend swapping.
- **RAII COM Guard**: `ComGuard` handles COM initialization/teardown automatically — no manual `CoUninitialize` needed.
- **Read & Write Support**: Read tag values and write typed values (`Int`, `Float`, `Bool`, `String`) to OPC tags.
- **Windows COM/DCOM Support**: Native OPC DA backend via `windows-rs` — no external OPC crates needed.
- **Robust Error Handling**: Leverages `anyhow` for clear error chains and `friendly_com_hint()` for human-readable HRESULT explanations.
- **Test-Friendly**: Built-in `MockOpcProvider` via the `test-support` feature.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
opc-da-client = "0.1.2"
```

## Prerequisites

- **Operating System**: Windows (COM/DCOM is a Windows-only technology).
- **OPC DA Core Components**: Ensure the OPC DA Core Components are installed and registered on your system.
- **DCOM Configuration**: If connecting to remote servers, appropriate DCOM permissions must be configured.

## Usage Examples

### Connecting & Listing Servers

Enumerate available OPC DA servers on a local or remote host.

```rust
use opc_da_client::{ComGuard, OpcDaWrapper, OpcProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = ComGuard::new()?;
    let client = OpcDaWrapper::default();

    let servers = client.list_servers("localhost").await?;
    println!("Available Servers:");
    for server in servers {
        println!("  - {}", server);
    }
    Ok(())
}
```

### Reading Tags

Connect to a specific server and read current values for a set of tags.

```rust
use opc_da_client::{ComGuard, OpcDaWrapper, OpcProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = ComGuard::new()?;
    let client = OpcDaWrapper::default();
    let server_progid = "Matrikon.OPC.Simulation.1";
    let tags = vec![
        "Random.Int4".to_string(),
        "Random.Real8".to_string(),
    ];

    let values = client.read_tag_values(server_progid, tags).await?;

    for v in values {
        println!("Tag: {}, Value: {}, Quality: {}, Time: {}",
            v.tag_id, v.value, v.quality, v.timestamp);
    }
    Ok(())
}
```

### Writing a Value

Write a typed value to a single OPC tag.

```rust
use opc_da_client::{ComGuard, OpcDaWrapper, OpcProvider, OpcValue};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = ComGuard::new()?;
    let client = OpcDaWrapper::default();
    let server = "Matrikon.OPC.Simulation.1";

    let result = client
        .write_tag_value(server, "Bucket Brigade.Int4", OpcValue::Int(42))
        .await?;

    if result.success {
        println!("✓ Write succeeded");
    } else {
        println!("✗ Write failed: {}", result.error.unwrap_or_default());
    }
    Ok(())
}
```

### Browsing the Address Space

Recursively discover available tags on an OPC server.

```rust
use opc_da_client::{ComGuard, OpcDaWrapper, OpcProvider};
use std::sync::{Arc, Mutex, atomic::AtomicUsize};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = ComGuard::new()?;
    let client = OpcDaWrapper::default();
    let server_progid = "Matrikon.OPC.Simulation.1";

    let sink = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::new(AtomicUsize::new(0));

    client.browse_tags(
        server_progid,
        100, // Max tags to discover
        progress,
        sink.clone()
    ).await?;

    let discovered_tags = sink.lock().unwrap();
    println!("Found {} tags", discovered_tags.len());
    Ok(())
}
```

## Architecture

The library is split into a core trait layer and concrete implementations:

- **`OpcProvider`**: The primary async trait defining OPC operations (list, browse, read, write).
- **`OpcDaWrapper`**: The default implementation using native `windows-rs` COM calls. Generic over `ServerConnector` for testability; defaults to `ComConnector`.
- **`ComGuard`**: RAII guard ensuring `CoUninitialize` is called exactly once per successful `CoInitializeEx`.

See [architecture.md](https://github.com/wends155/opc-cli/blob/main/opc-da-client/architecture.md) for in-depth design details and [spec.md](https://github.com/wends155/opc-cli/blob/main/opc-da-client/spec.md) for behavioral contracts.

## License

This project is licensed under the MIT License.
