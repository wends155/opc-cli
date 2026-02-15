# 🧠 Project Context: opc-cli

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

### Recursive Summarization (TARS)
*   **Context Update**: Architecture now tolerates intermittent COM iterator errors. Quality gates are automated via PowerShell scripts to ensure consistent environment across PRs.
*   **Pruned**: Redundant manual `cargo` check instructions in `GEMINI.md` have been replaced by the verification script reference.
*   **New Constraints**: All browse-related COM calls MUST provide an empty string filter (`Some("")`) rather than `None`.
