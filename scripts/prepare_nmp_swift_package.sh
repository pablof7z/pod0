#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
NMP_REVISION=bca64d75eeee8496b93ca220976c4fa6046cf6cb
NMP_CHECKOUT="$REPO_ROOT/.build/nmp"
NMP_REMOTE=https://github.com/pablof7z/nmp.git

if [[ ! -d "$NMP_CHECKOUT/.git" ]]; then
  git clone --filter=blob:none "$NMP_REMOTE" "$NMP_CHECKOUT"
fi

git -C "$NMP_CHECKOUT" fetch --depth=1 origin "$NMP_REVISION"
git -C "$NMP_CHECKOUT" checkout --detach "$NMP_REVISION"

cd "$NMP_CHECKOUT"
CARGO_TARGET_DIR="$REPO_ROOT/.build/nmp-swift-target" \
  "$NMP_CHECKOUT/scripts/build-swift-xcframework.sh"
