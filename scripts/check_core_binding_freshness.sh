#!/usr/bin/env bash
# Fails the build when .build/pod0core/Pod0CoreFFI.xcframework was compiled
# against different bindings than the ones now in Generated/Pod0Core.
#
# The xcframework is an untracked local artifact with no dependency edges into
# the Rust sources, so Xcode will happily link a stale static library against
# freshly regenerated Swift bindings. uniffi checksums do not catch this: they
# cover function signatures, not record and enum layouts. The result is a
# silent wire-format mismatch that only aborts once a drifted type crosses the
# FFI at runtime — which is exactly how a shipped build bricked a device by
# persisting undeliverable evidence and replaying it on every launch.
#
# Runs inside the Xcode user-script sandbox, so it compares two declared files
# rather than reading the bindings tree.
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
EXPECTED="$REPO_ROOT/Generated/Pod0Core/bindings.fingerprint"
ACTUAL="$REPO_ROOT/.build/pod0core/Pod0CoreFFI.xcframework/bindings.fingerprint"

rebuild_note() {
  echo "note: run scripts/build_pod0_core_apple.sh to rebuild the Rust core." >&2
}

if [ ! -f "$EXPECTED" ]; then
  echo "error: Generated/Pod0Core/bindings.fingerprint is missing." >&2
  echo "note: run scripts/generate_core_bindings.sh to regenerate bindings." >&2
  exit 1
fi

if [ ! -f "$ACTUAL" ]; then
  echo "error: Pod0CoreFFI.xcframework is missing or predates fingerprinting." >&2
  rebuild_note
  exit 1
fi

if ! cmp -s "$EXPECTED" "$ACTUAL"; then
  echo "error: Pod0CoreFFI.xcframework was built against different bindings." >&2
  echo "note: the linked Rust core and Generated/Pod0Core disagree on FFI" >&2
  echo "note: layout; linking them aborts at runtime on the first drifted type." >&2
  rebuild_note
  exit 1
fi

echo "Pod0CoreFFI.xcframework matches the checked-in bindings"
