#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
SOURCE="$REPO_ROOT/Config/SwiftPackages/Package.resolved"
DESTINATIONS=(
  "$REPO_ROOT/Podcastr.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved"
  "$REPO_ROOT/Podcastr.xcworkspace/xcshareddata/swiftpm/Package.resolved"
)

if [[ ! -f "$SOURCE" ]]; then
  echo "Canonical Swift package lock is missing: $SOURCE" >&2
  exit 1
fi

for destination in "${DESTINATIONS[@]}"; do
  mkdir -p "$(dirname "$destination")"
  install -m 0644 "$SOURCE" "$destination"
done
