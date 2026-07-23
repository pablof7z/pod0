#!/bin/zsh
set -euo pipefail

simulator_id="${1:-}"
if [[ -z "$simulator_id" ]]; then
  simulator_id="$(xcrun simctl list devices booted --json | jq -r '
    [.devices[][] | select(.state == "Booted")][0].udid // empty
  ')"
fi
if [[ -z "$simulator_id" ]]; then
  simulator_name="${SIMULATOR_NAME:-iPhone 17}"
  simulator_id="$(xcrun simctl list devices available --json | jq -r \
    --arg name "$simulator_name" '
      [.devices[][] | select(.isAvailable and .name == $name)][0].udid // empty
    ')"
fi
if [[ -z "$simulator_id" ]]; then
  echo "No available iOS simulator is available for process reconstruction." >&2
  exit 1
fi

booted_here=false
simulator_state="$(xcrun simctl list devices --json | jq -r \
  --arg udid "$simulator_id" '
    [.devices[][] | select(.udid == $udid)][0].state // empty
  ')"
if [[ "$simulator_state" == "Shutdown" ]]; then
  xcrun simctl boot "$simulator_id"
  xcrun simctl bootstatus "$simulator_id" -b
  booted_here=true
elif [[ "$simulator_state" != "Booted" ]]; then
  echo "Simulator $simulator_id is not ready: ${simulator_state:-unknown state}." >&2
  exit 1
fi
cleanup() {
  if [[ "$booted_here" == true ]]; then
    xcrun simctl shutdown "$simulator_id" 2>/dev/null || true
  fi
}
trap cleanup EXIT

bundle_id="${APP_BUNDLE_ID:-io.f7z.podcast}"
build_app_path="${APP_PATH:-$PWD/.build/DerivedData/Xcode/Build/Products/Debug-iphonesimulator/Podcastr.app}"
app_path="$(xcrun simctl get_app_container "$simulator_id" "$bundle_id" app 2>/dev/null || true)"
if [[ -d "$build_app_path" ]]; then
  xcrun simctl install "$simulator_id" "$build_app_path"
  app_path="$build_app_path"
elif [[ -z "$app_path" ]]; then
  if ! command -v xcodebuildmcp >/dev/null; then
    echo "Pod0 is not installed and xcodebuildmcp is unavailable to locate the build." >&2
    exit 1
  fi
  app_path="$(xcodebuildmcp simulator get-app-path \
    --workspace-path Podcastr.xcworkspace \
    --scheme Podcastr \
    --simulator-id "$simulator_id" \
    --platform 'iOS Simulator' \
    --output json | jq -r '.data.artifacts.appPath')"
  xcrun simctl install "$simulator_id" "$app_path"
fi
container="$(xcrun simctl get_app_container "$simulator_id" "$bundle_id" data)"
marker_dir="$container/Library/Application Support/podcastr/workflow-harness"

SIMCTL_CHILD_POD0_WORKFLOW_HARNESS_PHASE=seed \
  xcrun simctl launch --terminate-running-process "$simulator_id" "$bundle_id" >/dev/null
for _ in {1..100}; do
  [[ -f "$marker_dir/seed.json" ]] && break
  sleep 0.1
done
[[ -f "$marker_dir/seed.json" ]]
xcrun simctl terminate "$simulator_id" "$bundle_id" 2>/dev/null || true

SIMCTL_CHILD_POD0_WORKFLOW_HARNESS_PHASE=recover \
  xcrun simctl launch --terminate-running-process "$simulator_id" "$bundle_id" >/dev/null
for _ in {1..100}; do
  [[ -f "$marker_dir/recover.json" ]] && break
  sleep 0.1
done
[[ -f "$marker_dir/recover.json" ]]

jq -e '.phase == "recover" and .attempt == 2 and .state == "succeeded" and .firstLeaseToken != .recoveredLeaseToken' \
  "$marker_dir/recover.json" >/dev/null
echo "workflow process reconstruction passed"
