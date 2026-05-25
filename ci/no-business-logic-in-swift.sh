#!/usr/bin/env bash
# no-business-logic-in-swift.sh — Lint gate that enforces NMP doctrine §2:
# "No business logic in Swift."
#
# Scans ios/Podcast/Podcast/Features/ in the NMP repo for patterns that
# indicate business logic has leaked into the UI layer.  Exits 1 if any
# violation is found; exits 0 if Features/ does not exist yet (M0 trivial
# pass) or if all checks are clean.
#
# Usage:
#   ./ci/no-business-logic-in-swift.sh [--nmp-root <path>]
#
# Options:
#   --nmp-root <path>   Path to the nostrmultiplatform repo root.
#                       Default: /Users/pablofernandez/Work/nostrmultiplatform
#
# Forbidden patterns in Features/:
#   Class declarations: ObservableObject subclasses, @Observable class,
#   *Service, *Store, *Session, *Client, *Controller, *Composer, *ViewModel
#
#   OS API calls outside Capabilities/:
#   URLSession, URLRequest, WebSocket, Keychain (any variant)
#
#   Legacy singleton imports:
#   AppStateStore, NostrRelayService, UserIdentityStore,
#   RAGService, AgentSession, AudioEngine

set -euo pipefail

NMP_ROOT="/Users/pablofernandez/Work/nostrmultiplatform"

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

FEATURES_DIR="$NMP_ROOT/ios/Podcast/Podcast/Features"

# Trivial pass if the Features directory hasn't been created yet.
if [[ ! -d "$FEATURES_DIR" ]]; then
  echo "no-business-logic: Features/ not found — trivial pass (M0 skeleton)."
  exit 0
fi

violations=0

# ── Helper ───────────────────────────────────────────────────────────────────

check_pattern() {
  local label="$1"
  local pattern="$2"
  local dir="$3"

  if [[ ! -d "$dir" ]]; then
    return
  fi

  matches=$(grep -rn --include="*.swift" -E "$pattern" "$dir" || true)
  if [[ -n "$matches" ]]; then
    echo "VIOLATION [$label]:" >&2
    echo "$matches" | sed 's/^/  /' >&2
    violations=$((violations + 1))
  fi
}

# ── Forbidden class declarations in Features/ ────────────────────────────────

check_pattern "ObservableObject subclass" \
  'class[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*:[[:space:]]*(.*[[:space:]]+)?ObservableObject' \
  "$FEATURES_DIR"

check_pattern "@Observable class" \
  '@Observable[[:space:]]+class' \
  "$FEATURES_DIR"

check_pattern "class *Service" \
  'class[[:space:]]+[A-Za-z_][A-Za-z0-9_]*Service[[:space:]]*(:|{)' \
  "$FEATURES_DIR"

check_pattern "class *Store" \
  'class[[:space:]]+[A-Za-z_][A-Za-z0-9_]*Store[[:space:]]*(:|{)' \
  "$FEATURES_DIR"

check_pattern "class *Session" \
  'class[[:space:]]+[A-Za-z_][A-Za-z0-9_]*Session[[:space:]]*(:|{)' \
  "$FEATURES_DIR"

check_pattern "class *Client" \
  'class[[:space:]]+[A-Za-z_][A-Za-z0-9_]*Client[[:space:]]*(:|{)' \
  "$FEATURES_DIR"

check_pattern "class *Controller" \
  'class[[:space:]]+[A-Za-z_][A-Za-z0-9_]*Controller[[:space:]]*(:|{)' \
  "$FEATURES_DIR"

check_pattern "class *Composer" \
  'class[[:space:]]+[A-Za-z_][A-Za-z0-9_]*Composer[[:space:]]*(:|{)' \
  "$FEATURES_DIR"

check_pattern "class *ViewModel" \
  'class[[:space:]]+[A-Za-z_][A-Za-z0-9_]*ViewModel[[:space:]]*(:|{)' \
  "$FEATURES_DIR"

# ── OS API calls outside Capabilities/ ──────────────────────────────────────
# These are forbidden anywhere in Features/; Capabilities/ is the only home.

check_pattern "URLSession in Features" \
  'URLSession' \
  "$FEATURES_DIR"

check_pattern "URLRequest in Features" \
  'URLRequest' \
  "$FEATURES_DIR"

check_pattern "WebSocket in Features" \
  'WebSocket' \
  "$FEATURES_DIR"

check_pattern "Keychain* in Features" \
  'Keychain' \
  "$FEATURES_DIR"

# ── Legacy singleton imports in Features/ ────────────────────────────────────

LEGACY_IMPORTS=(
  'AppStateStore'
  'NostrRelayService'
  'UserIdentityStore'
  'RAGService'
  'AgentSession'
  'AudioEngine'
)

for module in "${LEGACY_IMPORTS[@]}"; do
  check_pattern "import $module" \
    "^import[[:space:]]+${module}$" \
    "$FEATURES_DIR"
done

# ── Result ───────────────────────────────────────────────────────────────────

if [[ $violations -gt 0 ]]; then
  echo "no-business-logic: $violations violation group(s) found." >&2
  exit 1
fi

echo "no-business-logic: clean."
