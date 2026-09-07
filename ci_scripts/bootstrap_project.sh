#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
SOURCE_PACKAGES_PATH="${SOURCE_PACKAGES_PATH:-$REPO_ROOT/.build/DerivedData/SourcePackages}"

cd "$REPO_ROOT"

./scripts/check_apple_release_inputs.sh --toolchain-only
./scripts/build_pod0_core_apple.sh

tuist generate --no-open
python3 scripts/normalize_pod0_core_project.py
./scripts/materialize_swift_package_lock.sh
xcodebuild \
  -resolvePackageDependencies \
  -project Podcastr.xcodeproj \
  -clonedSourcePackagesDirPath "$SOURCE_PACKAGES_PATH" \
  -onlyUsePackageVersionsFromResolvedFile \
  -skipPackageUpdates
./scripts/check_apple_release_inputs.sh
