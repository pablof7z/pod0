#!/usr/bin/env bash
# Prints a content fingerprint of the generated Swift core bindings.
#
# Written into Generated/Pod0Core/bindings.fingerprint by
# generate_core_bindings.sh and copied into the built xcframework by
# build_pod0_core_apple.sh, so a stale Rust core can be detected by comparing
# two small files instead of rehashing the bindings inside the Xcode script
# sandbox.
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
BINDINGS_DIR=${1:-"$REPO_ROOT/Generated/Pod0Core/Swift"}

find "$BINDINGS_DIR" -type f \( -name '*.swift' -o -name '*.h' \) -print0 \
  | sort -z \
  | xargs -0 shasum -a 256 \
  | sed "s|$BINDINGS_DIR/||" \
  | shasum -a 256 \
  | cut -d' ' -f1
