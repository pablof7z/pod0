#!/usr/bin/env python3
"""Prevent recall provider payloads or response bodies from entering diagnostics."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys


PROVIDER_FILES = (
    "App/Sources/Knowledge/EmbeddingsClient.swift",
    "App/Sources/Knowledge/OllamaEmbeddingsClient.swift",
    "App/Sources/Knowledge/RerankerClient.swift",
)
FORBIDDEN = (
    "requestPayloadJSON",
    "responseContentPreview",
    "String(data: data, encoding:",
    "apiKey, privacy:",
    "text, privacy:",
    "query, privacy:",
)


def evaluate(sources: dict[str, str]) -> list[str]:
    errors: list[str] = []
    for path, source in sources.items():
        for token in FORBIDDEN:
            if token in source:
                errors.append(f"{path}: forbidden recall diagnostic token {token!r}")
    return errors


def self_test() -> None:
    safe = {
        path: "CostLedger.shared.log" if path != PROVIDER_FILES[2] else "status only"
        for path in PROVIDER_FILES
    }
    assert not evaluate(safe)
    unsafe = dict(safe)
    unsafe[PROVIDER_FILES[0]] += "\nrequestPayloadJSON: privateContent"
    assert any("requestPayloadJSON" in error for error in evaluate(unsafe))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Recall privacy negative fixtures passed")
        return 0

    root = Path(args.root).resolve()
    sources = {
        path: (root / path).read_text(encoding="utf-8")
        for path in PROVIDER_FILES
    }
    errors = evaluate(sources)
    if errors:
        print("Recall privacy policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Recall privacy policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
