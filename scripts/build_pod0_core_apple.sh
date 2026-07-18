#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
OUTPUT_ROOT="$REPO_ROOT/.build/pod0core/Pod0CoreFFI.xcframework"
TEMP_ROOT=$(mktemp -d /tmp/pod0-core-apple.XXXXXX)
TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim)

cleanup() {
  case "$TEMP_ROOT" in
    /tmp/pod0-core-apple.*) rm -rf "$TEMP_ROOT" ;;
  esac
}
trap cleanup EXIT

cd "$REPO_ROOT/rust"
for target in "${TARGETS[@]}"; do
  rustup target add --toolchain 1.93.0 "$target"
  cargo rustc -p pod0-facade --release --locked --target "$target" --crate-type staticlib
done
CARGO_OUTPUT=$(cargo metadata --format-version 1 --no-deps --locked \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')

mkdir -p \
  "$TEMP_ROOT/ios-arm64/Headers" \
  "$TEMP_ROOT/ios-arm64-simulator/Headers"
cp "$CARGO_OUTPUT/aarch64-apple-ios/release/libpod0_facade.a" \
  "$TEMP_ROOT/ios-arm64/libpod0_facade.a"
cp "$CARGO_OUTPUT/aarch64-apple-ios-sim/release/libpod0_facade.a" \
  "$TEMP_ROOT/ios-arm64-simulator/libpod0_facade.a"

for identifier in ios-arm64 ios-arm64-simulator; do
  cp "$REPO_ROOT"/Generated/Pod0Core/Swift/*.h "$TEMP_ROOT/$identifier/Headers/"
  cp "$REPO_ROOT/rust/apple/Pod0CoreFFI.modulemap" \
    "$TEMP_ROOT/$identifier/Headers/module.modulemap"
done
cp "$REPO_ROOT/rust/apple/Pod0CoreFFI.xcframework.Info.plist" "$TEMP_ROOT/Info.plist"

plutil -lint "$TEMP_ROOT/Info.plist"
lipo "$TEMP_ROOT/ios-arm64/libpod0_facade.a" -verify_arch arm64
lipo "$TEMP_ROOT/ios-arm64-simulator/libpod0_facade.a" -verify_arch arm64
mkdir -p "$OUTPUT_ROOT"
rsync -a --delete "$TEMP_ROOT/" "$OUTPUT_ROOT/"
echo "Built $OUTPUT_ROOT"
