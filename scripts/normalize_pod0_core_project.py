#!/usr/bin/env python3
"""Make Tuist's local XCFramework reference checkout-path independent."""

from __future__ import annotations

import re
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
PROJECT_FILE = REPO_ROOT / "Podcastr.xcodeproj" / "project.pbxproj"
RELATIVE_ARTIFACT = ".build/pod0core/Pod0CoreFFI.xcframework"


def main() -> int:
    project = PROJECT_FILE.read_text(encoding="utf-8")
    pattern = re.compile(
        r"(name = Pod0CoreFFI\.xcframework; )"
        r'path = .*?; sourceTree = "<absolute>";'
    )
    replacement = (
        rf"\1path = {RELATIVE_ARTIFACT}; "
        "sourceTree = SOURCE_ROOT;"
    )
    normalized, replacement_count = pattern.subn(replacement, project)
    if replacement_count != 1:
        expected = (
            f"path = {RELATIVE_ARTIFACT}; "
            "sourceTree = SOURCE_ROOT;"
        )
        if project.count(expected) == 1:
            return 0
        raise SystemExit(
            "Expected exactly one absolute Pod0CoreFFI XCFramework reference, "
            f"found {replacement_count}"
        )
    PROJECT_FILE.write_text(normalized, encoding="utf-8")
    print("Normalized Pod0CoreFFI project reference to SOURCE_ROOT")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
