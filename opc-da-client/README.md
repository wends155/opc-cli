# opc-da-client

[![Crates.io](https://img.shields.io/crates/v/opc-da-client.svg)](https://crates.io/crates/opc-da-client)
[![Docs.rs](https://docs.rs/opc-da-client/badge.svg)](https://docs.rs/opc-da-client)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Backend-agnostic OPC DA client library for Rust with an async, trait-based API.

## Features

- **Async/Await API**: Built for modern asynchronous Rust using `tokio` and `async-trait`.
- **Trait-Based Abstraction**: The `OpcProvider` trait allows for easy mocking and backend swapping.
- **Windows COM/DCOM Support**: Includes a default backend (`opc-da-backend` feature) powered by the `opc_da` crate.
- **Robust Error Handling**: Leverages `anyhow` for clear error chains and context.
- **Test-Friendly**: Built-in support for mocking via the `test-support` feature.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
opc-da-client = "0.0.1"
```

## Prerequisites

- **Operating System**: Windows (COM/DCOM is a Windows-only technology).
- **OPC DA Core Components**: Ensure the OPC DA Core Components are installed and registered on your system.
- **DCOM Configuration**: If connecting to remote servers, appropriate DCOM permissions must be configured.

## Usage Examples

### Connecting & Listing Servers

Enumerate available OPC DA servers on a local or remote host.

```rust
use opc_da_client::{OpcDaWrapper, OpcProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // mt MTA initialization is handled internally
    let client = OpcDaWrapper::new();
    
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
use opc_da_client::{OpcDaWrapper, OpcProvider};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = OpcDaWrapper::new();
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

### Browsing the Address Space

Recursively discover available tags on an OPC server.

```rust
use opc_da_client::{OpcDaWrapper, OpcProvider};
use std::sync::{Arc, Mutex, atomic::AtomicUsize};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = OpcDaWrapper::new();
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

- `OpcProvider`: The primary async trait defining OPC operations.
- `OpcDaWrapper`: The default implementation that wraps Windows COM calls and handles MTA threading requirements.

See [architecture.md](./architecture.md) for more in-depth design details.

## License

This project is licensed under the MIT License.
