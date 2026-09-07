#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

if [[ "${1:-}" == "--self-test" ]]; then
  cd "$REPO_ROOT"
  python3 -m unittest scripts.tests.test_mandatory_ci_gates
  exit 0
fi

python3 "$SCRIPT_DIR/check_architecture_ownership.py"
"$SCRIPT_DIR/check_core_binding_drift.sh"
"$SCRIPT_DIR/check_swift_core_bindings.sh"
"$SCRIPT_DIR/check_kotlin_core_bindings.sh"
echo "Mandatory ownership and generated-binding CI gates passed"
