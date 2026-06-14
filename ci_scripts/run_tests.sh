#!/usr/bin/env bash
set -euo pipefail

APP_SCHEME="${APP_SCHEME:-Podcastr}"
PROJECT_PATH="${PROJECT_PATH:-Podcastr.xcodeproj}"
RUST_PACKAGE="${RUST_PACKAGE:-nmp-app-podcast}"
SIM_RUST_TARGET="aarch64-apple-ios-sim"

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

# Pre-build hygiene: the secp256k1 / P256K SwiftPM build-tool plugin writes its
# generated shared sources read-only. If a prior build was interrupted (e.g. a
# cancelled run on this self-hosted runner), those files survive read-only and
# the next build's `cp` over them fails "Permission denied", aborting the whole
# build → test → deploy pipeline. Make any existing Podcastr DerivedData
# writable so the plugin can overwrite its own cached outputs.
for dd in "$HOME/Library/Developer/Xcode/DerivedData/${APP_SCHEME}-"*; do
  [ -d "$dd" ] && chmod -R u+w "$dd" 2>/dev/null || true
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

# SKIP_UI_TESTS: when set, run the unit-test target only (skip PodcastrUITests).
# The TestFlight deploy lane sets this so a SHIP is gated on the deterministic
# unit suite, not the simulator-flaky end-to-end playback UI tests (audio-start
# timing, app lifecycle, a known peer-owned kernel resume-position bug). The
# regular `Test` workflow does NOT set it, so PRs still run the full UI suite.
SKIP_UI_ARG=""
if [ -n "${SKIP_UI_TESTS:-}" ]; then
  echo "SKIP_UI_TESTS set — unit tests only (skipping PodcastrUITests)"
  SKIP_UI_ARG="-skip-testing:${APP_SCHEME}UITests"
fi

xcodebuild \
  -project "$PROJECT_PATH" \
  -scheme "$APP_SCHEME" \
  -destination "$TEST_DESTINATION" \
  -skipPackagePluginValidation \
  ${SKIP_UI_ARG} \
  test
