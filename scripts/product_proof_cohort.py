#!/usr/bin/env python3
"""Aggregate validated Pod0 product signals into predeclared cohort measures."""

from __future__ import annotations

from collections import defaultdict
from datetime import datetime, timedelta, timezone
import math
from typing import Any, Iterable, Optional

from scripts.product_proof_export import (
    CohortInputError,
    LATENCY_BUCKETS,
    SCHEMA_VERSION,
    signals_from_archive,
)


Z95 = 1.959963984540054


def _matching(
    signals: Iterable[dict[str, Any]],
    name: str,
    outcome: Optional[str] = None,
) -> list[dict[str, Any]]:
    return [
        signal for signal in signals
        if signal["name"] == name
        and (outcome is None or signal["outcome"] == outcome)
    ]


def _activation(signals: list[dict[str, Any]]) -> Optional[datetime]:
    launches = _matching(signals, "appLaunch")
    if not launches:
        return None
    first_launch = min(signal["time"] for signal in launches)
    deadline = first_launch + timedelta(hours=24)
    subscriptions = [
        signal["time"]
        for signal in _matching(signals, "firstSubscription", "created")
        if first_launch <= signal["time"] <= deadline
    ]
    plays = [
        signal["time"]
        for signal in _matching(signals, "playStarted", "succeeded")
        if first_launch <= signal["time"] <= deadline
    ]
    return max(min(subscriptions), min(plays)) if subscriptions and plays else None


def _wilson(
    numerator: int,
    denominator: int,
) -> tuple[Optional[float], Optional[float]]:
    if denominator == 0:
        return None, None
    proportion = numerator / denominator
    scale = 1 + Z95 * Z95 / denominator
    center = (proportion + Z95 * Z95 / (2 * denominator)) / scale
    margin = Z95 * math.sqrt(
        (proportion * (1 - proportion) + Z95 * Z95 / (4 * denominator))
        / denominator
    ) / scale
    return max(0.0, center - margin), min(1.0, center + margin)


def _measure(
    key: str,
    numerator: int,
    denominator: int,
    minimum: int,
    point_minimum: float,
    lower_minimum: Optional[float],
    evidence_sample: Optional[int] = None,
) -> dict[str, Any]:
    if numerator > denominator:
        raise CohortInputError(f"{key} numerator exceeds its denominator")
    sample = denominator if evidence_sample is None else evidence_sample
    lower, upper = _wilson(numerator, denominator)
    estimate = numerator / denominator if denominator else None
    if sample < minimum or denominator == 0:
        status = "insufficient_evidence"
    else:
        point_passes = estimate is not None and estimate >= point_minimum
        lower_passes = (
            lower_minimum is None
            or (lower is not None and lower >= lower_minimum)
        )
        status = "passes" if point_passes and lower_passes else "misses_threshold"
    return {
        "key": key,
        "numerator": numerator,
        "denominator": denominator,
        "pointEstimate": None if estimate is None else round(estimate, 6),
        "wilson95": {
            "lower": None if lower is None else round(lower, 6),
            "upper": None if upper is None else round(upper, 6),
        },
        "minimumEvidence": minimum,
        "evidenceSampleSize": sample,
        "threshold": {
            "pointMinimum": point_minimum,
            "wilsonLowerMinimum": lower_minimum,
        },
        "status": status,
    }


def _activated_installs(
    installs: list[list[dict[str, Any]]],
) -> tuple[list[list[dict[str, Any]]], dict[int, datetime]]:
    activations = {
        id(signals): activated_at
        for signals in installs
        if (activated_at := _activation(signals)) is not None
    }
    activated = [signals for signals in installs if id(signals) in activations]
    return activated, activations


def _install_counts(
    activated: list[list[dict[str, Any]]],
    activations: dict[int, datetime],
) -> tuple[int, int, int, int]:
    repeat = 0
    agent_repeat = 0
    meaningful = 0
    retained = 0
    for signals in activated:
        activated_at = activations[id(signals)]
        first_day = activated_at.date()
        last_day = first_day + timedelta(days=7)
        launch_days = {
            signal["time"].date()
            for signal in _matching(signals, "appLaunch")
            if first_day <= signal["time"].date() <= last_day
        }
        repeat += len(launch_days) >= 2
        agent_days = {
            signal["time"].date()
            for signal in _matching(signals, "agentTurnCompleted", "succeeded")
        }
        agent_repeat += len(agent_days) >= 2
        meaningful += bool(
            _matching(signals, "meaningfulListening", "succeeded")
        )
        retained += bool(
            _matching(signals, "noteCreated", "created")
            or _matching(signals, "clipCreated", "created")
        )
    return repeat, agent_repeat, meaningful, retained


def _measures(installs: list[list[dict[str, Any]]]) -> list[dict[str, Any]]:
    activated, activations = _activated_installs(installs)
    repeat, agent_repeat, meaningful, retained = _install_counts(
        activated,
        activations,
    )
    signals = [signal for install in installs for signal in install]

    def count(name: str, outcome: Optional[str] = None) -> int:
        return len(_matching(signals, name, outcome))

    transcript_applicable = [
        install
        for install in installs
        if _matching(install, "transcriptReady", "ready")
        and _matching(install, "meaningfulListening", "succeeded")
    ]
    transcript_used = sum(
        bool(_matching(install, "transcriptUsed", "used"))
        for install in transcript_applicable
    )
    recall_asked = count("recallAsked", "started")
    grounded = count("recallGrounded", "grounded")
    completed_recall = [
        signal
        for signal in signals
        if signal["name"] == "recallGrounded"
        and signal["outcome"] in {"grounded", "noEvidence"}
    ]
    fast_recall = sum(
        signal["latency"] in LATENCY_BUCKETS - {"seconds5Plus"}
        for signal in completed_recall
    )
    launches = count("appLaunch")
    unclean = min(count("uncleanTermination", "detected"), launches)
    return [
        _measure("activation", len(activated), len(installs), 50, 0.50, 0.35),
        _measure("repeat_use", repeat, len(activated), 50, 0.40, 0.25),
        _measure("meaningful_listening", meaningful, len(activated), 50, 0.60, 0.45),
        _measure("play_reliability", count("playStarted", "succeeded"), count("playStarted"), 300, 0.985, 0.97),
        _measure("resume_reliability", count("resumeAttempt", "succeeded"), count("resumeAttempt"), 100, 0.97, 0.93),
        _measure("transcript_utility", transcript_used, len(transcript_applicable), 50, 0.40, 0.25),
        _measure("grounded_recall", grounded, recall_asked, 50, 0.70, 0.55),
        _measure("citation_use", count("recallCitationOpened", "opened"), grounded, 50, 0.30, 0.15, recall_asked),
        _measure("retained_artifact", retained, len(activated), 50, 0.20, 0.10),
        _measure("agent_repeat_use", agent_repeat, len(activated), 50, 0.20, 0.10),
        _measure("recall_latency", fast_recall, len(completed_recall), 50, 0.95, None, recall_asked),
        _measure("session_integrity", launches - unclean, launches, 500, 0.99, None),
    ]


def analyze(
    archives: Iterable[tuple[str, Any]],
    generated_at: Optional[datetime] = None,
) -> dict[str, Any]:
    """Return a content-free cohort report from named export archives."""
    archive_list = list(archives)
    unique_signals: dict[str, dict[str, Any]] = {}
    for label, archive in archive_list:
        for signal in signals_from_archive(archive, label):
            previous = unique_signals.get(signal["id"])
            if previous is not None and previous != signal:
                raise CohortInputError(
                    f"signal {signal['id']} conflicts across exports"
                )
            unique_signals[signal["id"]] = signal

    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for signal in unique_signals.values():
        grouped[signal["install"]].append(signal)
    installs = [
        signals for signals in grouped.values()
        if _matching(signals, "appLaunch")
    ]
    measures = _measures(installs)
    data_loss = len(
        _matching(
            unique_signals.values(),
            "dataLossEvidence",
            "detected",
        )
    )
    statuses = {measure["status"] for measure in measures}
    if data_loss:
        product_status = "stop_data_loss_detected"
    elif "misses_threshold" in statuses:
        product_status = "stop_thresholds_not_met"
    elif "insufficient_evidence" in statuses:
        product_status = "insufficient_evidence"
    else:
        product_status = "thresholds_met"
    now = (generated_at or datetime.now(timezone.utc)).astimezone(timezone.utc)
    return {
        "schemaVersion": SCHEMA_VERSION,
        "generatedAt": now.isoformat().replace("+00:00", "Z"),
        "sourceArchiveCount": len(archive_list),
        "uniqueSignalCount": len(unique_signals),
        "evaluatedInstallCount": len(installs),
        "ignoredInstallCount": len(grouped) - len(installs),
        "measures": measures,
        "dataSafety": {
            "dataLossEvidenceDetected": data_loss,
            "status": (
                "stop"
                if data_loss
                else "requires_migration_and_restart_evidence"
            ),
        },
        "productEvidenceStatus": product_status,
        "androidDecision": "not_computed_external_gates_required",
    }
