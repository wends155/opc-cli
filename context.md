# 🧠 Project Context: opc-cli

### [2026-02-18] Documentation Alignment (opc-da-client)
- **Request**: Create `spec.md` and update `architecture.md` for the library crate.
- **Action**:
    - Created `opc-da-client/spec.md` as the Behavioral Source of Truth.
    - Rewrote `opc-da-client/architecture.md` to follow the 11-section mandatory template from `GEMINI.md`.
    - Added Mermaid architecture diagram to `architecture.md`.
    - Integrated library-specific toolchain and observability details.
- **Outcome**: Fixed the documentation debt for the library. Both workspace crates now comply with the project-wide governance rules in `GEMINI.md`.


### [2026-02-16] Library Extraction & Workspace Restructuring
- **Request**: Extract OPC DA interaction code into a standalone, backend-agnostic library.
- **Action**: 
    - Converted project to a Cargo workspace.
    - Created `opc-da-client` library crate with a trait-based API (`OpcProvider`).
    - Implemented `opc-da-backend` and `test-support` (Mock) features for the library.
    - Decoupled `opc-cli` binary from direct COM/OPC dependencies.
    - Updated documentation (`GEMINI.md`, `architecture.md`) to reflect the new architecture.
- **Outcome**: Improved maintainability and testability. The library can now be reused by other projects or have its backend replaced without affecting the CLI.

### [2026-02-16] Tag Values UI Improvements & Timestamp Reform
- **Request**: Adjust Tag ID column width and format timestamps to local time.
- **Action**: 
    - Added `chrono` dependency.
    - Updated `src/ui.rs` column constraints: Tag ID (45%), Value (15%), Quality (10%), Timestamp (30%).
    - Implemented `FILETIME` -> Local Time conversion in `src/opc_impl.rs`.
    - Added 3 new unit tests in `src/opc_impl.rs`.
- **Outcome**: 34 tests passed. Tag ID paths no longer truncated. Timestamps are human-readable.

### [2026-02-16] Tag Browser Navigation & Search
- **Request**: Raise max tags to 5000, add Page Up/Down, add search feature.
- **Action**:
    - Raised `MAX_BROWSE_TAGS` from 500 to 5000.
    - Implemented `page_down`/`page_up` (20-item jumps) for fast scrolling.
    - Added inline search (`s`) with substrings, result cycling (`Tab`), and Spacebar toggle.
    - Updated `ui.rs` with search bar and match highlighting.
    - Added 4 new unit tests for navigation and search.
- **Outcome**: 37 tests passed. Large namespaces are now easily navigable and searchable.
- **Graceful Timeout**: Increased timeout to 300s (5 minutes) and implemented partial results harvesting on timeout.

---

## History & Decisions

### 2026-02-14: Migration to Rust
**Decision**: Migrate the project's governance and tooling rules from Go to Rust.
**Reasoning**: To leverage Rust's safety, strict type system, and modern tooling for a system-level tool that interacts heavily with Windows COM.
**Changes**:
*   Updated `GEMINI.md` to reflect Rust toolchain (`cargo`).
*   Established valid stack in `architecture.md`.
*   Enforced strict error handling (`anyhow`/`thiserror`) and observability (`tracing`) standards.
*   Mandated `unwrap()` prohibition in production code.
*   Configured `code-index` MCP tool for project root: `c:\Users\WSALIGAN\code\opc-cli`.

## Lessons Learned & Mistakes to Prevent

### Protocol & Governance
*   **The Planning Gate**: 🛑 **Mistake to Prevent**: Skipping Phase 1 (Think/Plan) when addressing user feedback. Even if the change seems "obvious" or small (like fixing a UI bug), an updated Implementation Plan must be approved before execution to ensure alignment with `architecture.md`.
*   **Sequential Execution**: 🛑 **Mistake to Prevent**: Chaining commands with `&&` in the `run_command` tool when targeting Windows PowerShell. This leads to `TokenError`. Always use sequential tool calls (e.g., `cargo check` then `cargo test`).
*   **Startup Event Clearing**: 🛑 **Bug to Prevent**: Terminal events (like the "Enter" key used to run the app) can be queued and read by `crossterm` immediately on startup, causing unintended transitions. Always clear the event queue with a poll/read loop before entering the main event loop.
*   **Shell Portability**: Use `busybox sh` for any script more complex than a single command. PowerShell syntax is inconsistent across versions (v5.1 vs v7+) and often fails on special characters or pipe redirection common in developer workflows.

### 2026-02-15: Hierarchical Tag Browse Fix & Observability
**Decision**: Implement recursive depth-first browse strategy with non-blocking UI and rich observability.
**Reasoning**: Hierarchical servers were failing at root leaf listing. Observability was needed to diagnose why certain servers (Schneider/RSLinx) were hanging/failing silently.
**Changes**:
*   Added `max_tags` and `progress` params to `browse_tags`.
*   Implemented `browse_recursive()` helper in `opc_impl.rs`.
*   Moved browse logic to background tasks using `tokio::spawn`.
*   Implemented elapsed-time instrumentation for all major COM calls.
*   Implemented "Friendly HRESULT Hints" to map technical errors (e.g., `0x80040112`) to actionable messages.

### Definitive Findings (Log Audit)
Through enriched observability, we identified the root causes of server connection issues:
*   **ABB** (`0x800706F4`): RPC/DCOM marshaling error (corrupted proxy/stub).
*   **Schneider** (`0x80080005`): Server execution failed (process crash or slow startup).
*   **RSLinx** (`0x80040112`): License rejection (missing Gateway/OEM license for 3rd-party clients).
*   **Matrikon** (`0x80004003`): Iteration failure during `IEnumString::Next`. Root cause: strict error handling failing on null-pointer quirks within COM batches. Fixed via permissive iteration.
*   **DCOM Marshalling** (`0x800706F4`): Occurs when passing `None` (NULL) as the filter string to remote OPC servers. Fixed by standardizing on `Some("")`.

### Project Context Compression (TARS)
*   **API Pattern**: Standardized on `map_err(|e| { tracing::error!(...); e })` before `.context(...)` to ensure raw HRESULTs reach the logs while rich messages reach the UI.
*   **UX Pattern**: Error messages now lead with a human-readable hint while keeping the technical chain in parentheses for expert diagnostics.

### Architectural Consistency
*   **Elm-Arch Adherence**: 🛑 **Mistake to Prevent**: Implementing navigation logic directly inside the main event loop or `ui::render`. All state transitions MUST be centralized in the `App::update` (or dedicated `impl App` methods) to maintain testability via mocks.
*   **Dependency Planning**: Dependencies should be explicitly planned in the Implementation Plan. Adding crates via `cargo add` without prior approval violates the Planning Gate.
### 2026-02-15: Testing Strategy Documentation
**Decision**: Documented the unit testing strategy and coverage in `architecture.md`.
**Reasoning**: To ensure visibility of the architectural design (trait-based decoupling) that allows UI verification without a live OPC server.
**Changes**:
*   Added `## Testing Strategy` to `architecture.md`.
*   Detailed unit test coverage for `src/app.rs` (UI state) and `src/main.rs` (input handling).
*   Confirmed decoupling via `MockOpcProvider`.
*   Verified coverage with 22 passing tests.

### 2026-02-16: Robust OPC Tree Iterated & Quality Gates
**Decision**: Standardize on permissive iteration loops for all COM collections (Strings, GUIDs, Groups).
**Reasoning**: Real-world OPC DA implementations often exhibit "soft failures" (e.g., `E_POINTER` on empty batch slots) that do not warrant a fatal crash of the entire browse tree.
**Changes**:
*   Refactored `opc_impl.rs` leaf iteration to match branch iteration (skip on error).
*   Enforced standard filter pattern `Some("")` for all browse calls to prevent marshaling errors.
*   Updated `GEMINI.md` to mandate `scripts/verify.ps1` as the primary quality gate.
*   Updated `GEMINI.md` to mandate `scripts/verify.ps1` as the primary quality gate.

### 2026-02-16: Upstream Bug Mitigation & Log Cleanliness
**Decision**: Implement workaround for `opc_da` crate `StringIterator` bug (`E_POINTER` flood) and formalize "Triple Safety" browsing.
**Reasoning**: Users were seeing hundreds of "Invalid pointer" warnings per browse operation, drowning out legitimate errors. The upstream crate initializes iterators with null pointers in the cache.
**Changes**:
*   Implemented `is_known_iterator_bug()` to detect and downgrade specific `E_POINTER` (0x80004003) errors to `TRACE`.
*   Updated `friendly_com_hint` to identify this specific issue for users.
*   Documented **OPC-BUG-001** in `architecture.md`.
*   Verified clean logs via `audit_report_v2.md`.


### 2026-02-16: Enhanced Diagnostics & DCOM Troubleshooting
**Decision**: Implement granular phase logging and expanded DCOM error hints.
**Reasoning**: Audit logs showed a 30s hang in server connection for `SchneiderElectric`, but didn't pinpoint the exact COM phase or root cause.
**Changes**:
*   Modified `browse_tags` in `opc_impl.rs` to use "Phase Envelope" logging (Started/Complete) for CLSID resolution, server creation, and namespace querying.
*   Enriched `create_server` failure logs with activation-specific timing and DCOM hints.
*   Expanded `friendly_com_hint` with `0x80070005` (Access Denied) and `0x800706BA` (RPC Server Unavailable).
*   Enriched `app.rs` timeout logging to explicitly reference phase details in logs.

### Project Context Compression (TARS)
*   **Feature**: Enhanced Diagnostics Logging & DCOM Troubleshooting.
*   **Changes**: Granular phase logging, expanded DCOM HRESULT hints, enriched timeout reporting.
*   **New Constraints**: None.
### Build Environment: Portable MSVC
🛑 **Mistake to Prevent**: Using double-escaped backslashes (`\\`) in `.cargo/config.toml` basic strings. TOML interprets `\\` as two literal backslashes. Use **literal strings** (single quotes) for Windows paths: `linker = 'C:\path\to\link.exe'`.

🛑 **Mistake to Prevent**: Assuming `[target.x86_64-pc-windows-msvc] linker = "..."` applies to build scripts. It does **not** — proc-macros and build scripts compile for the host triple via the `cc` crate, which discovers `link.exe` through `PATH`, `vswhere`, or registry. Always ensure `$env:PATH` includes the MSVC bin directory (handled by `scripts/verify.ps1`).

**Portable MSVC Paths** (for reference):
- Linker: `C:\bin\portable-msvc\msvc\VC\Tools\MSVC\14.50.35717\bin\Hostx64\x64\link.exe`
- Libs: `...\lib\x64` + `Windows Kits\10\Lib\10.0.26100.0\{ucrt,um}\x64`
- Includes: `...\include` + `Windows Kits\10\Include\10.0.26100.0\{ucrt,um,shared}`

### [2026-02-16] Zero Tags Fix & Scale Improvements
- **Request**: Fix "0 tags" regression, increase tag limit, and audit logs.
- **Action**:
    - **Bug Fix**: Restored correct `if-flat / else-hierarchical` branching in `opc_impl.rs`.
    - **Scale**: Increased `MAX_BROWSE_TAGS` to **10,000**.
    - **Audit**: Confirmed system health via `log_audit_report.md` (no recent errors).

- **Outcome**: 37 tests passed. Hierarchical browsing restored. Confirmed stable operations against `ABB.AC800MC`.

### [2026-02-16] Documentation Overhaul (opc-da-client)
- **Request**: Improve README documentation for the library on crates.io.
- **Action**:
    - Expanded `opc-da-client/README.md` with comprehensive features, installation guide, and copy-pasteable usage examples.
    - Verified link consistency with `architecture.md`.
    - Pushed changes to the remote repository.
- **Outcome**: The library is now ready for publishing with professional-grade documentation.

### [2026-02-16] Release v0.0.2
- **Request**: Tag version to v0.0.2.
- **Action**:
    - Bumped `opc-da-client` and `opc-cli` to version `0.0.2`.
    - Updated `Cargo.lock`.
    - Created git tag `v0.0.2` and pushed to remote.
- **Outcome**: Version `v0.0.2` is now live on the repository.

## Final Project Handover (2026-02-16)

The project has reached its target state:
1.  **Decoupling Complete**: The OPC DA logic is now isolated in `opc-da-client` with a stable, async-trait based API.
2.  **Standards Met**: Both library and CLI follow the strict documentation and error handling standards defined in `GEMINI.md`.
3.  **Stability**: Full test coverage (including mock providers and doctests) ensures reliability across future backend swaps.
4.  **Polish**: Root metadata (`README.md`, `LICENSE`, crate descriptions) is finalized.
5.  **Versioning**: Project tagged as `v0.0.1` on the `main` branch.

**Artifacts**:
- `opc-da-client/architecture.md`: Detailed design of the library.
- `README.md`: Workspace overview and user controls.
- `walkthrough.md`: Historical record of the extraction and refactoring process.
