# OPC DA Client CLI

A modern, asynchronous TUI (Terminal User Interface) client for browsing, reading, and writing OPC DA (Data Access) tags on Windows.

## 🏗️ Architecture

The project is structured as a Cargo workspace with two crates:

- **`opc-cli`**: The interactive TUI application built with `ratatui` + `crossterm`.
- **`opc-da-client`**: A backend-agnostic library that abstracts OPC DA communication through an async trait (`OpcProvider`).

See **[architecture.md](./architecture.md)** for the full design, state machine, and data flow diagrams.

## ✨ Features

- **Server Discovery**: Enumerate OPC DA servers on local or remote hosts.
- **Hierarchical Browsing**: Recursive exploration of complex server namespaces with partial-result harvesting on timeout.
- **Real-time Monitoring**: Live tag value updates with 1-second auto-refresh.
- **Tag Write Support**: Write typed values (int, float, bool, string) to individual tags.
- **Search & Filter**: Substring search with `Tab`/`Shift+Tab` cycling through matches.
- **Rich Error Hints**: Human-readable explanations for cryptic Windows COM/DCOM HRESULT codes.
- **RAII COM Guard**: Safe COM initialization/teardown via `ComGuard` — no manual `CoUninitialize`.
- **Mockable Backend**: Unit-test the TUI on any OS without a live OPC server.

## 🚀 Getting Started

### Prerequisites

- **Windows OS**: This application uses Windows COM/DCOM.
- **OPC Core Components**: Must be installed on the system to resolve OPC ProgIDs.
- **Rust 1.93+**: Edition 2024.

### Build & Run

```powershell
# Run the TUI
cargo run --bin opc-cli

# Run the full verification gate (format → lint → test)
pwsh -File scripts/verify.ps1
```

## ⌨️ Controls

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

## 📄 License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.
