#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
NMP_PATH="$REPO_ROOT/Vendor/nmp"
NMP_TOOLCHAIN="nightly-2026-07-07"
NMP_BUILD_MODE="${NMP_BUILD_MODE:-sim-only}"

cd "$REPO_ROOT"
"$SCRIPT_DIR/verify_repository_dependencies.sh"
"$SCRIPT_DIR/verify_fail_closed_ingress.sh"
git submodule sync -- Vendor/nmp
git submodule update --init --recursive Vendor/nmp

expected_nmp_revision=$(tr -d '[:space:]' < "$REPO_ROOT/Vendor/nmp-revision.txt")
actual_nmp_revision=$(git -C "$NMP_PATH" rev-parse HEAD)
if [[ "$actual_nmp_revision" != "$expected_nmp_revision" ]]; then
  echo "error: Vendor/nmp is at $actual_nmp_revision, expected $expected_nmp_revision" >&2
  exit 1
fi
echo "NMP source revision: $actual_nmp_revision"
echo "NMP Rust toolchain: $NMP_TOOLCHAIN"

case "$NMP_BUILD_MODE" in
  all)
    nmp_build_argument=""
    nmp_targets=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin)
    ;;
  sim-only)
    nmp_build_argument="--sim-only"
    nmp_targets=(aarch64-apple-ios-sim x86_64-apple-ios aarch64-apple-darwin)
    ;;
  macos-only)
    nmp_build_argument="--macos-only"
    nmp_targets=(aarch64-apple-darwin)
    ;;
  *)
    echo "error: NMP_BUILD_MODE must be all, sim-only, or macos-only" >&2
    exit 2
    ;;
esac

if ! command -v rustup >/dev/null 2>&1; then
  echo "error: rustup is required to build the pinned NMP source" >&2
  exit 1
fi
rustup toolchain install "$NMP_TOOLCHAIN" --profile minimal
rustup target add --toolchain "$NMP_TOOLCHAIN" "${nmp_targets[@]}"

(
  cd "$NMP_PATH"
  if [[ -n "$nmp_build_argument" ]]; then
    RUSTUP_TOOLCHAIN="$NMP_TOOLCHAIN" \
      CARGO_TARGET_DIR="${NMP_CARGO_TARGET_DIR:-$REPO_ROOT/build/nmp-cargo}" \
      scripts/build-swift-xcframework.sh "$nmp_build_argument"
  else
    RUSTUP_TOOLCHAIN="$NMP_TOOLCHAIN" \
      CARGO_TARGET_DIR="${NMP_CARGO_TARGET_DIR:-$REPO_ROOT/build/nmp-cargo}" \
      scripts/build-swift-xcframework.sh
  fi
)

if ! command -v tuist >/dev/null 2>&1; then
  curl -Ls https://install.tuist.io | bash
fi

tuist generate --no-open
