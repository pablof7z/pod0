#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

cd "$REPO_ROOT/rust"
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
python3 "$SCRIPT_DIR/check_rust_dependency_policy.py"
python3 "$SCRIPT_DIR/check_rust_facade_boundary.py"

if ! command -v cargo-deny >/dev/null 2>&1; then
  echo "cargo-deny 0.20.2 is required" >&2
  exit 1
fi
if ! command -v cargo-audit >/dev/null 2>&1; then
  echo "cargo-audit 0.22.2 is required" >&2
  exit 1
fi

cargo deny check
cargo audit --ignore RUSTSEC-2026-0118 --ignore RUSTSEC-2026-0119
