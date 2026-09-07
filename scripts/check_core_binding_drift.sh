#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
TEMP_ROOT=$(mktemp -d /tmp/pod0-binding-drift.XXXXXX)

cleanup() {
  case "$TEMP_ROOT" in
    /tmp/pod0-binding-drift.*) rm -rf "$TEMP_ROOT" ;;
  esac
}
trap cleanup EXIT

POD0_BINDINGS_OUTPUT_ROOT="$TEMP_ROOT" "$SCRIPT_DIR/generate_core_bindings.sh"
"$SCRIPT_DIR/compare_core_binding_snapshot.sh" \
  "$REPO_ROOT/Generated/Pod0Core" "$TEMP_ROOT"
echo "Generated core bindings match Rust facade metadata"
