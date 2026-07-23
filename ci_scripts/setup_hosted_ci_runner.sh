#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
MODE="${1:---with-android}"

if [[ "$MODE" != "--with-android" && "$MODE" != "--apple-only" ]]; then
  echo "Usage: $0 [--with-android|--apple-only]" >&2
  exit 2
fi

expected_xcode=$(tr -d '[:space:]' < "$REPO_ROOT/.xcode-version")
expected_tuist=$(
  awk '$1 == "tuist" { print $2 }' "$REPO_ROOT/.tool-versions"
)
expected_ndk=$(
  sed -nE 's/^ANDROID_NDK_VERSION=([0-9.]+)$/\1/p' \
    "$REPO_ROOT/scripts/check_core_portability.sh"
)

if [[ -z "$expected_xcode" || -z "$expected_tuist" || -z "$expected_ndk" ]]; then
  echo "Tracked hosted-runner toolchain declarations are incomplete." >&2
  exit 1
fi

current_xcode=$(xcodebuild -version | awk '/^Xcode / { print $2 }')
if [[ "$current_xcode" != "$expected_xcode" ]]; then
  pinned_xcode="/Applications/Xcode_${expected_xcode}.app/Contents/Developer"
  if [[ ! -d "$pinned_xcode" ]]; then
    echo "Pinned Xcode is unavailable: $pinned_xcode" >&2
    exit 1
  fi
  sudo xcode-select --switch "$pinned_xcode"
fi

install_tuist() {
  local version="$1"
  local checksum
  case "$version" in
    4.200.5)
      checksum=f9381b11bdc0b30a7e2a8a63f669dc0f69ba1ce40dc32f427c26ada4d346e58a
      ;;
    *)
      echo "No verified Tuist release checksum is tracked for $version." >&2
      return 1
      ;;
  esac

  if [[ -z "${RUNNER_TEMP:-}" || -z "${GITHUB_PATH:-}" ]]; then
    echo "RUNNER_TEMP and GITHUB_PATH are required to install Tuist." >&2
    return 1
  fi

  local install_root="$RUNNER_TEMP/pod0-tuist-$version"
  local archive_path="$install_root/tuist.zip"
  mkdir -p "$install_root"
  curl \
    --fail \
    --location \
    --silent \
    --show-error \
    "https://github.com/tuist/tuist/releases/download/$version/tuist.zip" \
    --output "$archive_path"
  printf '%s  %s\n' "$checksum" "$archive_path" | shasum -a 256 -c -
  unzip -q "$archive_path" -d "$install_root"
  chmod +x "$install_root/tuist"
  printf '%s\n' "$install_root" >> "$GITHUB_PATH"
  export PATH="$install_root:$PATH"
}

actual_tuist=$(tuist version 2>/dev/null || true)
if [[ "$actual_tuist" != "$expected_tuist" ]]; then
  install_tuist "$expected_tuist"
fi
if [[ "$(tuist version)" != "$expected_tuist" ]]; then
  echo "Tuist $expected_tuist was not installed successfully." >&2
  exit 1
fi

if [[ "$MODE" == "--with-android" ]]; then
  ndk_candidates=()
  [[ -n "${ANDROID_NDK_HOME:-}" ]] && ndk_candidates+=("$ANDROID_NDK_HOME")
  [[ -n "${ANDROID_NDK_ROOT:-}" ]] && ndk_candidates+=("$ANDROID_NDK_ROOT")
  [[ -n "${ANDROID_SDK_ROOT:-}" ]] && \
    ndk_candidates+=("$ANDROID_SDK_ROOT/ndk/$expected_ndk")
  [[ -n "${ANDROID_HOME:-}" ]] && \
    ndk_candidates+=("$ANDROID_HOME/ndk/$expected_ndk")
  ndk_candidates+=(
    "/opt/homebrew/share/android-commandlinetools/ndk/$expected_ndk"
    "/usr/local/share/android-commandlinetools/ndk/$expected_ndk"
  )

  ndk_path=""
  for candidate in "${ndk_candidates[@]}"; do
    if [[ -f "$candidate/source.properties" ]] && \
      grep -q "Pkg.Revision = $expected_ndk" "$candidate/source.properties"
    then
      ndk_path="$candidate"
      break
    fi
  done

  if [[ -z "$ndk_path" ]]; then
    android_root="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
    if [[ -z "$android_root" ]]; then
      echo "ANDROID_SDK_ROOT or ANDROID_HOME is required to install the NDK." >&2
      exit 1
    fi
    sdkmanager="$android_root/cmdline-tools/latest/bin/sdkmanager"
    if [[ ! -x "$sdkmanager" ]]; then
      echo "Android sdkmanager is unavailable: $sdkmanager" >&2
      exit 1
    fi
    "$sdkmanager" "ndk;$expected_ndk"
    ndk_path="$android_root/ndk/$expected_ndk"
  fi
  if ! grep -q "Pkg.Revision = $expected_ndk" "$ndk_path/source.properties"; then
    echo "Android NDK $expected_ndk was not installed successfully." >&2
    exit 1
  fi
fi

"$REPO_ROOT/scripts/check_apple_release_inputs.sh" --toolchain-only
echo "Hosted CI runner matches the tracked toolchain."
