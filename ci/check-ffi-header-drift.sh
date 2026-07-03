#!/usr/bin/env bash
set -euo pipefail

# Check that the remaining NmpCore.h C imports exist in Rust.
#
# Scope: NmpCore.h is app-owned and intentionally narrow. App-domain
# `nmp_app_podcast_*` calls should route through generated UniFFI (`PodcastApp`)
# instead of being declared here.
#
# NOTE: Uses only POSIX-compatible tools (grep -E, sed, awk) to support
#       BSD grep on macOS as well as GNU grep on Linux.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HEADER_FILE="${REPO_ROOT}/App/Sources/Bridge/NmpCore.h"
APP_SRC_DIR="${REPO_ROOT}/apps/nmp-app-podcast/src"

if [[ ! -f "$HEADER_FILE" ]]; then
    echo "Error: Header file not found: $HEADER_FILE"
    exit 1
fi

if [[ ! -d "$APP_SRC_DIR" ]]; then
    echo "Error: app source directory not found: $APP_SRC_DIR"
    exit 1
fi

# ---------------------------------------------------------------------------
# Extract function names from a stream of Rust source lines.
#
# Matches the pattern:
#   pub [unsafe] extern "C" fn nmp_<name>(
#   pub [unsafe] extern "system" fn nmp_<name>(
#
# This is the reliable single-line form: the `fn nmp_` is always on the same
# line as `extern "C"` in well-formed Rust FFI code, regardless of how many
# other attributes (e.g. #[allow(...)]) may appear between #[no_mangle] and
# the function declaration.
# ---------------------------------------------------------------------------
extract_nmp_funcs() {
    grep -E 'pub[[:space:]]+(unsafe[[:space:]]+)?extern[[:space:]]+"(C|system)"[[:space:]]+fn[[:space:]]+nmp_' \
    | grep -oE 'fn[[:space:]]+nmp_[A-Za-z0-9_]+' \
    | sed 's/fn[[:space:]]*//'
}

# ---------------------------------------------------------------------------
# Extract function names from the C header.
# Pattern: any nmp_* name immediately before '('
# ---------------------------------------------------------------------------
echo "Extracting function names from C header..."
HEADER_FUNCS=$(
    grep -oE 'nmp_[A-Za-z0-9_]+[[:space:]]*\(' "$HEADER_FILE" \
    | sed 's/[[:space:]]*($//' \
    | sort | uniq
)

HEADER_COUNT=$(echo "$HEADER_FUNCS" | grep -cE '^nmp_' || true)
echo "  -> $HEADER_COUNT declarations found."

# ---------------------------------------------------------------------------
# Extract function names from LOCAL Rust app code, excluding dedicated tests.
# ---------------------------------------------------------------------------
echo "Extracting function names from local Rust FFI code..."
LOCAL_FUNCS=$(
    find "$APP_SRC_DIR" -name "*.rs" \
        ! -name "*_tests.rs" \
        ! -name "*_tests_ext.rs" \
        ! -name "*_test.rs" \
        -type f \
    | xargs grep -hE \
        'pub[[:space:]]+(unsafe[[:space:]]+)?extern[[:space:]]+"C"[[:space:]]+fn[[:space:]]+nmp_' \
        2>/dev/null \
    | extract_nmp_funcs \
    | sort | uniq
)

LOCAL_COUNT=$(echo "$LOCAL_FUNCS" | grep -cE '^nmp_' || true)
echo "  -> $LOCAL_COUNT local symbols found."

RUST_FUNCS="$LOCAL_FUNCS"
RUST_COUNT="$LOCAL_COUNT"

echo ""
echo "Summary:"
echo "  Header:               $HEADER_COUNT functions"
echo "  Local FFI:            $LOCAL_COUNT functions"
echo "  Combined Rust total:  $RUST_COUNT functions"
echo ""

# ---------------------------------------------------------------------------
# Find declarations that do not exist in local Rust. Local Rust may export
# additional C symbols for non-iOS compatibility while iOS migrates through
# UniFFI.
# ---------------------------------------------------------------------------

ONLY_IN_HEADER=$(
    comm -23 <(echo "$HEADER_FUNCS") <(echo "$RUST_FUNCS") \
    | grep -vE '^[[:space:]]*$' || true
)

EXIT_CODE=0

if [[ -n "$ONLY_IN_HEADER" ]]; then
    echo "ERROR: Functions declared in NmpCore.h but NOT found in local Rust source:"
    EXIT_CODE=1
    echo "$ONLY_IN_HEADER" | while read -r func; do
        echo "  - $func"
    done
    echo ""
fi

if [[ $EXIT_CODE -eq 0 ]]; then
    echo "✓ FFI header declarations exist in Rust."
fi

exit $EXIT_CODE
