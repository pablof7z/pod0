#!/usr/bin/env bash
set -euo pipefail

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

kill_process_tree() {
  local signal="$1"
  local pid="$2"
  local child

  while IFS= read -r child; do
    kill_process_tree "$signal" "$child"
  done < <(pgrep -P "$pid" 2>/dev/null || true)

  kill "-$signal" "$pid" 2>/dev/null || true
}

run_tuist_generate_once() {
  tuist generate --no-open &
  local tuist_pid=$!

  (
    sleep "$TUIST_GENERATE_TIMEOUT_SECONDS"
    if kill -0 "$tuist_pid" 2>/dev/null; then
      echo "error: tuist generate exceeded ${TUIST_GENERATE_TIMEOUT_SECONDS}s; terminating stalled package resolution" >&2
      kill_process_tree TERM "$tuist_pid"
      sleep 10
      kill_process_tree KILL "$tuist_pid"
    fi
  ) &
  local watchdog_pid=$!

  set +e
  wait "$tuist_pid"
  local status=$?
  set -e

  kill "$watchdog_pid" 2>/dev/null || true
  wait "$watchdog_pid" 2>/dev/null || true
  return "$status"
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
