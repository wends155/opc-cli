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

## Phase 3 — Merge `opc_da_bindings` into `opc-da-client`

- [ ] Create branch `feature/merge-opc-da-bindings`
- [ ] Copy `vendor/opc_da_bindings/src/` into `opc-da-client/src/bindings/da/`
- [ ] Copy `vendor/opc_da_bindings/.metadata/` into `opc-da-client/.metadata/da/`
- [ ] Copy `vendor/opc_da_bindings/build.rs` logic into `opc-da-client/build.rs`
- [ ] Evaluate merging `opc_comn_bindings` similarly into `opc-da-client/src/bindings/comn/`
- [ ] Replace `use opc_da_bindings::` with `use crate::bindings::da::` across codebase
- [ ] Replace `use opc_comn_bindings::` with `use crate::bindings::comn::` (if merged)
- [ ] Remove `opc_da_bindings` / `opc_comn_bindings` from workspace
- [ ] Remove `vendor/opc_da_bindings/` and `vendor/opc_comn_bindings/` directories
- [ ] Add `windows-bindgen` as build-dep of `opc-da-client`
- [ ] `cargo check -p opc-da-client`
- [ ] `cargo clippy --workspace -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] Evaluate removing `vendor/opc_classic_utils/` (merge or drop)
- [ ] Remove `vendor/` directory if empty
- [ ] Update `opc-da-client/architecture.md`
- [ ] Update `architecture.md`
- [ ] Git commit

## Future Tests (Post-Vendor)

- [ ] Unit test `StringIterator` — verify no phantom `E_POINTER` errors
- [ ] Unit test `Client` trait methods with mock COM
- [ ] Static assert GUID type compatibility (`windows::core::GUID` unification)
- [ ] Integration test: list_servers against mock registry
- [ ] Integration test: browse_tags against mock server
