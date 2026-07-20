#!/usr/bin/env python3
"""Keep chapter navigation and ad-skip policy active only in Rust."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


def source_findings(controls: str, host: str, production_swift: str) -> list[str]:
    errors: list[str] = []
    if "autoSkipAds: settings.autoSkipAds" not in controls:
        errors.append("iOS must pass the user preference to Rust auto-skip policy")
    if "autoSkipAds: false" in controls:
        errors.append("iOS must not keep Rust auto-skip dormant after authority cutover")
    if not re.search(
        r"case\s+\.seek\(let episodeID, let positionMilliseconds, _, _\):",
        host,
    ):
        errors.append("native playback host must accept typed seek metadata mechanically")
    if "engine.seek(to: Self.seconds(positionMilliseconds))" not in host:
        errors.append("native playback host must execute the exact Rust millisecond target")
    if re.search(r"switch\s+(?:reason|chapterContext)", host):
        errors.append("native playback host must not interpret chapter seek policy")
    for action in ("nextChapter", "previousChapter"):
        if not re.search(rf"\.{action}\s*\(\s*context\s*:", production_swift):
            errors.append(f"production Swift must dispatch typed Rust action .{action}(context:)")
    for policy in ("applyAutoSkipAdsIfNeeded", "nextChapter(after:", "previousChapter(from:"):
        if policy in production_swift:
            errors.append(f"production Swift retains chapter policy {policy}")
    return errors


def validate(root: Path) -> list[str]:
    controls = (root / "App/Sources/Features/Player/PlaybackState+Controls.swift").read_text()
    host = (root / "App/Sources/Core/CorePlaybackHost.swift").read_text()
    production_swift = "\n".join(
        path.read_text(encoding="utf-8")
        for path in (root / "App/Sources").rglob("*.swift")
    )
    errors = source_findings(controls, host, production_swift)
    policy = (root / "rust/crates/pod0-domain/src/chapter_playback_policy.rs").read_text()
    runtime = (root / "rust/crates/pod0-facade/src/runtime_chapter_playback.rs").read_text()
    stream = (root / "rust/crates/pod0-facade/src/runtime_playback_host.rs").read_text()
    required = {
        "Rust next/previous decision": "decide_chapter_navigation" in policy,
        "Rust automatic ad decision": "decide_automatic_ad_skip" in policy,
        "typed chapter seek context": "chapter_context: Some(context)" in runtime,
        "one-second observation cadence": "minimum_interval_milliseconds: 1_000" in stream,
    }
    errors.extend(f"missing {name}" for name, present in required.items() if not present)
    return errors


def self_test() -> None:
    safe_controls = "autoSkipAds: settings.autoSkipAds"
    safe_host = (
        "case .seek(let episodeID, let positionMilliseconds, _, _):\n"
        "engine.seek(to: Self.seconds(positionMilliseconds))"
    )
    actions = ".nextChapter(context: value)\n.previousChapter(context: value)"
    assert not source_findings(safe_controls, safe_host, actions)
    assert source_findings("autoSkipAds: false", safe_host, actions)
    assert source_findings(safe_controls, "switch reason {}", actions)
    assert source_findings(safe_controls, safe_host, "")
    assert source_findings(safe_controls, safe_host, actions + "\napplyAutoSkipAdsIfNeeded()")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Chapter playback staging negative fixtures passed")
        return 0
    errors = validate(Path(args.root).resolve())
    if errors:
        print("Chapter playback staging failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Chapter playback staging boundary passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
