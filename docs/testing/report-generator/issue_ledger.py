from __future__ import annotations

from typing import Any


BASE_VALIDATION_ISSUES = [
    {
        "id": "GH-702",
        "url": "https://github.com/pablof7z/podcast-player/issues/702",
        "severity": "major",
        "title": "Populate gh-pages report with screenshot-level UX critique",
        "affected_dimensions": ["artifacts", "ui_polish", "ux_polish", "liquid_glass_ios_primitives"],
        "status": "open",
        "owner": "review-agent",
    },
    {
        "id": "GH-730",
        "url": "https://github.com/pablof7z/podcast-player/issues/730",
        "severity": "major",
        "title": "Add accessibility audit evidence for every scenario cluster",
        "affected_dimensions": ["accessibility_dynamic_type", "touch_ergonomics", "device_os_matrix"],
        "status": "open",
        "owner": "validation-agent",
    },
    {
        "id": "GH-733",
        "url": "https://github.com/pablof7z/podcast-player/issues/733",
        "severity": "major",
        "title": "Automate xcresult evidence ingestion into scenario report overlays",
        "affected_dimensions": ["evidence_provenance", "revalidation_status", "observability"],
        "status": "open",
        "owner": "validation-agent",
    },
]

PERFORMANCE_ISSUE = {
    "id": "GH-700",
    "url": "https://github.com/pablof7z/podcast-player/issues/700",
    "severity": "major",
    "title": "Publish per-scenario performance metrics in the Pod0 validation report",
    "affected_dimensions": ["performance", "reliability_flakiness", "readiness_gates"],
    "status": "open",
    "owner": "performance-agent",
}

CASSETTE_ISSUE = {
    "id": "GH-731",
    "url": "https://github.com/pablof7z/podcast-player/issues/731",
    "severity": "major",
    "title": "Normalize provider cassette IDs and fill cassette evidence gaps",
    "affected_dimensions": ["replayability_cassette_provenance", "privacy_security", "observability"],
    "status": "open",
    "owner": "validation-agent",
}

NMP_SYNC_ISSUE = {
    "id": "GH-704",
    "url": "https://github.com/pablof7z/podcast-player/issues/704",
    "severity": "major",
    "title": "Audit Pod0 against latest NMP and chirp-shipped fixes",
    "affected_dimensions": ["sister_app_nmp_chirp_comparison", "nmp_architecture", "regression_risk"],
    "status": "open",
    "owner": "architecture-agent",
}

PUBLISH_LIFECYCLE_ISSUE = {
    "id": "GH-707",
    "url": "https://github.com/pablof7z/podcast-player/issues/707",
    "severity": "blocker",
    "title": "Re-pin Pod0 past NMP publish/action-lifecycle fixes and expose terminal publish state",
    "affected_dimensions": ["nmp_architecture", "actual_result", "error_recovery"],
    "status": "open",
    "owner": "architecture-agent",
}

NIP05_ISSUE = {
    "id": "GH-708",
    "url": "https://github.com/pablof7z/podcast-player/issues/708",
    "severity": "blocker",
    "title": "Adopt NMP pollable NIP-05 lookup state for Add Show/Add Friend",
    "affected_dimensions": ["nmp_architecture", "navigation_state_restoration", "error_recovery"],
    "status": "open",
    "owner": "architecture-agent",
}

CODEGEN_ISSUE = {
    "id": "GH-709",
    "url": "https://github.com/pablof7z/podcast-player/issues/709",
    "severity": "major",
    "title": "Make app action/projection registry drift checks runnable in Pod0 CI",
    "affected_dimensions": ["nmp_architecture", "observability", "regression_risk"],
    "status": "open",
    "owner": "architecture-agent",
}

D8_POLLING_ISSUE = {
    "id": "GH-734",
    "url": "https://github.com/pablof7z/podcast-player/issues/734",
    "severity": "blocker",
    "title": "Remove Task.sleep polling paths from haptics, press feedback, and CarPlay startup",
    "affected_dimensions": ["performance", "nmp_architecture", "motion_haptics", "reliability_flakiness"],
    "status": "open",
    "owner": "architecture-agent",
}


def issues_for_scenario(scenario: Any) -> list[dict[str, Any]]:
    issues = [*BASE_VALIDATION_ISSUES]
    tags = set(getattr(scenario, "tags", []))
    text = " ".join(
        [
            getattr(scenario, "scenario_id", ""),
            getattr(scenario, "title", ""),
            getattr(scenario, "category", ""),
            getattr(scenario, "source_file", ""),
            getattr(scenario, "evidence", {}).get("deps", ""),
        ]
    ).lower()
    if getattr(scenario, "performance_required", False):
        issues.append(PERFORMANCE_ISSUE)
    if getattr(scenario, "cassettes", []) or getattr(scenario, "provider_mode", "") == "blocked":
        issues.append(CASSETTE_ISSUE)
    if "chirp" in text or "nmp" in text:
        issues.append(NMP_SYNC_ISSUE)
    if any(token in text for token in ["publish", "outbox", "relay ack", "nmpm-005", "queued publish"]):
        issues.append(PUBLISH_LIFECYCLE_ISSUE)
    if "nip05" in getattr(scenario, "slug", "") or "nip-05" in text:
        issues.append(NIP05_ISSUE)
    if "codegen" in text or "nmpm-008" in text:
        issues.append(CODEGEN_ISSUE)
    if "d8" in tags or "d8" in text or getattr(scenario, "scenario_id", "").startswith("D8-"):
        issues.append(D8_POLLING_ISSUE)
    return dedupe_issues(issues)


def issue_refs_for(issues: list[dict[str, Any]]) -> list[str]:
    return [issue["id"] for issue in issues]


def generated_validation_issue(issue: dict[str, Any]) -> bool:
    return isinstance(issue.get("id"), str) and issue["id"].startswith("GH-")


def merge_issue_lists(current: list[dict[str, Any]], previous: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return dedupe_issues([*current, *previous])


def dedupe_issues(issues: list[dict[str, Any]]) -> list[dict[str, Any]]:
    merged: dict[str, dict[str, Any]] = {}
    for issue in issues:
        merged.setdefault(issue["id"], issue)
    return list(merged.values())
