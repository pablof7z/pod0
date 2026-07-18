#!/usr/bin/env python3
"""Reject platform leakage and dynamic payloads in the shared facade contract."""

from __future__ import annotations

from pathlib import Path
import re
import sys


FORBIDDEN_TOKENS = (
    "UIKit",
    "SwiftUI",
    "AVFoundation",
    "URLSession",
    "Foundation.Date",
    "android.",
    "androidx.",
    "Media3",
    "java.time",
    "serde_json",
    "JsonValue",
)


def validate_text(relative: str, text: str) -> list[str]:
    errors: list[str] = []
    for token in FORBIDDEN_TOKENS:
        if token in text:
            errors.append(f"{relative}: shared facade contains forbidden token {token!r}")

    trait = re.search(
        r"pub trait Pod0ApplicationApi[^\{]*\{(?P<body>.*?)\n\}", text, re.DOTALL
    )
    if trait:
        body = trait.group("body")
        if "Result<" in body:
            errors.append(
                f"{relative}: per-operation Result must not cross Pod0ApplicationApi"
            )
        if re.search(r"\b(poll|next_projection|sleep)\b", body, re.IGNORECASE):
            errors.append(f"{relative}: native projection polling is forbidden")
    return errors


def validate(root: Path) -> list[str]:
    sources = (
        root / "rust/crates/pod0-application/src/contract.rs",
        root / "rust/crates/pod0-application/src/effects.rs",
        root / "rust/crates/pod0-facade/src/lib.rs",
    )
    errors: list[str] = []
    for source in sources:
        relative = source.relative_to(root).as_posix()
        errors.extend(validate_text(relative, source.read_text(encoding="utf-8")))
    return errors


def self_test() -> None:
    fixture = """
pub trait Pod0ApplicationApi {
    fn poll(&self) -> Result<serde_json::Value, AVFoundation>;
}
"""
    errors = validate_text("fixture.rs", fixture)
    assert any("AVFoundation" in error for error in errors)
    assert any("serde_json" in error for error in errors)
    assert any("per-operation Result" in error for error in errors)
    assert any("polling" in error for error in errors)


def main() -> int:
    if "--self-test" in sys.argv:
        self_test()
        print("Rust facade-boundary negative fixture passed")
        return 0
    root = Path(__file__).resolve().parents[1]
    errors = validate(root)
    if errors:
        print("Rust facade boundary failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Rust facade boundary passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
