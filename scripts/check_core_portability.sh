#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
RUST_TOOLCHAIN=1.93.0
CARGO_NDK_VERSION=4.1.2
ANDROID_NDK_VERSION=26.3.11579264
ANDROID_API_LEVEL=23
ANDROID_RUST_TARGETS=(aarch64-linux-android x86_64-linux-android)

require_cargo_ndk() {
  if ! command -v cargo-ndk >/dev/null 2>&1; then
    echo "cargo-ndk $CARGO_NDK_VERSION is required" >&2
    return 1
  fi
  local installed
  installed=$(cargo ndk --version | awk '{print $2}')
  if [[ "$installed" != "$CARGO_NDK_VERSION" ]]; then
    echo "cargo-ndk $CARGO_NDK_VERSION is required; found $installed" >&2
    return 1
  fi
}

resolve_android_ndk() {
  local candidates=()
  [[ -n "${ANDROID_NDK_HOME:-}" ]] && candidates+=("$ANDROID_NDK_HOME")
  [[ -n "${ANDROID_NDK_ROOT:-}" ]] && candidates+=("$ANDROID_NDK_ROOT")
  [[ -n "${ANDROID_SDK_ROOT:-}" ]] && \
    candidates+=("$ANDROID_SDK_ROOT/ndk/$ANDROID_NDK_VERSION")
  [[ -n "${ANDROID_HOME:-}" ]] && \
    candidates+=("$ANDROID_HOME/ndk/$ANDROID_NDK_VERSION")
  candidates+=(
    "/opt/homebrew/share/android-commandlinetools/ndk/$ANDROID_NDK_VERSION"
    "/usr/local/share/android-commandlinetools/ndk/$ANDROID_NDK_VERSION"
  )

  local candidate revision
  for candidate in "${candidates[@]}"; do
    [[ -f "$candidate/source.properties" ]] || continue
    revision=$(sed -nE 's/^Pkg\.Revision[[:space:]]*=[[:space:]]*(.+)$/\1/p' \
      "$candidate/source.properties")
    if [[ "$revision" == "$ANDROID_NDK_VERSION" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  echo "Android NDK $ANDROID_NDK_VERSION is required" >&2
  echo "Install ndk;$ANDROID_NDK_VERSION or set ANDROID_NDK_HOME" >&2
  return 1
}

require_cargo_ndk
export ANDROID_NDK_HOME
ANDROID_NDK_HOME=$(resolve_android_ndk)

for target in "${ANDROID_RUST_TARGETS[@]}"; do
  rustup target add --toolchain "$RUST_TOOLCHAIN" "$target"
done

"$SCRIPT_DIR/build_pod0_core_apple.sh"

cd "$REPO_ROOT/rust"
for target in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios; do
  cargo build -p pod0-recall-index \
    --bin pod0-recall-index-benchmark --release --locked --target "$target"
done
cargo ndk -t arm64-v8a -P "$ANDROID_API_LEVEL" \
  check --workspace --all-targets --all-features --locked
cargo ndk -t arm64-v8a -t x86_64 -P "$ANDROID_API_LEVEL" \
  build -p pod0-recall-index \
  --bin pod0-recall-index-benchmark --release --locked

FACADE_VERSION=$(sed -nE 's/^version = "([^"]+)"$/\1/p' Cargo.toml | head -n 1)
SCHEMA_VERSION=$(sed -nE \
  's/^pub const CURRENT_SCHEMA_VERSION: u32 = ([0-9]+);$/\1/p' \
  crates/pod0-storage/src/model.rs)
ANDROID_OUTPUT="$REPO_ROOT/.build/pod0core/android/facade-$FACADE_VERSION-schema-$SCHEMA_VERSION"

cargo ndk -t arm64-v8a -t x86_64 -P "$ANDROID_API_LEVEL" \
  -o "$ANDROID_OUTPUT" build -p pod0-facade --release --locked

ARM_LIBRARY="$ANDROID_OUTPUT/arm64-v8a/libpod0_facade.so"
X86_LIBRARY="$ANDROID_OUTPUT/x86_64/libpod0_facade.so"
file "$ARM_LIBRARY" | grep -q "ARM aarch64"
file "$X86_LIBRARY" | grep -q "x86-64"
CARGO_OUTPUT=$(cargo metadata --format-version 1 --no-deps --locked \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
file "$CARGO_OUTPUT/aarch64-apple-ios/release/pod0-recall-index-benchmark" \
  | grep -q "Mach-O 64-bit executable arm64"
file "$CARGO_OUTPUT/aarch64-apple-ios-sim/release/pod0-recall-index-benchmark" \
  | grep -q "Mach-O 64-bit executable arm64"
file "$CARGO_OUTPUT/x86_64-apple-ios/release/pod0-recall-index-benchmark" \
  | grep -q "Mach-O 64-bit executable x86_64"
file "$CARGO_OUTPUT/aarch64-linux-android/release/pod0-recall-index-benchmark" \
  | grep -q "ARM aarch64"
file "$CARGO_OUTPUT/x86_64-linux-android/release/pod0-recall-index-benchmark" \
  | grep -q "x86-64"

echo "Apple device/simulator and Android API $ANDROID_API_LEVEL core portability passed"
echo "Recall-index evidence binaries compile for every guarded mobile target"
echo "Android artifacts: $ANDROID_OUTPUT"
