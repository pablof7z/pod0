#!/usr/bin/env bash
set -euo pipefail

# Check that source files don't exceed size limits.
# - Source files (non-test): hard limit 500 lines
# - Test files: hard limit 2000 lines
# - Exemptions: generated files, codegen output, umbrella C headers
#
# Pre-existing over-limit files are grandfathered below and excluded from the
# gate.  They should be split as follow-up work.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "Checking file sizes..."
echo ""

# ---------------------------------------------------------------------------
# Grandfathered source files: pre-existing violations when this gate was
# introduced.  Exclude from the check until they are split.
# ---------------------------------------------------------------------------
GRANDFATHERED_SOURCE=(
    "App/Sources/Agent/AgentTools+Podcast.swift"
    "App/Sources/Agent/LivePodcastInventoryAdapter.swift"
    "App/Sources/Bridge/AppStateStore+KernelActions.swift"
    "App/Sources/Bridge/AppStateStore+KernelProjection.swift"
    "android/Podcast/app/src/main/java/io/f7z/podcast/ui/EpisodeDetailScreen.kt"
    "apps/nmp-app-podcast/src/clip_handler.rs"
    "apps/nmp-app-podcast/src/ffi/actions/podcast_module.rs"
    "apps/nmp-app-podcast/src/inbox_handler_triage.rs"
    "apps/nmp-app-podcast/src/state/knowledge_search.rs"
    "apps/nmp-app-podcast/src/store/persistence.rs"
)

# Write grandfathered paths to a temp file for grep -Fxvf (fixed-string exact match).
# Using a temp file avoids regex-escaping issues (e.g. '+' in Swift file names).
GRANDFATHERED_FILE=$(mktemp)
printf '%s\n' "${GRANDFATHERED_SOURCE[@]}" > "$GRANDFATHERED_FILE"
trap 'rm -f "$GRANDFATHERED_FILE"' EXIT

# Track violations
SOURCE_VIOLATIONS=()
TEST_VIOLATIONS=()

# Function to check source files
check_source_files() {
    git -C "$REPO_ROOT" ls-files \
        -- '*.swift' '*.rs' '*.kt' '*.h' \
    | grep -v 'App/Sources/Bridge/Generated/' \
    | grep -v '\.generated\.swift$' \
    | grep -v 'NmpCore.*\.h$' \
    | grep -v '_tests\.rs$' \
    | grep -v '_tests_ext\.rs$' \
    | grep -v '_test\.rs$' \
    | grep -v '/tests\.rs$' \
    | grep -v '_test\.swift$' \
    | grep -v '_tests\.swift$' \
    | grep -v 'Tests\.swift$' \
    | grep -v 'Test\.swift$' \
    | grep -v 'Test\.kt$' \
    | grep -v '^AppTests/' \
    | grep -v '^AppUITests/' \
    | grep -Fxvf "$GRANDFATHERED_FILE" \
    | while read -r file; do
        line_count=$(wc -l < "$REPO_ROOT/$file")
        if (( line_count > 500 )); then
            echo "  ✗ $file: $line_count lines (exceeds 500-line hard limit)"
            SOURCE_VIOLATIONS+=("$file")
        fi
    done
}

# Function to check test files
check_test_files() {
    git -C "$REPO_ROOT" ls-files \
        -- '*.swift' '*.rs' '*.kt' '*.h' \
    | grep -E '(_tests\.rs|_tests_ext\.rs|_test\.rs|/tests\.rs|_test\.swift|_tests\.swift|Tests\.swift|Test\.swift|Test\.kt)$|^AppTests/|^AppUITests/' \
    | while read -r file; do
        line_count=$(wc -l < "$REPO_ROOT/$file")
        if (( line_count > 2000 )); then
            echo "  ✗ $file: $line_count lines (exceeds 2000-line test hard limit)"
            TEST_VIOLATIONS+=("$file")
        fi
    done
}

echo "Checking source files (hard limit: 500 lines)..."
source_results=$(check_source_files 2>&1 || true)
if [[ -n "$source_results" ]]; then
    echo "$source_results"
fi

echo ""
echo "Checking test files (hard limit: 2000 lines)..."
test_results=$(check_test_files 2>&1 || true)
if [[ -n "$test_results" ]]; then
    echo "$test_results"
fi

# Check for violations
if [[ -n "$source_results" ]] || [[ -n "$test_results" ]]; then
    echo ""
    echo "ERROR: File size violations detected. Split files and try again."
    exit 1
else
    echo "✓ All files are within size limits."
    exit 0
fi
