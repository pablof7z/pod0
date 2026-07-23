#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
MODE="${1:-full}"

if [[ "$MODE" != "full" && "$MODE" != "--toolchain-only" ]]; then
  echo "Usage: $0 [--toolchain-only]" >&2
  exit 2
fi

require_command() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "Required command is unavailable: $command_name" >&2
    exit 1
  fi
}

require_command xcodebuild
require_command tuist
require_command python3

expected_xcode=$(tr -d '[:space:]' < "$REPO_ROOT/.xcode-version")
expected_xcode_build=$(tr -d '[:space:]' < "$REPO_ROOT/.xcode-build-version")
expected_tuist=$(
  awk '$1 == "tuist" { print $2 }' "$REPO_ROOT/.tool-versions"
)

if [[ -z "$expected_xcode" || -z "$expected_xcode_build" || -z "$expected_tuist" ]]; then
  echo "Tracked Apple toolchain declarations are incomplete." >&2
  exit 1
fi

xcode_output=$(xcodebuild -version)
actual_xcode=$(awk '/^Xcode / { print $2 }' <<< "$xcode_output")
actual_xcode_build=$(awk '/^Build version / { print $3 }' <<< "$xcode_output")
actual_tuist=$(tuist version)

if [[ "$actual_xcode" != "$expected_xcode" ]]; then
  echo "Xcode $expected_xcode is required; found $actual_xcode." >&2
  exit 1
fi
if [[ "$actual_xcode_build" != "$expected_xcode_build" ]]; then
  echo "Xcode build $expected_xcode_build is required; found $actual_xcode_build." >&2
  exit 1
fi
if [[ "$actual_tuist" != "$expected_tuist" ]]; then
  echo "Tuist $expected_tuist is required; found $actual_tuist." >&2
  exit 1
fi

python3 - "$REPO_ROOT/.github/workflows/test.yml" \
  "$REPO_ROOT/.github/workflows/testflight.yml" <<'PY'
import pathlib
import sys

test_path = pathlib.Path(sys.argv[1])
testflight_path = pathlib.Path(sys.argv[2])
test = test_path.read_text()
testflight = testflight_path.read_text()

if test.count("runs-on: macos-26") != 1:
    raise SystemExit("Test CI must use exactly one pinned macos-26 hosted runner.")
if test.count("run: ./ci_scripts/setup_hosted_ci_runner.sh") != 1:
    raise SystemExit("Test CI must install the pinned hosted toolchain exactly once.")
if "runs-on: self-hosted" in test:
    raise SystemExit("Ordinary Test CI must not depend on a repository runner.")

if testflight.count("runs-on: macos-26") != 2:
    raise SystemExit("TestFlight test and deploy jobs must use pinned macos-26 runners.")
if testflight.count("setup_hosted_ci_runner.sh") != 2:
    raise SystemExit("Both TestFlight jobs must install the pinned hosted toolchain.")
if "workflow_dispatch:" not in testflight or "\n  push:" in testflight:
    raise SystemExit("TestFlight must remain manual and must not run on push.")
if "if: ${{ inputs.confirm_upload }}" not in testflight:
    raise SystemExit("TestFlight deploy must require explicit upload confirmation.")
if "runs-on: self-hosted" in testflight:
    raise SystemExit("TestFlight must not depend on a repository runner.")
PY

if [[ "$MODE" == "--toolchain-only" ]]; then
  echo "Apple toolchain and hosted-runner inputs match the tracked release versions."
  exit 0
fi

canonical_lock="$REPO_ROOT/Config/SwiftPackages/Package.resolved"
project_lock="$REPO_ROOT/Podcastr.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved"
workspace_lock="$REPO_ROOT/Podcastr.xcworkspace/xcshareddata/swiftpm/Package.resolved"

python3 - "$canonical_lock" "$REPO_ROOT/Project.swift" <<'PY'
import json
import pathlib
import sys

lock_path = pathlib.Path(sys.argv[1])
manifest_path = pathlib.Path(sys.argv[2])
if not lock_path.is_file():
    raise SystemExit(f"Canonical Swift package lock is missing: {lock_path}")

payload = json.loads(lock_path.read_text())
expected = {
    "https://github.com/GigaBitcoin/secp256k1.swift": (
        "secp256k1.swift",
        "0.23.1",
        "cfab52e538683557259302c39ef25df60226eb30",
    ),
    "https://github.com/onevcat/Kingfisher": (
        "kingfisher",
        "8.9.0",
        "cf8be20d07654570554c8a8a4952bc8a5766a8b0",
    ),
}
pins = {
    pin["location"]: (
        pin["identity"],
        pin["state"].get("version"),
        pin["state"].get("revision"),
    )
    for pin in payload.get("pins", [])
}
if pins != expected:
    raise SystemExit(f"Canonical Swift package pins differ: {pins!r}")

manifest = manifest_path.read_text()
for location, (_, version, _) in expected.items():
    declaration = (
        f'url: "{location}",\n'
        f'            requirement: .exact("{version}")'
    )
    if declaration not in manifest:
        raise SystemExit(
            f"Project.swift does not declare the exact {version} pin for {location}"
        )
PY

for generated_lock in "$project_lock" "$workspace_lock"; do
  if [[ ! -f "$generated_lock" ]]; then
    echo "Generated Swift package lock is missing: $generated_lock" >&2
    exit 1
  fi
  if ! cmp -s "$canonical_lock" "$generated_lock"; then
    echo "Generated Swift package lock drifted: $generated_lock" >&2
    exit 1
  fi
done

echo "Apple release inputs match the tracked toolchain and package lock."
