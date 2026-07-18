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
diff -ru "$REPO_ROOT/Generated/Pod0Core/Swift" "$TEMP_ROOT/Swift"
diff -ru "$REPO_ROOT/Generated/Pod0Core/Kotlin" "$TEMP_ROOT/Kotlin"
echo "Generated core bindings match Rust facade metadata"
