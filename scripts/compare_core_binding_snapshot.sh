#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 EXPECTED_ROOT ACTUAL_ROOT" >&2
  exit 2
fi

EXPECTED_ROOT=$1
ACTUAL_ROOT=$2

diff -ru "$EXPECTED_ROOT/Swift" "$ACTUAL_ROOT/Swift"
diff -ru "$EXPECTED_ROOT/Kotlin" "$ACTUAL_ROOT/Kotlin"
diff -u "$EXPECTED_ROOT/bindings.fingerprint" \
  "$ACTUAL_ROOT/bindings.fingerprint"
