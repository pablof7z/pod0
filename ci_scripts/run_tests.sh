#!/usr/bin/env bash
set -euo pipefail

APP_SCHEME="${APP_SCHEME:-Podcastr}"
PROJECT_PATH="${PROJECT_PATH:-Podcastr.xcodeproj}"
TEST_DESTINATION="${TEST_DESTINATION:-platform=iOS Simulator,name=iPhone 17,OS=latest}"

xcodebuild \
  -project "$PROJECT_PATH" \
  -scheme "$APP_SCHEME" \
  -destination "$TEST_DESTINATION" \
  -skipPackagePluginValidation \
  test
