#!/usr/bin/env bash
set -euo pipefail

if ! command -v tuist >/dev/null 2>&1; then
  curl -Ls https://install.tuist.io | bash
fi

tuist generate --no-open
