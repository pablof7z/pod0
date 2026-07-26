#!/usr/bin/env python3
"""Keep expensive show-note/search normalization out of SwiftUI render paths."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys
import tempfile


def findings(root: Path) -> list[str]:
    feature_root = root / "App/Sources/Features"
    errors: list[str] = []
    for path in feature_root.rglob("*View.swift"):
        text = path.read_text()
        if "EpisodeShowNotesFormatter." in text:
            errors.append(f"{path.relative_to(root)}: call cached Episode/Podcast text projections")
        if "PodcastSearchEngine.localResults(" in text:
            errors.append(f"{path.relative_to(root)}: run local search through its async view model")
    image_loader = root / "App/Sources/Design/CachedAsyncImage.swift"
    if image_loader.is_file() and ".isCached(" in image_loader.read_text():
        errors.append(
            "App/Sources/Design/CachedAsyncImage.swift: synchronous disk-cache probe is forbidden"
        )
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    if args.self_test:
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory)
            target = fixture / "App/Sources/Features/Search"
            target.mkdir(parents=True)
            (target / "BadView.swift").write_text(
                "let x = EpisodeShowNotesFormatter.plainText(from: raw)\n"
                "let y = PodcastSearchEngine.localResults(query: q, state: state)\n"
            )
            image_loader = fixture / "App/Sources/Design"
            image_loader.mkdir(parents=True)
            (image_loader / "CachedAsyncImage.swift").write_text("cache.isCached(forKey: key)\n")
            if len(findings(fixture)) != 3:
                print("Render-path normalization negative fixture failed")
                return 1
        print("Render-path normalization negative fixture passed")
        return 0
    errors = findings(root)
    if errors:
        print("\n".join(errors))
        return 1
    print("Render-path normalization boundary passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
