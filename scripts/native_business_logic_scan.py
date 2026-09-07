"""Semantic native-source rules for the Rust business-logic boundary."""

from __future__ import annotations

from pathlib import Path
import re


IGNORED_SOURCE = re.compile(
    r'""".*?"""|"(?:\\.|[^"\\])*"|/\*.*?\*/|//[^\n]*',
    re.DOTALL,
)
RULES = (
    (
        "native_product_policy",
        re.compile(
            r"\b(?:class|struct|enum|actor|object)\s+"
            r"[A-Za-z_][A-Za-z0-9_]*(?:BusinessPolicy|ProductPolicy|"
            r"RetryPolicy|FallbackPolicy|SemanticPlanner|PolicyEngine|DecisionEngine)\b"
        ),
        "native product-policy declaration",
    ),
    (
        "direct_durable_write",
        re.compile(
            r"\b(?:ProductStateStore|DomainStateStore|DurableStateStore|"
            r"ActivityFactStore)\b[^\n]{0,120}\b(?:save|insert|update|delete|"
            r"append|commit|upsert|write)\s*\("
        ),
        "direct durable product-store mutation",
    ),
    (
        "semantic_fact_construction",
        re.compile(
            r"\b(?:DomainEventEnvelope|ActivityFact|ActivityEventEnvelope|"
            r"SemanticProductFact)\s*\("
        ),
        "native semantic-fact construction",
    ),
    (
        "direct_effect_dispatch",
        re.compile(
            r"\b(?:ExternalEffectDispatcher|HostEffectDispatcher|"
            r"ProductEffectDispatcher|UnsafeEffectExecutor)\b[^\n]{0,120}"
            r"\b(?:dispatch|execute|send|perform)\s*\("
        ),
        "direct external-effect dispatch",
    ),
    (
        "native_default_fallback_retry",
        re.compile(
            r"\b(?:productDefault|fallbackProvider|retryCount|retryPolicy|"
            r"fallbackPolicy)[A-Za-z0-9_]*\s*(?:=|\()"
        ),
        "native default, fallback, or retry policy",
    ),
    (
        "in_memory_only_authorization",
        re.compile(
            r"\b(?:authorizedEffects|authorizedRequests|pendingAuthorizedEffects)"
            r"\s*\.\s*(?:append|insert|add)\s*\("
        ),
        "in-memory-only effect authorization",
    ),
    (
        "stale_observation_acceptance",
        re.compile(r"\b(?:accept|apply|commit)StaleObservation\s*\("),
        "stale observation acceptance",
    ),
    (
        "restored_retired_writer",
        re.compile(
            r"\b(?:TranscriptStore|EpisodeAuditLogStore|"
            r"ScheduledAgentRunJobExecutor|FeedDiscoveryJobExecutor)\b"
        ),
        "restored retired native writer",
    ),
)


def code_only(source: str) -> str:
    """Mask comments and string contents while preserving line positions."""

    def mask(match: re.Match[str]) -> str:
        return "".join("\n" if character == "\n" else " " for character in match.group())

    return IGNORED_SOURCE.sub(mask, source)


def semantic_violations(path: Path) -> list[tuple[str, int, str]]:
    code = code_only(path.read_text(encoding="utf-8"))
    violations: list[tuple[str, int, str]] = []
    for rule_id, pattern, description in RULES:
        for match in pattern.finditer(code):
            line = code.count("\n", 0, match.start()) + 1
            violations.append((rule_id, line, description))
    return violations
