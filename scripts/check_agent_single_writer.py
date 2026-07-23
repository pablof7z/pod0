#!/usr/bin/env python3
"""Prevent retired Swift chat policy from becoming authoritative again."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


FORBIDDEN_FILES = (
    "App/Sources/Features/Agent/AgentChatSession.swift",
    "App/Sources/Features/Agent/AgentChatSession+Conversations.swift",
    "App/Sources/Features/Agent/AgentChatSession+Recall.swift",
    "App/Sources/Features/Agent/AgentChatSession+Turns.swift",
    "App/Sources/Features/Agent/AgentChatView.swift",
    "App/Sources/Features/Agent/AgentChatTranscriptView.swift",
    "App/Sources/Agent/AgentTools.swift",
    "App/Sources/Agent/AgentToolSchema.swift",
    "App/Sources/Agent/PodcastAgentToolDeps.swift",
)

FORBIDDEN_GLOBS = (
    "App/Sources/Agent/AgentTools*.swift",
    "App/Sources/Agent/AgentToolSchema*.swift",
    "App/Sources/Agent/PodcastAgentToolDeps*.swift",
    "App/Sources/Agent/PodcastAgentToolValues*.swift",
)

FORBIDDEN_CODE = (
    ("legacy-agent-session", re.compile(r"\bAgentChatSession\b")),
    ("legacy-agent-view", re.compile(r"\bAgentChatView\b")),
    ("legacy-agent-tools", re.compile(r"\bAgentTools\b")),
)


def strip_swift_comments(source: str) -> str:
    output: list[str] = []
    in_block = False
    index = 0
    while index < len(source):
        if in_block:
            if source.startswith("*/", index):
                in_block = False
                output.extend("  ")
                index += 2
            else:
                output.append("\n" if source[index] == "\n" else " ")
                index += 1
            continue
        if source.startswith("/*", index):
            in_block = True
            output.extend("  ")
            index += 2
            continue
        if source.startswith("//", index):
            while index < len(source) and source[index] != "\n":
                output.append(" ")
                index += 1
            continue
        output.append(source[index])
        index += 1
    return "".join(output)


def scan_source(source: str) -> list[tuple[str, int]]:
    code = strip_swift_comments(source)
    findings: list[tuple[str, int]] = []
    for rule, pattern in FORBIDDEN_CODE:
        for match in pattern.finditer(code):
            findings.append((rule, code.count("\n", 0, match.start()) + 1))
    return findings


def validate(root: Path) -> list[str]:
    errors = [path for path in FORBIDDEN_FILES if (root / path).exists()]
    for pattern in FORBIDDEN_GLOBS:
        errors.extend(
            f"{path.relative_to(root).as_posix()}: prohibited legacy agent policy file"
            for path in root.glob(pattern)
        )
    for path in sorted((root / "App/Sources").rglob("*.swift")):
        relative = path.relative_to(root).as_posix()
        for rule, line in scan_source(path.read_text(encoding="utf-8")):
            errors.append(f"{relative}:{line}: prohibited {rule}")

    required = {
        "App/Sources/App/RootView.swift": "SharedAgentChatView(",
        "App/Sources/Features/AgentChat/AskAgentView.swift": "SharedAgentConversationSession",
        "App/Sources/Core/CoreAgentToolSchemas.swift":
            "schemas(for definitions: [AgentToolDefinition])",
        "App/Sources/Core/CoreAgentHost.swift": "execution.toolDefinitions",
    }
    for path, marker in required.items():
        if marker not in (root / path).read_text(encoding="utf-8"):
            errors.append(f"{path}: missing shared-core chat marker {marker}")

    session = (root / "App/Sources/Core/SharedAgentConversationSession.swift").read_text(
        encoding="utf-8"
    )
    if "productProofTools" in session or "availableTools:" in session:
        errors.append(
            "SharedAgentConversationSession.swift: native tool-catalog ownership is forbidden"
        )
    encoder = (root / "App/Sources/Core/CoreAgentToolSchemas.swift").read_text(
        encoding="utf-8"
    )
    if re.search(r'case\s+\.(createNote|recordMemory|queryTranscripts)', encoder):
        errors.append(
            "CoreAgentToolSchemas.swift: native provider-neutral tool policy is forbidden"
        )
    return errors


def self_test() -> None:
    assert scan_source("let session: AgentChatSession") == [("legacy-agent-session", 1)]
    assert scan_source("AgentTools.dispatch(name: name)") == [("legacy-agent-tools", 1)]
    assert scan_source("// AgentChatSession\nlet shared = true") == []
    assert scan_source("SharedAgentChatView(session: session)") == []


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--root", default=str(Path(__file__).resolve().parents[1]))
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("Agent single-writer negative fixture passed")
        return 0

    errors = validate(Path(args.root).resolve())
    if errors:
        print("Agent single-writer policy failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Agent single-writer policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
