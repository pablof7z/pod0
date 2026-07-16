#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
SOURCE_URL="https://github.com/pablof7z/ios-shake-feedback"
SOURCE_VERSION="1.0.0"
SOURCE_PATH="$REPO_ROOT/build/dependency-sources/ios-shake-feedback"
STAGED_PATH="$REPO_ROOT/build/dependencies/ios-shake-feedback"
REVISION=$(tr -d '[:space:]' < "$REPO_ROOT/Vendor/shake-feedback-revision.txt")

if [[ ! "$REVISION" =~ ^[0-9a-f]{40}$ ]]; then
  echo "error: ShakeFeedbackKit revision must be a full Git commit" >&2
  exit 1
fi

if [[ ! -d "$SOURCE_PATH/.git" ]]; then
  git clone "$SOURCE_URL" "$SOURCE_PATH"
fi

if [[ -n "$(git -C "$SOURCE_PATH" status --short)" ]]; then
  echo "error: cached ShakeFeedbackKit source is dirty: $SOURCE_PATH" >&2
  exit 1
fi

git -C "$SOURCE_PATH" fetch --depth 1 origin "$REVISION"
git -C "$SOURCE_PATH" checkout --detach "$REVISION"

actual_revision=$(git -C "$SOURCE_PATH" rev-parse HEAD)
if [[ "$actual_revision" != "$REVISION" ]]; then
  echo "error: ShakeFeedbackKit is at $actual_revision, expected $REVISION" >&2
  exit 1
fi
actual_version=$(git -C "$SOURCE_PATH" describe --tags --exact-match HEAD)
if [[ "$actual_version" != "$SOURCE_VERSION" ]]; then
  echo "error: ShakeFeedbackKit revision is tag $actual_version, expected $SOURCE_VERSION" >&2
  exit 1
fi

rm -rf "$STAGED_PATH"
mkdir -p "$STAGED_PATH"
rsync -a --exclude .git "$SOURCE_PATH/" "$STAGED_PATH/"

headers_count=0
while IFS= read -r headers_path; do
  headers_count=$((headers_count + 1))
  if [[ ! -f "$headers_path/module.modulemap" || ! -f "$headers_path/shake_feedback_core.h" ]]; then
    echo "error: ShakeFeedbackKit $SOURCE_VERSION has an unexpected XCFramework header layout" >&2
    exit 1
  fi
  namespaced_path="$headers_path/ShakeFeedbackCoreFFI"
  mkdir -p "$namespaced_path"
  mv "$headers_path/module.modulemap" "$namespaced_path/module.modulemap"
  mv "$headers_path/shake_feedback_core.h" "$namespaced_path/shake_feedback_core.h"
done < <(find "$STAGED_PATH/Frameworks/ShakeFeedbackCore.xcframework" -type d -name Headers -print)

if [[ "$headers_count" -ne 2 ]]; then
  echo "error: ShakeFeedbackKit $SOURCE_VERSION has $headers_count XCFramework header directories; expected 2" >&2
  exit 1
fi

echo "Staged public ShakeFeedbackKit $SOURCE_VERSION ($REVISION) with namespaced XCFramework headers"
