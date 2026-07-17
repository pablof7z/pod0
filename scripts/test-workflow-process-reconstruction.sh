#!/bin/zsh
set -euo pipefail

simulator_id="${1:-F19BC699-69BF-4C1A-BCA7-381575E5DAA6}"
app_path="$(xcodebuildmcp simulator get-app-path --workspace-path Podcastr.xcworkspace --scheme Podcastr --simulator-id "$simulator_id" --platform 'iOS Simulator' --output json | jq -r '.data.artifacts.appPath')"
bundle_id="$(plutil -extract CFBundleIdentifier raw "$app_path/Info.plist")"

xcrun simctl install "$simulator_id" "$app_path"
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
