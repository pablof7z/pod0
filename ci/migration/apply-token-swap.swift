#!/usr/bin/env bash
# apply-token-swap.swift — Launcher wrapper for the SwiftPM-based token-swap CLI.
#
# This file has a .swift extension to match the M0.D milestone naming convention,
# but it is a bash script, not a Swift source file.  The real implementation is
# in Sources/apply-token-swap/main.swift, built via SwiftPM.
#
# SwiftSyntax cannot be imported in swift-script mode (swift <file>.swift);
# it requires a SwiftPM build.  This wrapper performs that build transparently.
#
# Usage (from the podcast-player repo root):
#   ./ci/migration/apply-token-swap.swift <file.swift> [--toml <path>]
#   ./ci/migration/apply-token-swap.swift --help
#
# Equivalent to:
#   swift run --package-path ci/migration apply-token-swap <args>

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec swift run --package-path "$SCRIPT_DIR" apply-token-swap "$@"
