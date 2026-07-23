#!/usr/bin/env bash
set -euo pipefail

APP_SCHEME="${APP_SCHEME:-Podcastr}"
APP_PRODUCT_NAME="${APP_PRODUCT_NAME:-$APP_SCHEME}"
PROJECT_PATH="${PROJECT_PATH:-Podcastr.xcodeproj}"
WIDGET_EXTENSION_NAME="${WIDGET_EXTENSION_NAME:-${APP_PRODUCT_NAME}Widget}"
BUILD_ROOT="${BUILD_ROOT:-$PWD/build/non-publishing}"
ARCHIVE_PATH="${ARCHIVE_PATH:-$BUILD_ROOT/Podcastr.xcarchive}"
DERIVED_DATA_PATH="${DERIVED_DATA_PATH:-$BUILD_ROOT/DerivedData}"
SOURCE_PACKAGES_PATH="${SOURCE_PACKAGES_PATH:-$PWD/.build/DerivedData/SourcePackages}"
EVIDENCE_PATH="${EVIDENCE_PATH:-$BUILD_ROOT/archive-evidence.txt}"

rm -rf "$ARCHIVE_PATH" "$DERIVED_DATA_PATH"
mkdir -p "$BUILD_ROOT" "$DERIVED_DATA_PATH"

xcodebuild \
  -project "$PROJECT_PATH" \
  -scheme "$APP_SCHEME" \
  -configuration Release \
  -destination "generic/platform=iOS" \
  -derivedDataPath "$DERIVED_DATA_PATH" \
  -archivePath "$ARCHIVE_PATH" \
  -clonedSourcePackagesDirPath "$SOURCE_PACKAGES_PATH" \
  -onlyUsePackageVersionsFromResolvedFile \
  -disableAutomaticPackageResolution \
  -skipPackagePluginValidation \
  -quiet \
  CODE_SIGNING_ALLOWED=NO \
  archive

archived_app="$ARCHIVE_PATH/Products/Applications/${APP_PRODUCT_NAME}.app"
archived_widget="$archived_app/PlugIns/${WIDGET_EXTENSION_NAME}.appex"
if [[ ! -d "$archived_app" || ! -d "$archived_widget" ]]; then
  echo "Archive is missing the app or widget product." >&2
  exit 1
fi

{
  echo "commit=$(git rev-parse HEAD)"
  echo "xcode=$(xcodebuild -version | tr '\n' ' ')"
  echo "tuist=$(tuist version)"
  echo "app_binary_sha256=$(shasum -a 256 "$archived_app/$APP_PRODUCT_NAME" | awk '{print $1}')"
  echo "widget_binary_sha256=$(shasum -a 256 "$archived_widget/$WIDGET_EXTENSION_NAME" | awk '{print $1}')"
} > "$EVIDENCE_PATH"

cat "$EVIDENCE_PATH"
