# OPC DA Client CLI

A modern, asynchronous TUI (Terminal User Interface) client for browsing and reading OPC DA (Data Access) servers on Windows.

## 🏗️ Architecture

The project is structured as a Cargo workspace with two main components:

- **`opc-cli`**: The TUI application built with `ratatui`.
- **`opc-da-client`**: A backend-agnostic library that abstracts OPC DA communication through an async trait.

See **[architecture.md](file:///c:/Users/WSALIGAN/code/opc-cli/opc-da-client/architecture.md)** for detailed design information.

## ✨ Features

- **Asynchronous Data Access**: Non-blocking IO for all OPC operations.
- **Hierarchical Browsing**: Recursive exploration of complex server namespaces.
- **Real-time Monitoring**: Live tag value updates with auto-refresh.
- **Search & Filter**: Quickly find tags in large namespaces.
- **Rich Error Hints**: Human-readable explanations for cryptic Windows COM/DCOM errors.
- **Mockable Backend**: Built-in support for unit testing without a live OPC server.

## 🚀 Getting Started

### Prerequisites

- **Windows OS**: This application uses Windows COM/DCOM.
- **OPC Core Components**: Must be installed on the system to resolve OPC ProgIDs.

### Running

Execute the TUI from the project root:

```powershell
cargo run --bin opc-cli
```

## ⌨️ Controls

- `Enter`: Navigate forward / Confirm input.
- `Esc`: Navigate back.
- `Space`: Toggle tag selection.
- `s`: Enter search/filter mode (Tag List).
- `↑/↓`: Navigate lists.
- `PgUp/PgDn`: Page through lists.
- `q`: Quit application.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
