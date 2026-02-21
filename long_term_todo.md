# Long-Term TODO: OPC DA Integration

## Phase 2 — Merge `opc_da` into `opc-da-client`

- [x] Create branch `feature/merge-opc-da`
- [x] Copy `vendor/opc_da/src/` modules into `opc-da-client/src/opc_da/`
- [x] Add `mod opc_da;` to `opc-da-client/src/lib.rs`
- [x] Replace `use opc_da::` with `use crate::opc_da::` in `backend/opc_da.rs`
- [x] Replace `use opc_da::` with `use crate::opc_da::` in `helpers.rs`
- [x] Add `thiserror` as a direct dep of `opc-da-client`
- [x] Remove `opc_da` from `[workspace.dependencies]` and `members` in root `Cargo.toml`
- [x] Remove `vendor/opc_da/` directory
- [x] `cargo check -p opc-da-client`
- [x] `cargo clippy --workspace -- -D warnings`
- [x] `cargo test --workspace`
- [x] Update `opc-da-client/architecture.md` dependency table
- [x] Update `architecture.md` workspace diagram
- [x] Git commit

## Phase 3 — Merge `opc_da_bindings` into `opc-da-client` (Freeze Strategy)

- [x] Copy `vendor/opc_da_bindings/src/` into `opc-da-client/src/bindings/da/`
- [x] Copy `vendor/opc_comn_bindings/src/` into `opc-da-client/src/bindings/comn/`
- [x] Archive `.winmd` files into `opc-da-client/.winmd/` for future re-generation
- [x] Wire `mod bindings;` in `opc-da-client/src/lib.rs` (behind `opc-da-backend` feature)
- [x] Replace `use opc_da_bindings::` with `use crate::bindings::da::` across codebase
- [x] Replace `use opc_comn_bindings::` with `use crate::bindings::comn::` across codebase
- [x] Remove `opc_da_bindings` / `opc_comn_bindings` from workspace members + deps
- [x] Drop `windows-bindgen` from `[workspace.dependencies]` (no longer a build-dep)
- [x] Drop `opc_classic_utils` from workspace (unused by `opc-da-client`)
- [x] `cargo check -p opc-da-client`
- [x] `cargo clippy --workspace -- -D warnings`
- [x] `cargo test --workspace`
- [x] Remove `vendor/opc_da_bindings/` and `vendor/opc_comn_bindings/` directories
- [x] Evaluate removing `vendor/` directory if only `opc_classic_utils` remains (keep per GEMINI.md data safety rule)
- [x] Update `opc-da-client/architecture.md`
- [x] Update `architecture.md`
- [x] Git commit

## Phase 4 — Testability Refactor (ServerConnector Extraction)

- [x] C0: Extract `ServerConnector` trait in `opc-da-client/src/backend/connector.rs`
- [x] C1: Parameterize `OpcDaWrapper<C: ServerConnector = ComConnector>`
- [x] C2: Implement `ComConnector` (moves existing COM logic, zero behavior change)
- [x] `cargo check -p opc-da-client`
- [x] `cargo test --workspace`
- [x] Git commit

## Future Tests (Post-Refactor)

- [x] Unit test `StringIterator` — mock `IEnumString` via `#[windows::core::implement]`, verify no phantom `E_POINTER`
- [x] Static assert GUID type compatibility (`const_assert_eq!(size_of::<GUID>(), 16)`)
- [x] Client trait mock test via `MockServerConnector` (requires Phase 4)
- [x] Integration test: `list_servers` via `MockServerConnector` (requires Phase 4)
- [x] Integration test: `browse_tags` via `MockServerConnector` (requires Phase 4)

## Feature: SafeArray Display

- [x] Enhance `variant_to_string()` to iterate `SAFEARRAY` elements via `SafeArrayGetElement` / `SafeArrayAccessData`
- [x] Format as `[val1, val2, ...]` with max-display cap (20 elements + `...`)
- [x] Unit tests: 1-D int array, 1-D string array, empty array, multi-dimensional
