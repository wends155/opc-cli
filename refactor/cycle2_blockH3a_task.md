# Task: Sub-Block H3a: Server Identity & Host Canonicalization

- [x] Step 1: [TEST] `opc-da-client/src/types/server.rs` — [+] `test_normalize_host_lowercases_remote`
- [x] Step 2: [MODIFY] `opc-da-client/src/types/server.rs` — [~] `normalize_host` (L43-45)
- [x] Step 3: [TEST] `opc-da-client/src/types/server.rs` — [+] `test_endpoint_from_str_canonicalizes_host`
- [x] Step 4: [MODIFY] `opc-da-client/src/types/server.rs` — [~] `<OpcServerEndpoint as FromStr>::from_str` (L631, L652, L669) - 🔒 CHECKPOINT 1
- [x] Step 5: [TEST] `opc-da-client/src/types/server.rs` — [+] `test_server_identifier_matches_progid`
- [x] Step 6: [TEST] `opc-da-client/src/types/server.rs` — [+] `test_server_identifier_matches_clsid`
- [x] Step 7: [MODIFY] `opc-da-client/src/types/server.rs` — [+] `ServerIdentifier::matches` (L~185)
- [x] Step 8: [TEST] `opc-da-client/src/types/server.rs` — [+] `test_endpoint_matches`
- [x] Step 9: [MODIFY] `opc-da-client/src/types/server.rs` — [+] `OpcServerEndpoint::matches` (L~526) - 🔒 CHECKPOINT 2
- [x] Step 10: [TEST] `opc-da-client/src/client/tests.rs` — [+] `test_validate_bound_server_mixed_case`
- [x] Step 11: [MODIFY] `opc-da-client/src/client/gateway.rs` — [~] `validate_bound_server` (L376-391)
- [x] Step 12: [TEST] `opc-da-client/src/client/tests.rs` — [+] `test_validate_bound_server_host_mismatch` - 🔒 CHECKPOINT 3
- [x] Step 13: [MODIFY] `opc-da-client/src/types/server.rs` — [~] `OpcServerEndpoint::local` (L460-514)
- [x] Step 14: [MODIFY] `opc-da-client/src/types/server.rs` — verification (`cargo test --doc`) - 🔒 CHECKPOINT 4

## Builder Notes
- 💡 Step 2: Doc comment for `normalize_host` synchronized to explicitly mention ASCII lowercase canonicalization.
- 💡 Step 5 & 8: Minimal stubs added during TDD red phase to allow clean compilation and runtime assertion failures.
- 💡 Step 13: Cleaned up orphaned doc block on `pub fn new` and reconnected doc comments with doctest on `pub fn local` using `ServerIdentifier::new().unwrap()`.
