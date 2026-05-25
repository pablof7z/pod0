#!/usr/bin/env bash
# copy-features.sh — Copy Swift feature files from the legacy podcast-player
# repo into the NMP ios/Podcast/Podcast/Features/ tree and record each copy
# in ci/migration/manifest.tsv.
#
# Usage:
#   ./ci/migration/copy-features.sh [--nmp-root <path>]
#
# Options:
#   --nmp-root <path>   Path to the nostrmultiplatform repo root.
#                       Default: /Users/pablofernandez/Work/nostrmultiplatform
#
# The script must be run from the podcast-player repo root.
# Each copied file has its SHA-256 recorded in the manifest so that
# verify-copy-fidelity.sh can detect drift later.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
NMP_ROOT="/Users/pablofernandez/Work/nostrmultiplatform"
MANIFEST="$SCRIPT_DIR/manifest.tsv"

# ── Argument parsing ─────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
  case "$1" in
    --nmp-root)
      NMP_ROOT="$2"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1" >&2
      echo "Usage: $0 [--nmp-root <path>]" >&2
      exit 1
      ;;
  esac
done

# ── Paths ────────────────────────────────────────────────────────────────────

LEGACY_FEATURES="$REPO_ROOT/App/Sources/Features"
NMP_FEATURES="$NMP_ROOT/ios/Podcast/Podcast/Features"

if [[ ! -d "$LEGACY_FEATURES" ]]; then
  echo "Error: legacy Features directory not found: $LEGACY_FEATURES" >&2
  exit 1
fi

if [[ ! -d "$NMP_ROOT" ]]; then
  echo "Error: NMP repo root not found: $NMP_ROOT" >&2
  exit 1
fi

# ── Helper: compute SHA-256 portably ────────────────────────────────────────

sha256_of() {
  if command -v sha256sum &>/dev/null; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

# ── Walk legacy Features/ ────────────────────────────────────────────────────

while IFS= read -r -d '' src; do
  # rel_path relative to LEGACY_FEATURES (e.g. Home/HomeView.swift)
  rel="${src#"$LEGACY_FEATURES/"}"

  # legacy_path relative to repo root
  legacy_rel="App/Sources/Features/$rel"

  # destination path
  dst="$NMP_FEATURES/$rel"
  dst_dir="$(dirname "$dst")"
  dst_rel="ios/Podcast/Podcast/Features/$rel"

  mkdir -p "$dst_dir"
  cp "$src" "$dst"

  checksum="$(sha256_of "$dst")"

  # Append manifest row (tab-separated)
  printf '%s\t%s\t%s\n' "$legacy_rel" "$dst_rel" "$checksum" >> "$MANIFEST"

  echo "Copied: $legacy_rel → $dst_rel"
done < <(find "$LEGACY_FEATURES" -type f -name "*.swift" -print0 | sort -z)

echo "Done. Manifest updated: $MANIFEST"
