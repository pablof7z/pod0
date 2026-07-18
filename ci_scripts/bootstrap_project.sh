#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)

cd "$REPO_ROOT"

if ! command -v tuist >/dev/null 2>&1; then
  curl -Ls https://install.tuist.io | bash
fi

tuist generate --no-open
