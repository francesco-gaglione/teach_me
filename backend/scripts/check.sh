#!/usr/bin/env bash
# Runs the same checks as CI: formatting, lints, tests.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all
echo "OK: all checks passed"
