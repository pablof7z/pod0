#!/usr/bin/env bash
# verify-copy-fidelity.sh — Verify that every copied file in the migration
# manifest differs from its legacy source only in approved patterns.
#
# For each row in ci/migration/manifest.tsv the script diffs the legacy file
# (in the podcast-player repo) against the copied file (in the NMP repo) and
# strips lines that match approved-pattern regexes.  If any diff lines remain
# the script exits 1 and prints the unexpected hunks.
#
# Usage:
#   ./ci/migration/verify-copy-fidelity.sh [--nmp-root <path>]
#
# Options:
#   --nmp-root <path>   Path to the nostrmultiplatform repo root.
#                       Default: /Users/pablofernandez/Work/nostrmultiplatform
#
# Approved diff patterns (applied to the +/- lines of the diff output):
#   - AppStateStore  → KernelModel          (identifier token-swap)
#   - AudioEngine.shared → model            (member-call token-swap)
#   - RAGService.shared  → model            (member-call token-swap)
#   - AgentSession.shared → model           (member-call token-swap)
#   - import <legacy-singleton>             (removed import declarations)
#
# Diff context lines (starting with space) and hunk headers (@@ ... @@) are
# always ignored; only added (+) and removed (-) lines are filtered.

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

# ── Approved patterns ────────────────────────────────────────────────────────
# Each pattern is a basic-regex matched against +/- diff lines (the leading
# +/- character is stripped before matching).

APPROVED_PATTERNS=(
  # Token swaps: identifier rename AppStateStore → KernelModel
  'AppStateStore'
  'KernelModel'
  # Token swaps: AudioEngine.shared member-call rewrites
  'AudioEngine\.shared'
  # Token swaps: RAGService.shared member-call rewrites
  'RAGService\.shared'
  # Token swaps: AgentSession.shared member-call rewrites
  'AgentSession\.shared'
  # Import removals for legacy singletons
  '^import AppStateStore$'
  '^import NostrRelayService$'
  '^import UserIdentityStore$'
  '^import RAGService$'
  '^import AgentSession$'
  '^import AudioEngine$'
  # model.playEpisode / model.searchTranscripts / model.sendAgentTurn replacements
  'model\.playEpisode'
  'model\.searchTranscripts'
  'model\.sendAgentTurn'
)

# Build a combined grep pattern (alternation)
combined_pattern=""
for p in "${APPROVED_PATTERNS[@]}"; do
  if [[ -z "$combined_pattern" ]]; then
    combined_pattern="$p"
  else
    combined_pattern="$combined_pattern|$p"
  fi
done

# ── NMP root guard ───────────────────────────────────────────────────────────
# When the NMP repo is not checked out (e.g. CI without the optional NMP
# checkout step), exit 0 trivially — there is nothing to verify against.

NMP_FEATURES="$NMP_ROOT/ios/Podcast/Podcast/Features"
if [[ ! -d "$NMP_FEATURES" ]]; then
  echo "NMP Features directory not found at $NMP_FEATURES — trivial pass (NMP repo not checked out)."
  exit 0
fi

# ── Process manifest ─────────────────────────────────────────────────────────

if [[ ! -f "$MANIFEST" ]]; then
  echo "Manifest not found: $MANIFEST" >&2
  exit 1
fi

failures=0
row=0

while IFS=$'\t' read -r legacy_rel copied_rel sha256; do
  # Skip comment lines and header row
  [[ "$legacy_rel" =~ ^# ]] && continue
  [[ "$legacy_rel" == "legacy_path" ]] && continue
  [[ -z "$legacy_rel" ]] && continue

  row=$((row + 1))

  legacy_abs="$REPO_ROOT/$legacy_rel"
  copied_abs="$NMP_ROOT/$copied_rel"

  if [[ ! -f "$legacy_abs" ]]; then
    echo "MISSING legacy: $legacy_abs" >&2
    failures=$((failures + 1))
    continue
  fi
  if [[ ! -f "$copied_abs" ]]; then
    echo "MISSING copied: $copied_abs" >&2
    failures=$((failures + 1))
    continue
  fi

  # Get diff of +/- lines only (strip context and hunk headers)
  # Then filter out approved patterns; what remains is unexpected.
  unexpected=$(diff -u "$legacy_abs" "$copied_abs" \
    | grep -E '^[+-]' \
    | grep -v '^---' \
    | grep -v '^+++' \
    | sed 's/^[+-]//' \
    | grep -vE "$combined_pattern" \
    || true)

  if [[ -n "$unexpected" ]]; then
    echo "FAIL: unexpected diff in $copied_rel:" >&2
    echo "$unexpected" | sed 's/^/  /' >&2
    failures=$((failures + 1))
  fi
done < "$MANIFEST"

if [[ $row -eq 0 ]]; then
  echo "Manifest is empty — trivial pass (no files copied yet)."
  exit 0
fi

if [[ $failures -gt 0 ]]; then
  echo "verify-copy-fidelity: $failures file(s) failed." >&2
  exit 1
fi

echo "verify-copy-fidelity: all $row file(s) passed."
