# Architecture: opc-da-client

## Overview
`opc-da-client` is a backend-agnostic Rust library for interacting with OPC DA (Data Access) servers. It provides an async, trait-based API that abstracts away the complexities of Windows COM/DCOM and the underlying OPC implementation.

## Core Design
The library follows a layered architecture to ensure stability for consumers while allowing flexibility for backend implementations.

### Stable Public API
- **`OpcProvider` Trait**: The primary interface for all OPC operations (listing servers, browsing tags, reading values).
- **`TagValue` Struct**: A standard data structure for representing tag read results.
- **`friendly_com_hint()`**: A utility to map cryptic COM HRESULTs to human-readable strings.

### Backend Architecture
Backend implementations are gated behind feature flags. This allows replacing the underlying OPC stack (e.g., swapping the `opc_da` crate for direct `windows-rs` calls) without affecting consumer code.

- **`opc-da-backend` (Default)**: Uses the `opc_da` crate.
- **`test-support`**: Provides a `MockOpcProvider` using the `mockall` crate for independent testability of consumers.

## COM Threading Model
OPC DA relies on Windows COM, which requires per-thread initialization. 
The `OpcDaWrapper` implementation handles this by:
1. Using `tokio::task::spawn_blocking` to move COM work to a dedicated thread pool.
2. Initializing COM (`CoInitializeEx` with `COINIT_MULTITHREADED`) at the start of each task.
3. Uninitializing COM (`CoUninitialize`) before the task returns to the pool.

## Browse Strategy
The library handles both flat and hierarchical OPC namespaces:
- **Flat Namespaces**: Leaves are enumerated directly from the root.
- **Hierarchical Namespaces**: A recursive depth-first walk is performed using `change_browse_position`.
- **Safety**: A maximum recursion depth (default 50) prevents infinite loops in circular namespaces.

## Known Upstream Bugs & Workarounds
- **OPC-BUG-001 (E_POINTER Flood)**: The `opc_da` crate's `StringIterator` has a known bug where it produces 16 phantom `E_POINTER` errors per iterator. The library detects and silences these specific errors in logs to avoid spam.
- **DCOM Filter**: The `Client` implementation intentionally does not filter for `CATID_OPCDAServer10` or `20` to avoid missing servers with incomplete registry metadata.

## Platform Constraints
This library is **Windows-only** as it depends on Windows COM/DCOM for OPC DA interaction.
