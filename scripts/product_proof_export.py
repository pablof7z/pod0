#!/usr/bin/env python3
"""Validate Pod0's content-free product-signal export contract."""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any
from uuid import UUID


SCHEMA_VERSION = 1
PRIVACY_STATEMENT = (
    "Content-free local product signals; manually exported by the user."
)
SIGNAL_NAMES = {
    "appLaunch",
    "firstSubscription",
    "playStarted",
    "meaningfulListening",
    "resumeAttempt",
    "playbackError",
    "transcriptReady",
    "transcriptUsed",
    "recallAsked",
    "recallGrounded",
    "recallCitationOpened",
    "recallShadowParity",
    "noteCreated",
    "clipCreated",
    "agentTurnCompleted",
    "uncleanTermination",
    "dataLossEvidence",
}
OUTCOMES = {
    "started", "succeeded", "failed", "created", "ready", "used", "grounded",
    "noEvidence", "opened", "detected", "cancelled", "matched", "mismatched",
}
LATENCY_BUCKETS = {
    "under250Milliseconds",
    "milliseconds250To749",
    "milliseconds750To1999",
    "seconds2To4",
    "seconds5Plus",
}
FAILURE_CODES = {
    "missingCredential", "permissionDenied", "rateLimited", "offline", "network",
    "unsupportedFormat", "providerRecovery", "corruptArtifact", "cancelled",
    "invalidInput", "missingDependency", "unexpected",
}
TOP_LEVEL_FIELDS = {"schemaVersion", "privacy", "report", "signals"}
REPORT_FIELDS = {
    "schemaVersion", "generatedAt", "signalCount", "distinctActiveDays",
    "activatedAt", "counts",
}
REPORT_COUNT_FIELDS = {"name", "outcome", "count"}
SIGNAL_REQUIRED_FIELDS = {
    "schemaVersion", "id", "anonymousInstallID", "occurredAt", "name", "outcome",
}
SIGNAL_OPTIONAL_FIELDS = {"latencyBucket", "errorClass", "domainRevision"}


class CohortInputError(ValueError):
    """Raised when an export is invalid or contains unapproved fields."""


def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise CohortInputError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _integer(value: Any, label: str, minimum: int = 0) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise CohortInputError(f"{label} must be an integer >= {minimum}")
    return value


def _timestamp(value: Any, label: str) -> datetime:
    if not isinstance(value, str):
        raise CohortInputError(f"{label} must be an ISO-8601 string")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise CohortInputError(f"{label} is not valid ISO-8601") from error
    if parsed.tzinfo is None:
        raise CohortInputError(f"{label} must include a time-zone offset")
    return parsed.astimezone(timezone.utc)


def _uuid(value: Any, label: str) -> str:
    if not isinstance(value, str):
        raise CohortInputError(f"{label} must be a UUID string")
    try:
        return str(UUID(value))
    except ValueError as error:
        raise CohortInputError(f"{label} is not a UUID") from error


def _validate_fields(
    value: dict[str, Any],
    allowed: set[str],
    required: set[str],
    label: str,
) -> None:
    unknown = sorted(set(value) - allowed)
    missing = sorted(required - set(value))
    if unknown:
        raise CohortInputError(f"{label} has unexpected fields: {', '.join(unknown)}")
    if missing:
        raise CohortInputError(f"{label} is missing fields: {', '.join(missing)}")


def _normalize_signal(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise CohortInputError(f"{label} must be an object")
    _validate_fields(
        value,
        SIGNAL_REQUIRED_FIELDS | SIGNAL_OPTIONAL_FIELDS,
        SIGNAL_REQUIRED_FIELDS,
        label,
    )
    if _integer(value["schemaVersion"], f"{label}.schemaVersion") != SCHEMA_VERSION:
        raise CohortInputError(f"{label} uses an unsupported schema version")
    name = value["name"]
    outcome = value["outcome"]
    if name not in SIGNAL_NAMES or outcome not in OUTCOMES:
        raise CohortInputError(f"{label} has an unknown typed name or outcome")
    latency = value.get("latencyBucket")
    failure = value.get("errorClass")
    if latency is not None and latency not in LATENCY_BUCKETS:
        raise CohortInputError(f"{label} has an unknown latency bucket")
    if failure is not None and failure not in FAILURE_CODES:
        raise CohortInputError(f"{label} has an unknown error class")
    revision = value.get("domainRevision")
    if revision is not None:
        _integer(revision, f"{label}.domainRevision")
    return {
        "id": _uuid(value["id"], f"{label}.id"),
        "install": _uuid(value["anonymousInstallID"], f"{label}.anonymousInstallID"),
        "time": _timestamp(value["occurredAt"], f"{label}.occurredAt"),
        "name": name,
        "outcome": outcome,
        "latency": latency,
        "failure": failure,
        "revision": revision,
    }


def signals_from_archive(value: Any, label: str) -> list[dict[str, Any]]:
    if not isinstance(value, dict):
        raise CohortInputError(f"{label} must contain a JSON object")
    _validate_fields(value, TOP_LEVEL_FIELDS, TOP_LEVEL_FIELDS, label)
    if _integer(value["schemaVersion"], f"{label}.schemaVersion") != SCHEMA_VERSION:
        raise CohortInputError(f"{label} uses an unsupported schema version")
    if value["privacy"] != PRIVACY_STATEMENT:
        raise CohortInputError(f"{label} has an unexpected privacy declaration")
    report = value["report"]
    if not isinstance(report, dict):
        raise CohortInputError(f"{label}.report must be an object")
    _validate_fields(
        report,
        REPORT_FIELDS,
        REPORT_FIELDS - {"activatedAt"},
        f"{label}.report",
    )
    if _integer(report["schemaVersion"], f"{label}.report.schemaVersion") != SCHEMA_VERSION:
        raise CohortInputError(f"{label}.report uses an unsupported schema version")
    _timestamp(report["generatedAt"], f"{label}.report.generatedAt")
    _integer(report["distinctActiveDays"], f"{label}.report.distinctActiveDays")
    if "activatedAt" in report:
        _timestamp(report["activatedAt"], f"{label}.report.activatedAt")
    counts = report["counts"]
    if not isinstance(counts, list):
        raise CohortInputError(f"{label}.report.counts must be an array")
    for index, count in enumerate(counts):
        count_label = f"{label}.report.counts[{index}]"
        if not isinstance(count, dict):
            raise CohortInputError(f"{count_label} must be an object")
        _validate_fields(
            count,
            REPORT_COUNT_FIELDS,
            REPORT_COUNT_FIELDS,
            count_label,
        )
        if count["name"] not in SIGNAL_NAMES or count["outcome"] not in OUTCOMES:
            raise CohortInputError(f"{count_label} has an unknown typed value")
        _integer(count["count"], f"{count_label}.count")
    raw_signals = value["signals"]
    if not isinstance(raw_signals, list):
        raise CohortInputError(f"{label}.signals must be an array")
    if _integer(report["signalCount"], f"{label}.report.signalCount") != len(raw_signals):
        raise CohortInputError(f"{label}.report.signalCount does not match signals")
    signals = [
        _normalize_signal(signal, f"{label}.signals[{index}]")
        for index, signal in enumerate(raw_signals)
    ]
    if len({signal["install"] for signal in signals}) > 1:
        raise CohortInputError(f"{label} mixes multiple anonymous install IDs")
    return signals
