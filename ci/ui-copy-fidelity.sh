#!/usr/bin/env bash
# ui-copy-fidelity.sh — Pre-commit alias for verify-copy-fidelity.sh.
# Delegates entirely to ci/migration/verify-copy-fidelity.sh.
#
# Usage:
#   ./ci/ui-copy-fidelity.sh [--nmp-root <path>]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/migration/verify-copy-fidelity.sh" "$@"
