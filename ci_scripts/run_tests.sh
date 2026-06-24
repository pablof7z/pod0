#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/process_helpers.sh"

APP_SCHEME="${APP_SCHEME:-Podcastr}"
PROJECT_PATH="${PROJECT_PATH:-Podcastr.xcodeproj}"
WORKSPACE_PATH="${WORKSPACE_PATH:-Podcastr.xcworkspace}"
RUST_PACKAGE="${RUST_PACKAGE:-nmp-app-podcast}"
SIM_RUST_TARGET="aarch64-apple-ios-sim"
XCODE_PACKAGE_RESOLVE_ATTEMPTS="${XCODE_PACKAGE_RESOLVE_ATTEMPTS:-2}"
XCODE_PACKAGE_RESOLVE_TIMEOUT_SECONDS="${XCODE_PACKAGE_RESOLVE_TIMEOUT_SECONDS:-600}"
XCODE_PACKAGE_CACHE_PATH="${XCODE_PACKAGE_CACHE_PATH:-$PWD/Derived/PackageCache}"
XCODE_CLONED_SOURCE_PACKAGES_DIR="${XCODE_CLONED_SOURCE_PACKAGES_DIR:-$PWD/Derived/SourcePackages}"

export GIT_TERMINAL_PROMPT="${GIT_TERMINAL_PROMPT:-0}"
export GIT_HTTP_LOW_SPEED_LIMIT="${GIT_HTTP_LOW_SPEED_LIMIT:-1024}"
export GIT_HTTP_LOW_SPEED_TIME="${GIT_HTTP_LOW_SPEED_TIME:-60}"

if [ -d "$WORKSPACE_PATH" ]; then
  XCODE_CONTAINER_ARGS=(-workspace "$WORKSPACE_PATH")
else
  XCODE_CONTAINER_ARGS=(-project "$PROJECT_PATH")
fi

# Build the Rust core for the simulator arch BEFORE xcodebuild test.
#
# The Swift app links the Rust FFI (e.g. `_nmp_free_string`) from
# `target/${SIM_RUST_TARGET}/debug/libnmp_app_podcast.{a,dylib}` — see the
# `LIBRARY_SEARCH_PATHS[sdk=iphonesimulator*]` entry in Project.swift. Without
# this step the linker silently falls back to a stale copy under
# `$HOME/.cargo/target-shared` (or finds nothing), which is missing any FFI
# symbol added since that copy was produced. The result is
# `Undefined symbol: _nmp_free_string` → "Testing cancelled because the build
# failed" → the whole TestFlight pipeline (test → deploy) never runs.
#
# Mirror the device path (`just pablo-iphone-deploy` builds the same package for
# `aarch64-apple-ios`) so the simulator test lane always links a fresh core.
echo "Building Rust core ${RUST_PACKAGE} for ${SIM_RUST_TARGET}..."
cargo build --target "${SIM_RUST_TARGET}" -p "${RUST_PACKAGE}"

make_xcode_caches_writable() {
  for dd in "$HOME/Library/Developer/Xcode/DerivedData/${APP_SCHEME}-"*; do
    [ -d "$dd" ] && chmod -R u+w "$dd" 2>/dev/null || true
  done
}

clean_xcode_package_state() {
  rm -rf "$XCODE_PACKAGE_CACHE_PATH" "$XCODE_CLONED_SOURCE_PACKAGES_DIR"
  for dd in "$HOME/Library/Developer/Xcode/DerivedData/${APP_SCHEME}-"*; do
    [ -d "$dd" ] && rm -rf "$dd/SourcePackages" 2>/dev/null || true
  done
}

resolve_xcode_packages_once() {
  mkdir -p "$XCODE_PACKAGE_CACHE_PATH" "$XCODE_CLONED_SOURCE_PACKAGES_DIR"
  xcodebuild \
    "${XCODE_CONTAINER_ARGS[@]}" \
    -scheme "$APP_SCHEME" \
    -resolvePackageDependencies \
    -skipPackagePluginValidation \
    -onlyUsePackageVersionsFromResolvedFile \
    -packageCachePath "$XCODE_PACKAGE_CACHE_PATH" \
    -clonedSourcePackagesDirPath "$XCODE_CLONED_SOURCE_PACKAGES_DIR"
}

# Pre-build hygiene: the secp256k1 / P256K SwiftPM build-tool plugin writes its
# generated shared sources read-only. If a prior build was interrupted (e.g. a
# cancelled run on this self-hosted runner), those files survive read-only and
# the next build's `cp` over them fails "Permission denied", aborting the whole
# build → test → deploy pipeline. Make any existing Podcastr DerivedData
# writable so the plugin can overwrite its own cached outputs.
make_xcode_caches_writable

for attempt in $(seq 1 "$XCODE_PACKAGE_RESOLVE_ATTEMPTS"); do
  echo "Resolving Xcode Swift packages (attempt ${attempt}/${XCODE_PACKAGE_RESOLVE_ATTEMPTS})"
  if run_command_with_timeout \
    "xcodebuild -resolvePackageDependencies" \
    "$XCODE_PACKAGE_RESOLVE_TIMEOUT_SECONDS" \
    resolve_xcode_packages_once; then
    break
  fi

  if [ "$attempt" -lt "$XCODE_PACKAGE_RESOLVE_ATTEMPTS" ]; then
    echo "xcodebuild package resolution failed or timed out; cleaning package state before retry" >&2
    make_xcode_caches_writable
    clean_xcode_package_state
  else
    echo "error: xcodebuild package resolution failed after ${XCODE_PACKAGE_RESOLVE_ATTEMPTS} attempt(s)" >&2
    exit 1
  fi
done

# Resolve the simulator at runtime instead of hardcoding a device name.
# History: the hardcoded name has broken CI twice (iPhone 16 deleted from the
# runner, then iPhone 17 never existed on it). Preference order:
#   1. explicit $TEST_DESTINATION (caller override, unchanged contract)
#   2. a simulator whose name ends in " ci" (the runner's purpose-built sim)
#   3. any available iPhone simulator
if [ -z "${TEST_DESTINATION:-}" ]; then
  available="$(xcrun simctl list devices available)"
  udid="$(printf '%s\n' "$available" | sed -n 's/.* ci (\([0-9A-F-]\{36\}\)) .*/\1/p' | head -1)"
  if [ -z "$udid" ]; then
    udid="$(printf '%s\n' "$available" | sed -n 's/^ *iPhone[^(]*(\([0-9A-F-]\{36\}\)) .*/\1/p' | head -1)"
  fi
  if [ -z "$udid" ]; then
    echo "error: no available iPhone simulator on this runner" >&2
    xcrun simctl list devices available >&2
    exit 70
  fi
  TEST_DESTINATION="platform=iOS Simulator,id=$udid"
fi
echo "Using test destination: $TEST_DESTINATION"

# Simulator udid for inter-chunk resets (derived from the resolved destination).
udid="${udid:-$(printf '%s' "$TEST_DESTINATION" | sed -n 's/.*id=\([0-9A-Fa-f-]\{36\}\).*/\1/p')}"

# One xcodebuild test invocation with the shared flags plus per-call test
# selection (-only-testing / -skip-testing). The first call builds; later calls
# reuse the build products (test-only) even after a simulator erase.
run_test_chunk() {
  echo "--- test chunk: $* ---"
  xcodebuild \
    "${XCODE_CONTAINER_ARGS[@]}" \
    -scheme "$APP_SCHEME" \
    -destination "$TEST_DESTINATION" \
    -skipPackagePluginValidation \
    -onlyUsePackageVersionsFromResolvedFile \
    -skipPackageUpdates \
    -packageCachePath "$XCODE_PACKAGE_CACHE_PATH" \
    -clonedSourcePackagesDirPath "$XCODE_CLONED_SOURCE_PACKAGES_DIR" \
    -retry-tests-on-failure \
    "$@" \
    test
}

# Erase the simulator between UI chunks so memory cannot accumulate across the
# full UI suite. Running all UI tests in one simulator session exhausted memory
# late in the run, SIGKILLing heavy tests and blowing the job timeout (#17).
# Each chunk re-seeds via --UITestSeed, so a wiped device is expected.
#
# run_test_chunk_with_retry wraps run_test_chunk with one full-chunk retry on a
# fresh simulator. xcodebuild can exit 65 even when -retry-tests-on-failure
# already re-ran all failing tests successfully — this happens when the test
# RUNNER itself crashes (e.g. duplicate XCTestSupport.framework entries from a
# newer simulator runtime cause a spurious crash early in the run). The runner
# restart re-runs the lost tests and they all pass, but xcodebuild still exits
# 65 because the initial session crashed. A single full-chunk retry on a wiped
# sim catches this class of flaky infrastructure failures without masking real
# test regressions (a genuine failure will fail again on retry).
run_test_chunk_with_retry() {
  local chunk_status=0
  run_test_chunk "$@" || chunk_status=$?
  if [ "$chunk_status" -ne 0 ]; then
    echo "--- chunk exited $chunk_status; resetting sim and retrying once ---" >&2
    reset_sim
    chunk_status=0
    run_test_chunk "$@" || chunk_status=$?
  fi
  return "$chunk_status"
}

reset_sim() {
  if [ -z "$udid" ]; then
    echo "warn: no simulator udid resolved; skipping inter-chunk reset" >&2
    return 0
  fi
  echo "--- resetting simulator $udid between UI chunks ---"
  xcrun simctl shutdown "$udid" >/dev/null 2>&1 || true
  xcrun simctl erase "$udid" >/dev/null 2>&1 || true
  xcrun simctl boot "$udid" >/dev/null 2>&1 || true
  # Wait for the device to finish booting before the next xcodebuild invocation.
  xcrun simctl bootstatus "$udid" -b >/dev/null 2>&1 || true
}

UI="${APP_SCHEME}UITests"

if [ -n "${SKIP_UI_TESTS:-}" ]; then
  # TestFlight deploy lane: gate the SHIP on the deterministic unit suite only,
  # not the simulator-flaky end-to-end playback UI tests.
  echo "SKIP_UI_TESTS set — unit tests only (skipping ${UI})"
  run_test_chunk -skip-testing:"${UI}"
else
  # UI-suite sharding (#17): unit suite + light UI run together; the heavy UI
  # classes (lifecycle/audio/agent/stress/download) run in their own fresh-sim
  # chunks. The catch-all chunk uses -skip-testing for every explicitly-sharded
  # class so any NEW UI class runs there automatically and is never dropped.
  HEAVY1=( -only-testing:"$UI/CoreJourneyUITests" -only-testing:"$UI/P0PlaybackUITests" )
  HEAVY2=( -only-testing:"$UI/AgentChatUITest" -only-testing:"$UI/StressUITests" -only-testing:"$UI/ClippingsFixUITests" -only-testing:"$UI/AutoDownloadUITests" )
  HEAVY3=( -only-testing:"$UI/DownloadUITests" -only-testing:"$UI/PlayerChaptersUITests" -only-testing:"$UI/QueueReorderUITests" -only-testing:"$UI/PlaybackSettingsUITests" -only-testing:"$UI/SubscribeViaRSSUITests" -only-testing:"$UI/NostrPublishUITests" )
  SKIP_HEAVY=( -skip-testing:"$UI/CoreJourneyUITests" -skip-testing:"$UI/P0PlaybackUITests" -skip-testing:"$UI/AgentChatUITest" -skip-testing:"$UI/StressUITests" -skip-testing:"$UI/ClippingsFixUITests" -skip-testing:"$UI/AutoDownloadUITests" -skip-testing:"$UI/DownloadUITests" -skip-testing:"$UI/PlayerChaptersUITests" -skip-testing:"$UI/QueueReorderUITests" -skip-testing:"$UI/PlaybackSettingsUITests" -skip-testing:"$UI/SubscribeViaRSSUITests" -skip-testing:"$UI/NostrPublishUITests" )

  TEST_STATUS=0
  # Chunk 0: unit suite + remaining light UI classes (this call builds).
  run_test_chunk_with_retry "${SKIP_HEAVY[@]}" || TEST_STATUS=$?
  # Chunk 1: heaviest lifecycle / audio UI.
  reset_sim
  run_test_chunk_with_retry "${HEAVY1[@]}" || TEST_STATUS=$?
  # Chunk 2: agent / stress / clippings / auto-download.
  reset_sim
  run_test_chunk_with_retry "${HEAVY2[@]}" || TEST_STATUS=$?
  # Chunk 3: download / chapters / queue-reorder / settings / subscribe / nostr.
  reset_sim
  run_test_chunk_with_retry "${HEAVY3[@]}" || TEST_STATUS=$?
  exit "$TEST_STATUS"
fi
