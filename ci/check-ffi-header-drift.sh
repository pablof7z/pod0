#!/usr/bin/env bash
set -euo pipefail

# Check that NmpCore.h declarations match Rust FFI implementations.
# This script detects drift between the C header and the Rust extern "C" symbols.
#
# Scope: NmpCore.h contains declarations from two sources:
#   1. App-local FFI  — apps/nmp-app-podcast/src/ffi/ (checked in this repo)
#   2. Upstream nmp_ffi — external git dep (github.com/pablof7z/nostr-multi-platform)
#
# Both sources are scanned.  The upstream rev is read from Cargo.lock so the
# check is always consistent with the pinned dependency.  If the upstream git
# object DB is not in the local cargo cache the upstream check is skipped with
# a warning (CI must have run `cargo fetch` beforehand).
#
# NOTE: Uses only POSIX-compatible tools (grep -E, sed, awk) to support
#       BSD grep on macOS as well as GNU grep on Linux.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HEADER_FILE="${REPO_ROOT}/App/Sources/Bridge/NmpCore.h"
FFI_DIR="${REPO_ROOT}/apps/nmp-app-podcast/src/ffi"

if [[ ! -f "$HEADER_FILE" ]]; then
    echo "Error: Header file not found: $HEADER_FILE"
    exit 1
fi

if [[ ! -d "$FFI_DIR" ]]; then
    echo "Error: FFI directory not found: $FFI_DIR"
    exit 1
fi

# ---------------------------------------------------------------------------
# Extract function names from a stream of Rust source lines.
#
# Matches the pattern:
#   pub [unsafe] extern "C" fn nmp_<name>(
#
# This is the reliable single-line form: the `fn nmp_` is always on the same
# line as `extern "C"` in well-formed Rust FFI code, regardless of how many
# other attributes (e.g. #[allow(...)]) may appear between #[no_mangle] and
# the function declaration.
# ---------------------------------------------------------------------------
extract_nmp_funcs() {
    grep -E 'pub[[:space:]]+(unsafe[[:space:]]+)?extern[[:space:]]+"C"[[:space:]]+fn[[:space:]]+nmp_' \
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
# Extract function names from LOCAL Rust FFI code.
# We scan apps/nmp-app-podcast/src/ffi/ excluding dedicated test files.
# ---------------------------------------------------------------------------
echo "Extracting function names from local Rust FFI code..."
LOCAL_FUNCS=$(
    find "$FFI_DIR" -name "*.rs" \
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

# ---------------------------------------------------------------------------
# Extract function names from UPSTREAM nmp_ffi source via cargo git cache.
#
# The rev pinned in Cargo.lock is used to read files directly from the bare
# git object DB using `git ls-tree` (to get blob hashes) followed by
# `git cat-file blob` (to read the content) — no checkout or network access
# needed, as long as `cargo fetch` has populated ~/.cargo/git/db/.
# ---------------------------------------------------------------------------
echo "Looking up upstream nmp_ffi rev from Cargo.lock..."
NMP_FFI_FULL_REV=$(
    grep -A5 'name = "nmp-ffi"' "$REPO_ROOT/Cargo.lock" \
    | grep 'source.*git+' \
    | grep -oE '#[a-f0-9]+' \
    | sed 's/#//' \
    | head -1
)

UPSTREAM_FUNCS=""
if [[ -z "$NMP_FFI_FULL_REV" ]]; then
    echo "WARNING: Could not determine upstream nmp_ffi rev from Cargo.lock; upstream drift check skipped."
else
    CARGO_HOME_DIR="${CARGO_HOME:-$HOME/.cargo}"
    if [ ! -d "$CARGO_HOME_DIR/git/db" ]; then
        echo "⚠ cargo git cache not found — run cargo fetch first; skipping upstream symbol check"
        NMP_GIT_DB=""
    else
    NMP_GIT_DB=$(
        find "$CARGO_HOME_DIR/git/db" -maxdepth 1 -type d \
             -name "nostr-multi-platform-*" 2>/dev/null | head -1
    )
    fi

    if [[ -z "$NMP_GIT_DB" ]] || \
       ! git -C "$NMP_GIT_DB" cat-file -t "$NMP_FFI_FULL_REV" &>/dev/null; then
        echo "WARNING: Upstream nmp_ffi git object DB not found or rev not fetched."
        echo "         Upstream drift check skipped. Run 'cargo fetch' to populate the cache."
    else
        echo "Scanning upstream nmp_ffi @ ${NMP_FFI_FULL_REV:0:8} ..."
        # Use ls-tree to enumerate blobs then cat-file blob to read them.
        # This works on bare repos including cargo's git object DB.
        UPSTREAM_FUNCS=$(
            git -C "$NMP_GIT_DB" ls-tree -r "$NMP_FFI_FULL_REV" \
            | grep -E 'crates/(nmp-ffi|nmp-signer-broker)/src/.*\.rs$' \
            | grep -vE '_tests(\.rs|_ext\.rs)$|_test\.rs$|/tests/' \
            | awk '{print $3}' \
            | while read -r hash; do
                git -C "$NMP_GIT_DB" cat-file blob "$hash" 2>/dev/null \
                | extract_nmp_funcs \
                || true
              done \
            | sort | uniq
        )
        UPSTREAM_COUNT=$(echo "$UPSTREAM_FUNCS" | grep -cE '^nmp_' || true)
        echo "  -> $UPSTREAM_COUNT upstream symbols found (nmp-ffi + nmp-signer-broker)."
    fi
fi

# Combine local + upstream into the full Rust symbol set.
RUST_FUNCS=$(
    { echo "$LOCAL_FUNCS"; echo "$UPSTREAM_FUNCS"; } \
    | grep -vE '^[[:space:]]*$' | sort | uniq
)

RUST_COUNT=$(echo "$RUST_FUNCS" | grep -cE '^nmp_' || true)

echo ""
echo "Summary:"
echo "  Header:               $HEADER_COUNT functions"
echo "  Local FFI:            $LOCAL_COUNT functions"
echo "  Upstream nmp_ffi:     ${UPSTREAM_COUNT:-skipped} functions"
echo "  Combined Rust total:  $RUST_COUNT functions"
echo ""

# ---------------------------------------------------------------------------
# Find differences
#
# NmpCore.h exposes only the podcast-app ABI subset — it intentionally does
# NOT include every upstream nmp_ffi symbol.  The severity rules are:
#
#   ERROR   — local app symbols (src/ffi/) missing from NmpCore.h   [drift risk]
#   ERROR   — NmpCore.h declares symbols absent from ALL Rust sources [ghost decls]
#             (demoted to WARNING when upstream check was skipped, because
#              we cannot distinguish upstream-sourced from truly ghost symbols)
#   WARNING — upstream symbols not in NmpCore.h                      [intentional]
# ---------------------------------------------------------------------------

# Local app symbols not declared in the header (real drift — always an error).
ONLY_IN_LOCAL=$(
    comm -13 <(echo "$HEADER_FUNCS") <(echo "$LOCAL_FUNCS") \
    | grep -vE '^[[:space:]]*$' || true
)

# Symbols declared in the header but absent from ALL Rust sources (ghost declarations).
# When upstream was skipped, these might be upstream-sourced; demoted to WARNING.
ONLY_IN_HEADER=$(
    comm -23 <(echo "$HEADER_FUNCS") <(echo "$RUST_FUNCS") \
    | grep -vE '^[[:space:]]*$' || true
)

# Upstream symbols not exposed by NmpCore.h — expected; NmpCore.h is a subset.
UPSTREAM_NOT_IN_HEADER=""
if [[ -n "$UPSTREAM_FUNCS" ]]; then
    UPSTREAM_NOT_IN_HEADER=$(
        comm -13 <(echo "$HEADER_FUNCS") <(echo "$UPSTREAM_FUNCS") \
        | grep -vE '^[[:space:]]*$' || true
    )
fi

UPSTREAM_SKIPPED=false
if [[ -z "$UPSTREAM_FUNCS" ]]; then
    UPSTREAM_SKIPPED=true
fi

EXIT_CODE=0

if [[ -n "$ONLY_IN_LOCAL" ]]; then
    echo "ERROR: Local app symbols (nmp-app-podcast/src/ffi) NOT declared in NmpCore.h:"
    echo "$ONLY_IN_LOCAL" | while read -r func; do
        echo "  - $func"
    done
    echo ""
    EXIT_CODE=1
fi

if [[ -n "$ONLY_IN_HEADER" ]]; then
    if [[ "$UPSTREAM_SKIPPED" == "true" ]]; then
        echo "WARNING: Functions in NmpCore.h not found in local Rust code."
        echo "         Upstream check was skipped (run 'cargo fetch' to fully validate)."
        echo "         These may be upstream-sourced declarations."
    else
        echo "ERROR: Functions declared in NmpCore.h but NOT found in any Rust source (ghost declarations):"
        EXIT_CODE=1
    fi
    echo "$ONLY_IN_HEADER" | while read -r func; do
        echo "  - $func"
    done
    echo ""
fi

if [[ -n "$UPSTREAM_NOT_IN_HEADER" ]]; then
    UPSTREAM_MISSING_COUNT=$(echo "$UPSTREAM_NOT_IN_HEADER" | grep -cE '^nmp_' || true)
    echo "WARNING: $UPSTREAM_MISSING_COUNT upstream nmp_ffi symbol(s) not declared in NmpCore.h."
    echo "         NmpCore.h intentionally exposes only the podcast-app ABI subset;"
    echo "         these upstream symbols exist in nmp-ffi/nmp-signer-broker but are"
    echo "         not used by this app and do not need to be declared."
    echo ""
fi

if [[ $EXIT_CODE -eq 0 ]]; then
    echo "✓ FFI header is in sync with Rust FFI implementations."
fi

exit $EXIT_CODE
