#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

fail() {
  echo "dependency architecture check failed: $1" >&2
  exit 1
}

revision=$(tr -d '[:space:]' < Vendor/nmp-revision.txt)
[[ "$revision" =~ ^[0-9a-f]{40}$ ]] || fail "Vendor/nmp-revision.txt is not a full Git revision"
gitlink=$(git ls-files --stage Vendor/nmp | awk '$1 == 160000 { print $2 }')
[[ -n "$gitlink" ]] || fail "Vendor/nmp is not recorded as a Git submodule"
[[ "$gitlink" == "$revision" ]] || fail "NMP revision file ($revision) disagrees with gitlink ($gitlink)"

app_revision=$(sed -nE \
  's/.*static let testedRevision = "([0-9a-f]{40})".*/\1/p' \
  App/Sources/NMP/Pod0NMPConfiguration.swift)
[[ "$app_revision" == "$revision" ]] \
  || fail "Pod0NMPBuild.testedRevision ($app_revision) disagrees with repository pin ($revision)"

grep -Fq 'url = https://github.com/pablof7z/nmp.git' .gitmodules \
  || fail "NMP submodule URL is not the canonical public repository"
if grep -Eq '\.local\(path: "\.\./|nostr-multi-platform|pablof7z/nmp[^"[:space:]]*\.git[^\n]*branch' Project.swift; then
  fail "Project.swift contains a sibling-only or floating NMP dependency"
fi

echo "Repository dependency architecture verified (NMP $revision)"
