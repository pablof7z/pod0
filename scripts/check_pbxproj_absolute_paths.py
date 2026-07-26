#!/usr/bin/env python3
"""Reject machine-local absolute paths in the generated Xcode project.

`tuist generate` resolves `.relativeToRoot(".build/pod0core/...")` to an
absolute path when the xcframework exists only in the generating checkout,
which is always true inside a worktree. The result builds fine for whoever
generated it and breaks every other checkout. Both worktree-generated
`project.pbxproj` commits that have existed carried this bug, so it is a
workflow defect rather than an occasional slip.

The project legitimately uses `<group>`, `SOURCE_ROOT`, `BUILT_PRODUCTS_DIR`,
and `DEVELOPER_DIR`. It never needs `<absolute>`.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import subprocess
import sys


PROJECT = "Podcastr.xcodeproj/project.pbxproj"
ABSOLUTE_SOURCE_TREE = re.compile(r'sourceTree\s*=\s*"?<absolute>"?\s*;')
# Any home-directory path is machine-local by construction.
ABSOLUTE_PATH = re.compile(r'path\s*=\s*"?(/Users/|/home/|/private/var/folders/)[^";]*"?\s*;')


def evaluate(source: str) -> list[str]:
    errors: list[str] = []
    for index, line in enumerate(source.splitlines(), start=1):
        if ABSOLUTE_SOURCE_TREE.search(line):
            errors.append(
                f"{PROJECT}:{index}: absolute sourceTree; regenerate with the "
                f"xcframework present at the repo root, or rewrite to "
                f"sourceTree = SOURCE_ROOT"
            )
        match = ABSOLUTE_PATH.search(line)
        if match:
            errors.append(
                f"{PROJECT}:{index}: machine-local absolute path "
                f"{match.group(1)}…; only repo-relative paths are portable"
            )
    return errors


def self_test() -> None:
    clean = (
        '\t\tA /* Pod0CoreFFI.xcframework */ = {isa = PBXFileReference; '
        'path = .build/pod0core/Pod0CoreFFI.xcframework; sourceTree = SOURCE_ROOT; };\n'
        '\t\tB /* AppMain.swift */ = {isa = PBXFileReference; '
        'path = AppMain.swift; sourceTree = "<group>"; };\n'
        '\t\tC = {isa = PBXFileReference; sourceTree = BUILT_PRODUCTS_DIR; };\n'
        '\t\tD = {isa = PBXFileReference; sourceTree = DEVELOPER_DIR; };\n'
    )
    assert not evaluate(clean), evaluate(clean)

    # The exact shape both worktree-generated commits carried.
    worktree_generated = (
        '\t\tA /* Pod0CoreFFI.xcframework */ = {isa = PBXFileReference; '
        'name = Pod0CoreFFI.xcframework; path = "/Users/someone/Work/pod0/'
        '.claude/worktrees/feature/.build/pod0core/Pod0CoreFFI.xcframework"; '
        'sourceTree = "<absolute>"; };\n'
    )
    found = evaluate(worktree_generated)
    assert any("absolute sourceTree" in error for error in found), found
    assert any("machine-local absolute path" in error for error in found), found

    # Each half must fail on its own, so neither rule carries the other.
    assert evaluate('\t\tA = {isa = PBXFileReference; sourceTree = "<absolute>"; };\n')
    assert evaluate('\t\tA = {isa = PBXFileReference; path = "/home/ci/x.a"; };\n')
    assert evaluate('\t\tA = {isa = PBXFileReference; path = /Users/ci/x.a; };\n')


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Xcode project absolute-path negative fixtures passed")
        return 0

    root = Path(args.root).resolve()
    project = root / PROJECT
    sources = {"working tree": project.read_text(encoding="utf-8")}
    # A fix applied after `git add` lives only in the working tree, so a
    # working-tree-only check passes while the commit stays poisoned. CI reads
    # a clean checkout and is unaffected, but a local pre-commit run would give
    # false assurance — so read what is actually staged as well.
    staged = staged_project(root)
    if staged is not None and staged != sources["working tree"]:
        sources["git index"] = staged

    errors = [
        f"[{label}] {error}"
        for label, source in sources.items()
        for error in evaluate(source)
    ]
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print(
        "Xcode project contains no machine-local absolute paths "
        f"({', '.join(sources)})"
    )
    return 0


def staged_project(root: Path) -> str | None:
    """The staged `project.pbxproj`, or None outside git / when untracked."""
    try:
        result = subprocess.run(
            ["git", "-C", str(root), "show", f":{PROJECT}"],
            capture_output=True,
            check=False,
        )
    except (OSError, ValueError):
        return None
    if result.returncode != 0:
        return None
    return result.stdout.decode("utf-8", errors="replace")


if __name__ == "__main__":
    raise SystemExit(main())
