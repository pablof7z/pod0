#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/process_helpers.sh"

APP_SCHEME="${APP_SCHEME:-Podcastr}"
TUIST_GENERATE_ATTEMPTS="${TUIST_GENERATE_ATTEMPTS:-2}"
TUIST_GENERATE_TIMEOUT_SECONDS="${TUIST_GENERATE_TIMEOUT_SECONDS:-600}"

if ! command -v tuist >/dev/null 2>&1; then
  curl -Ls https://install.tuist.io | bash
fi

make_xcode_caches_writable() {
  for dd in "$HOME/Library/Developer/Xcode/DerivedData/${APP_SCHEME}-"*; do
    [ -d "$dd" ] && chmod -R u+w "$dd" 2>/dev/null || true
  done
}

clean_generated_project_state() {
  rm -rf \
    Podcastr.xcworkspace \
    Podcastr.xcodeproj/project.xcworkspace \
    Podcastr.xcodeproj/xcshareddata/swiftpm
}

run_tuist_generate_once() {
  run_command_with_timeout \
    "tuist generate" \
    "$TUIST_GENERATE_TIMEOUT_SECONDS" \
    tuist generate --no-open
}

make_xcode_caches_writable

for attempt in $(seq 1 "$TUIST_GENERATE_ATTEMPTS"); do
  echo "Running tuist generate (attempt ${attempt}/${TUIST_GENERATE_ATTEMPTS})"
  if run_tuist_generate_once; then
    exit 0
  fi

  if [ "$attempt" -lt "$TUIST_GENERATE_ATTEMPTS" ]; then
    echo "tuist generate failed or timed out; cleaning generated project state before retry" >&2
    make_xcode_caches_writable
    clean_generated_project_state
  fi
done

echo "error: tuist generate failed after ${TUIST_GENERATE_ATTEMPTS} attempt(s)" >&2
exit 1
