#!/usr/bin/env bash
set -euo pipefail

require_env() {
  local name="$1"
  : "${!name:?${name} is required}"
}

set_plist_string() {
  local plist_path="$1"
  local key="$2"
  local value="$3"
  /usr/libexec/PlistBuddy -c "Set :${key} ${value}" "$plist_path" >/dev/null 2>&1 || \
    /usr/libexec/PlistBuddy -c "Add :${key} string ${value}" "$plist_path"
}

plist_value() {
  local plist_path="$1"
  local key="$2"
  /usr/libexec/PlistBuddy -c "Print :${key}" "$plist_path"
}

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "Required file not found: $path" >&2
    exit 1
  fi
}

assert_bundle_version() {
  local plist_path="$1"
  local label="$2"
  local marketing_version
  local build_number

  marketing_version="$(plist_value "$plist_path" "CFBundleShortVersionString")"
  build_number="$(plist_value "$plist_path" "CFBundleVersion")"

  if [[ "$marketing_version" != "$MARKETING_VERSION" ]] || [[ "$build_number" != "$BUILD_NUMBER" ]]; then
    echo "${label} has version ${marketing_version} (${build_number}); expected ${MARKETING_VERSION} (${BUILD_NUMBER})." >&2
    exit 1
  fi
}

require_env APP_STORE_CONNECT_KEY_ID
require_env APP_STORE_CONNECT_ISSUER_ID
require_env APP_STORE_CONNECT_API_KEY_P8

APP_SCHEME="${APP_SCHEME:-Podcastr}"
APP_PRODUCT_NAME="${APP_PRODUCT_NAME:-$APP_SCHEME}"
PROJECT_PATH="${PROJECT_PATH:-Podcastr.xcodeproj}"
APP_INFO_PLIST="${APP_INFO_PLIST:-App/Resources/Info.plist}"
WIDGET_INFO_PLIST="${WIDGET_INFO_PLIST:-App/Widget/Resources/Info.plist}"
WIDGET_EXTENSION_NAME="${WIDGET_EXTENSION_NAME:-${APP_PRODUCT_NAME}Widget}"
APPLE_TEAM_ID="${APPLE_TEAM_ID:-456SHKPP26}"
BUILD_ROOT="${BUILD_ROOT:-$PWD/build}"
ARCHIVE_PATH="${ARCHIVE_PATH:-$BUILD_ROOT/Podcastr.xcarchive}"
EXPORT_PATH="${EXPORT_PATH:-$BUILD_ROOT/testflight-export}"
EXPORT_OPTIONS_PLIST="${EXPORT_OPTIONS_PLIST:-$BUILD_ROOT/ExportOptions.plist}"
DERIVED_DATA_PATH="${DERIVED_DATA_PATH:-$BUILD_ROOT/DerivedData}"
AUTH_KEY_DIR="$HOME/.appstoreconnect/private_keys"
AUTH_KEY_PATH="$AUTH_KEY_DIR/AuthKey_${APP_STORE_CONNECT_KEY_ID}.p8"

cleanup_auth_key() {
  if [[ -f "$AUTH_KEY_PATH" ]]; then
    rm -f "$AUTH_KEY_PATH"
    rmdir "$AUTH_KEY_DIR" 2>/dev/null || true
    echo "Removed temporary App Store Connect API key."
  fi
}
trap cleanup_auth_key EXIT

MARKETING_VERSION="${MARKETING_VERSION:-}"
if [[ -z "$MARKETING_VERSION" ]]; then
  MARKETING_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_INFO_PLIST")"
fi

BUILD_NUMBER="${BUILD_NUMBER:-$(date -u +%Y%m%d%H%M)}"
VERSION_PLISTS=("$APP_INFO_PLIST" "$WIDGET_INFO_PLIST")

mkdir -p "$BUILD_ROOT" "$DERIVED_DATA_PATH"
rm -rf "$ARCHIVE_PATH" "$EXPORT_PATH"
mkdir -p "$EXPORT_PATH"

mkdir -p "$AUTH_KEY_DIR"
printf '%s' "$APP_STORE_CONNECT_API_KEY_P8" > "$AUTH_KEY_PATH"
chmod 600 "$AUTH_KEY_PATH"

for info_plist in "${VERSION_PLISTS[@]}"; do
  require_file "$info_plist"
  set_plist_string "$info_plist" "CFBundleShortVersionString" "$MARKETING_VERSION"
  set_plist_string "$info_plist" "CFBundleVersion" "$BUILD_NUMBER"
done

SIGNING_STYLE="automatic"
CODE_SIGN_ARGS=()
if [[ -n "${KEYCHAIN_PATH:-}" ]]; then
  SIGNING_STYLE="manual"
  CODE_SIGN_ARGS=(
    CODE_SIGN_STYLE=Manual
    "CODE_SIGN_IDENTITY=Apple Distribution"
    "CI_APP_PROFILE_SPECIFIER=${CI_APP_PROFILE_SPECIFIER:-}"
    "CI_WIDGET_PROFILE_SPECIFIER=${CI_WIDGET_PROFILE_SPECIFIER:-}"
  )
fi

PROVISIONING_PROFILES_XML=""
if [[ "$SIGNING_STYLE" == "manual" ]] && [[ -n "${CI_APP_PROFILE_SPECIFIER:-}" ]]; then
  APP_BUNDLE_ID="${APP_BUNDLE_ID:-io.f7z.podcast}"
  WIDGET_BUNDLE_ID="${WIDGET_BUNDLE_ID:-io.f7z.podcast.widget}"
  PROVISIONING_PROFILES_XML="
  <key>provisioningProfiles</key>
  <dict>
    <key>${APP_BUNDLE_ID}</key>
    <string>${CI_APP_PROFILE_SPECIFIER}</string>"
  if [[ -n "${CI_WIDGET_PROFILE_SPECIFIER:-}" ]]; then
    PROVISIONING_PROFILES_XML="${PROVISIONING_PROFILES_XML}
    <key>${WIDGET_BUNDLE_ID}</key>
    <string>${CI_WIDGET_PROFILE_SPECIFIER}</string>"
  fi
  PROVISIONING_PROFILES_XML="${PROVISIONING_PROFILES_XML}
  </dict>"
fi

cat > "$EXPORT_OPTIONS_PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>destination</key>
  <string>export</string>
  <key>method</key>
  <string>app-store-connect</string>
  <key>signingStyle</key>
  <string>${SIGNING_STYLE}</string>
  <key>stripSwiftSymbols</key>
  <true/>
  <key>teamID</key>
  <string>${APPLE_TEAM_ID}</string>
  <key>uploadSymbols</key>
  <true/>${PROVISIONING_PROFILES_XML}
</dict>
</plist>
EOF

echo "Archiving ${APP_SCHEME} ${MARKETING_VERSION} (${BUILD_NUMBER}) for TestFlight."

xcodebuild \
  -project "$PROJECT_PATH" \
  -scheme "$APP_SCHEME" \
  -configuration Release \
  -destination "generic/platform=iOS" \
  -derivedDataPath "$DERIVED_DATA_PATH" \
  -archivePath "$ARCHIVE_PATH" \
  -skipPackagePluginValidation \
  "DEVELOPMENT_TEAM=${APPLE_TEAM_ID}" \
  archive \
  ${CODE_SIGN_ARGS[@]+"${CODE_SIGN_ARGS[@]}"}

ARCHIVED_APP_INFO_PLIST="$ARCHIVE_PATH/Products/Applications/${APP_PRODUCT_NAME}.app/Info.plist"
ARCHIVED_WIDGET_INFO_PLIST="$ARCHIVE_PATH/Products/Applications/${APP_PRODUCT_NAME}.app/PlugIns/${WIDGET_EXTENSION_NAME}.appex/Info.plist"
require_file "$ARCHIVED_APP_INFO_PLIST"
require_file "$ARCHIVED_WIDGET_INFO_PLIST"
assert_bundle_version "$ARCHIVED_APP_INFO_PLIST" "App archive"
assert_bundle_version "$ARCHIVED_WIDGET_INFO_PLIST" "Widget archive"

xcodebuild \
  -exportArchive \
  -archivePath "$ARCHIVE_PATH" \
  -exportPath "$EXPORT_PATH" \
  -exportOptionsPlist "$EXPORT_OPTIONS_PLIST"

IPA_PATH="$(find "$EXPORT_PATH" -maxdepth 1 -name '*.ipa' -print -quit)"
if [[ -z "$IPA_PATH" ]]; then
  echo "No IPA found in $EXPORT_PATH." >&2
  exit 1
fi

upload_cmd=(
  xcrun altool
  --upload-app
  --type ios
  --file "$IPA_PATH"
  --apiKey "$APP_STORE_CONNECT_KEY_ID"
  --apiIssuer "$APP_STORE_CONNECT_ISSUER_ID"
  --output-format xml
)

if [[ -n "${APP_STORE_CONNECT_PROVIDER:-}" ]]; then
  upload_cmd+=(--asc-provider "$APP_STORE_CONNECT_PROVIDER")
fi

"${upload_cmd[@]}"

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  {
    echo "build_number=${BUILD_NUMBER}"
    echo "marketing_version=${MARKETING_VERSION}"
    echo "ipa_path=${IPA_PATH}"
  } >> "$GITHUB_OUTPUT"
fi
