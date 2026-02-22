#!/bin/sh
# --- Quality Gate ---
set -e

echo "Running Formatter Check..."
cargo fmt --all -- --check

echo "Running Linter Check..."
cargo clippy --workspace --all-targets -- -D warnings

echo "Running Tests..."
cargo test --workspace

echo "All Gates Passed!"
