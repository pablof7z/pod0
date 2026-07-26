#!/usr/bin/env python3
"""Keep Nostr signing key material inside native secure custody.

#137 requires that secrets never reach source, logs, snapshots, or generated
bindings. That holds today only because two files touch the private key and
neither logs. This ratchet makes it stay true: private key material may be
named only in the custody files, must never be logged or interpolated, and must
never appear in a generated binding that crosses the FFI.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


# The only production files permitted to name private signing material.
CUSTODY_FILES = (
    "App/Sources/Core/CoreNostrSignerHost.swift",
    "App/Sources/Core/CoreNostrSignerCrypto.swift",
)
SECRET_TOKENS = ("privateKeyHex", "privateKeyData", "dataRepresentation)")
# Emitting a secret is always one of these, so scan custody files for them.
LEAK_PATTERNS = (
    re.compile(r"(?:Logger|logger|os_log|print|NSLog|debugPrint)\b"),
    re.compile(r'"[^"\n]*\\\(\s*\w*[Pp]rivateKey\w*'),
    re.compile(r"privacy:\s*\.public"),
)
BINDING_DIRS = ("Generated/Pod0Core/Swift", "Generated/Pod0Core/Kotlin")
BINDING_TOKENS = ("privateKeyHex", "privateKey", "secretKey", "nsec")


def evaluate_custody(sources: dict[str, str]) -> list[str]:
    errors: list[str] = []
    for path, source in sources.items():
        for pattern in LEAK_PATTERNS:
            match = pattern.search(source)
            if match:
                errors.append(
                    f"{path}: signer custody must not emit key material "
                    f"({match.group(0)!r})"
                )
    return errors


def evaluate_spread(other_sources: dict[str, str]) -> list[str]:
    errors: list[str] = []
    for path, source in other_sources.items():
        for token in ("privateKeyHex", "privateKeyData"):
            if token in source:
                errors.append(
                    f"{path}: private signing material may live only in "
                    f"{', '.join(CUSTODY_FILES)}"
                )
                break
    return errors


def evaluate_bindings(bindings: dict[str, str]) -> list[str]:
    errors: list[str] = []
    for path, source in bindings.items():
        for token in BINDING_TOKENS:
            if re.search(rf"\b{re.escape(token)}\b", source):
                errors.append(f"{path}: generated bindings must not carry {token!r}")
    return errors


def self_test() -> None:
    safe = {path: "let key = credential.privateKeyHex" for path in CUSTODY_FILES}
    assert not evaluate_custody(safe)
    for leak in (
        'Logger.app("signer")',
        'let message = "key \\(privateKeyHex)"',
        "logger.error(\"\\(value, privacy: .public)\")",
    ):
        unsafe = dict(safe)
        unsafe[CUSTODY_FILES[0]] += "\n" + leak
        assert evaluate_custody(unsafe), leak
    assert evaluate_spread({"App/Sources/Other.swift": "privateKeyHex"})
    assert not evaluate_spread({"App/Sources/Other.swift": "publicKeyHex"})
    assert evaluate_bindings({"Generated/x.swift": "public let privateKey: String"})
    assert not evaluate_bindings({"Generated/x.swift": "public let publicKeyHex: String"})


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Signer secret boundary negative fixtures passed")
        return 0

    root = Path(args.root).resolve()
    custody = {
        path: (root / path).read_text(encoding="utf-8") for path in CUSTODY_FILES
    }
    others = {
        str(swift.relative_to(root)): swift.read_text(encoding="utf-8")
        for swift in (root / "App/Sources").rglob("*.swift")
        if str(swift.relative_to(root)) not in CUSTODY_FILES
    }
    bindings = {
        str(generated.relative_to(root)): generated.read_text(encoding="utf-8")
        for directory in BINDING_DIRS
        for generated in (root / directory).rglob("*")
        if generated.is_file()
    }

    errors = (
        evaluate_custody(custody)
        + evaluate_spread(others)
        + evaluate_bindings(bindings)
    )
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print(
        f"Signer secret boundary passed; {len(custody)} custody files, "
        f"{len(bindings)} generated bindings scanned"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
