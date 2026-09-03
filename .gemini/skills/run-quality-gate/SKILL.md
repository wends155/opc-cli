---
name: run-quality-gate
description: >
  Execute the strict zero-exit quality gate (formatting, linting, tests, and AST checks if applicable).
---

# Run Quality Gate

## When to Use
Invoked by `/build` (Final Verification), `/audit` (Verification Gate), `/plan-making` (implicit), and `/toolcheck` (environment validation).

## Constraints
- MUST use commands from `architecture.md § Toolchain` (do NOT invent commands).
- MUST achieve zero-exit on all gates (`FMT` + `LINT` + `TEST`).
- MUST run `sg scan` only if `sgconfig.yml` exists in the project root.
- MUST report exit codes for each gate.

## Procedure
1. Detect project type (e.g., `Cargo.toml` → Rust, `package.json` → Node).
2. Source commands from `architecture.md § Toolchain` (if it exists).
   - *Default Rust commands:*
     - Formatting: `cargo fmt --all -- --check`
     - Linting: `cargo clippy --all-targets --all-features -- -D warnings`
     - Tests: `cargo test --all-features`
3. Run Gate 1: Formatting
4. Run Gate 2: Linting
5. Run Gate 3: Tests
6. Run Gate 4: AST Linting — run `sg scan` ONLY if `sgconfig.yml` exists in the root.
7. Run Gate 5: Browser Smoke Test *(advisory — non-blocking)*:
   - Check if `chrome-devtools-mcp` is available AND local dev server (`http://localhost:8080`) is active.
   - If active: execute `browser-smoke-test` skill procedure.
   - If inactive/unavailable: report as `⏭️ Skipped` (not `❌ Failed`).
8. Report the results in a table containing the gate name, command/skill executed, and exit code / status.

## Error Recovery
If any blocking gate (Gates 1–4) fails, immediately report the failing gate and its exit code. Do not proceed past a failing gate unless explicitly instructed. Gate 5 (Browser Smoke) is advisory — failures or skips are logged in the summary table without halting the pipeline.
