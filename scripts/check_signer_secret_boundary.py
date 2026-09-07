#!/usr/bin/env python3
"""Enforce NMP as Pod0's only Nostr identity and signing implementation."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys


ACTIVE_ROOTS = ("App/Sources", "rust/crates")
FORBIDDEN = (
    "CoreNostrSigner",
    "NostrSignerCredential",
    "SignNostrEvent",
    "SignerAccountId",
    "SignerStore",
    "P256K",
    "secp256k1",
    "pod0-nmp",
    "pod0_nmp",
)


def evaluate(sources: dict[str, str]) -> list[str]:
    errors: list[str] = []
    for path, source in sources.items():
        for token in FORBIDDEN:
            if token in source:
                errors.append(
                    f"{path}: {token!r} recreates identity/signing outside upstream NMP"
                )
    return errors


def self_test() -> None:
    assert not evaluate({"App/Sources/NMP/NMPClient.swift": "import NMP\nlet engine: NMPEngine"})
    for token in FORBIDDEN:
        assert evaluate({"App/Sources/Bad.swift": token}), token


def read_active_text_files(root: Path) -> dict[str, str]:
    sources: dict[str, str] = {}
    for active_root in ACTIVE_ROOTS:
        for path in (root / active_root).rglob("*"):
            if not path.is_file() or "target" in path.parts:
                continue
            payload = path.read_bytes()
            if b"\0" in payload:
                continue
            try:
                sources[str(path.relative_to(root))] = payload.decode("utf-8")
            except UnicodeDecodeError:
                continue
    return sources


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("NMP ownership boundary negative fixtures passed")
        return 0

    root = Path(args.root).resolve()
    sources = read_active_text_files(root)
    sources["Project.swift"] = (root / "Project.swift").read_text(encoding="utf-8")
    sources["rust/Cargo.toml"] = (root / "rust/Cargo.toml").read_text(encoding="utf-8")
    errors = evaluate(sources)
    nmp_client = sources.get("App/Sources/NMP/NMPClient.swift", "")
    if "import NMP" not in nmp_client or "NMPEngine" not in nmp_client:
        errors.append("App/Sources/NMP/NMPClient.swift: upstream NMPEngine adoption is required")
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print(f"NMP ownership boundary passed; {len(sources)} active source files scanned")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
