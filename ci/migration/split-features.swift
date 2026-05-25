#!/usr/bin/env bash
# split-features.swift — Launcher wrapper for the SwiftPM-based split-features CLI.
#
# This file has a .swift extension to match the M0.D milestone naming convention,
# but it is a bash script.  The real implementation is in
# Sources/split-features/main.swift, built via SwiftPM.
#
# SwiftSyntax cannot be imported in swift-script mode (swift <file>.swift);
# it requires a SwiftPM build.  This wrapper performs that build transparently.
#
# Usage (from the podcast-player repo root):
#   ./ci/migration/split-features.swift <file.swift> <ClassName>
#   ./ci/migration/split-features.swift --help
#
# Equivalent to:
#   swift run --package-path ci/migration split-features <args>

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec swift run --package-path "$SCRIPT_DIR" split-features "$@"
