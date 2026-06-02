#!/usr/bin/env bash
#
# run_tui_integration.sh — build and run the headless kernel integration test.
#
# Builds `podcast-tui`'s `integration_test` binary, runs it against a fresh
# temp data dir, and reports pass/fail from its exit code.
#
#   exit 0  → "ALL ASSERTIONS PASSED" (every kernel round-trip assertion held)
#   exit 1  → a kernel assertion failed, a dispatch errored, or convergence
#             timed out (the binary prints the specific failure to stderr)
#
# NOTE: this is an *integration* test — the subscribe step makes a real
# network request to a live RSS feed. It is intentionally NOT part of
# `cargo test`.
#
# Usage:
#   tests/integration/run_tui_integration.sh
#
set -euo pipefail

# Resolve the repo root from this script's location (works from any CWD).
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

echo "==> Building integration_test binary (cargo build -p podcast-tui --bin integration_test)"
cargo build --manifest-path "${REPO_ROOT}/Cargo.toml" -p podcast-tui --bin integration_test

BIN="${REPO_ROOT}/target/debug/integration_test"

# The binary owns its own hermetic temp data dir: it creates one under the
# system temp directory and `remove_dir_all`s it on exit. The script does not
# pass a path, so there is a single owner of the test data dir (no races, no
# leftover state).
echo "==> Running ${BIN}"
set +e
"${BIN}"
EXIT_CODE=$?
set -e

echo "==> integration_test exited with code ${EXIT_CODE}"
if [[ "${EXIT_CODE}" -eq 0 ]]; then
  echo "PASS: headless kernel integration test"
else
  echo "FAIL: headless kernel integration test (exit ${EXIT_CODE})"
fi

exit "${EXIT_CODE}"
