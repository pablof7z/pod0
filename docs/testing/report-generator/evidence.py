from __future__ import annotations

import json
import shutil
from pathlib import Path
from typing import Any

from structures import evidence_placeholders_for


def load_evidence_overlays(evidence_root: Path) -> dict[str, dict[str, Any]]:
    records_dir = evidence_root / "scenario-records"
    if not records_dir.exists():
        return {}
    overlays: dict[str, dict[str, Any]] = {}
    for path in sorted(records_dir.glob("*.json")):
        data = json.loads(path.read_text())
        scenario_id = data.get("scenario_id")
        if not scenario_id:
            raise ValueError(f"{path} is missing scenario_id")
        if scenario_id in overlays:
            raise ValueError(f"Duplicate evidence overlay for {scenario_id}")
        for artifact in data.get("artifacts", []):
            enrich_screenshot_dimensions(artifact, evidence_root)
        overlays[scenario_id] = data
    return overlays


def apply_evidence_overlays(records: list[dict[str, Any]], overlays: dict[str, dict[str, Any]]) -> None:
    by_id = {record["scenario"]["id"]: record for record in records}
    unknown = sorted(set(overlays) - set(by_id))
    if unknown:
        raise ValueError(f"Evidence overlays reference unknown scenarios: {', '.join(unknown)}")
    for scenario_id, overlay in overlays.items():
        record = by_id[scenario_id]
        for artifact in overlay.get("artifacts", []):
            record["evidence"]["artifacts"].append(artifact)
        deep_merge(record, overlay.get("merge", {}))
        record["evidence"]["placeholders"] = evidence_placeholders_for(record["evidence"].get("missing", []))
        normalize_observed_record(record)


def normalize_observed_record(record: dict[str, Any]) -> None:
    if record["execution"]["status"] == "not_run":
        return
    normalize_coherence(record)
    normalize_readiness(record)
    normalize_launch_assessment(record)
    normalize_observed_sections(record)


def normalize_coherence(record: dict[str, Any]) -> None:
    scenario = record["scenario"]
    status = record["execution"]["status"]
    evidence_refs = observed_evidence_refs(record)
    if status == "fail":
        individual_status = "fail"
        individual_summary = (
            f"{scenario['id']} has current run evidence and currently fails its "
            "scenario-level product job; linked defects and revalidation remain required."
        )
    elif status == "pass_with_issues":
        individual_status = "pass_with_issues"
        individual_summary = (
            f"{scenario['id']} has current run evidence for the exercised product path, "
            "with remaining gaps tracked in missing evidence and follow-up."
        )
    else:
        individual_status = "incomplete"
        individual_summary = (
            f"{scenario['id']} has partial current run evidence, but required branches "
            "remain unvalidated."
        )

    record["coherence"]["individual_judgment"] = {
        "status": individual_status,
        "summary": individual_summary,
        "checks": [
            "observed scenario-level UI polish",
            "observed scenario-level UX polish",
            "observed performance/accessibility evidence",
            "scenario-level product fit",
        ],
        "evidence_refs": evidence_refs,
        "gaps": missing_evidence_labels(record),
    }
    record["coherence"]["group_judgment"] = {
        "status": "incomplete",
        "summary": (
            f"{record['coherence']['cluster']['title']} cluster coherence remains incomplete "
            f"until adjacent scenarios are judged together; {scenario['id']} now contributes "
            "current evidence and linked defects to that group view."
        ),
        "checks": [
            "related scenario consistency",
            "shared defect themes",
            "cluster readiness",
            "cross-flow state continuity",
        ],
        "evidence_refs": sorted(set(evidence_refs + ["rubric:review-skill-grounding"])),
        "gaps": ["Review adjacent scenario pages as a cluster before promoting group coherence."],
    }


def normalize_readiness(record: dict[str, Any]) -> None:
    status = record["execution"]["status"]
    missing = missing_evidence_labels(record)
    issue_ids = [issue["id"] for issue in record["issues"]]
    evidence_refs = observed_evidence_refs(record)
    record["readiness"]["ship_gate"] = "blocked" if status == "fail" else "incomplete"
    blockers = []
    if status == "fail":
        blockers.append("Current run evidence contains a functional failure that must be fixed and revalidated.")
    if issue_ids:
        blockers.append(f"Linked open defect(s): {', '.join(issue_ids)}.")
    if missing:
        blockers.append(f"Remaining missing evidence: {', '.join(missing)}.")
    blockers.append("Related scenario cluster coherence remains incomplete until adjacent pages are reviewed together.")
    record["readiness"]["blocking_reasons"] = blockers

    for gate in record["readiness"]["gates"]:
        gate_id = gate["id"]
        if gate_id == "required_evidence":
            gate["status"] = "pass_with_issues" if missing else "pass"
            gate["evidence_refs"] = evidence_refs
        elif gate_id == "skill_grounding":
            gate["status"] = "pass"
            gate["evidence_refs"] = ["rubric:review-skill-grounding"]
        elif gate_id == "score_integrity":
            gate["status"] = "pass_with_issues"
            gate["evidence_refs"] = evidence_refs
        elif gate_id == "product_coherence":
            gate["status"] = "incomplete"
            gate["evidence_refs"] = sorted(set(evidence_refs + ["rubric:review-skill-grounding"]))
        elif gate_id == "defect_tracking":
            gate["status"] = "pass_with_issues" if issue_ids else "incomplete"
            gate["evidence_refs"] = evidence_refs
        elif gate_id == "release_readiness":
            gate["status"] = "blocked" if status == "fail" or issue_ids else "incomplete"
            gate["evidence_refs"] = evidence_refs


def normalize_launch_assessment(record: dict[str, Any]) -> None:
    missing = missing_evidence_labels(record)
    open_issues = [issue for issue in record["issues"] if issue["status"] == "open"]
    risk_source = open_issues or record["risks"]
    blocked_gates = [
        gate["id"]
        for gate in record["readiness"]["gates"]
        if gate["status"] in {"blocked", "incomplete"}
    ]
    record["launch_assessment"] = {
        "launch_readiness": record["readiness"]["ship_gate"],
        "risk_classification": highest_severity(risk_source),
        "evidence_quality": "pass_with_issues" if missing else "pass",
        "accessibility_status": record["dimension_scores"]["accessibility_dynamic_type"]["status"],
        "regression_coverage": record["dimension_scores"]["regression_risk"]["status"],
        "dependency_posture": record["run"]["provider_mode"],
        "scenario_owner": "validation-agent",
        "scenario_status": record["execution"]["status"],
        "product_judgment": record["verdict"]["summary"],
        "individual_judgment": record["coherence"]["individual_judgment"]["summary"],
        "whole_product_judgment": record["coherence"]["group_judgment"]["summary"],
        "blocking_gates": blocked_gates,
        "issue_refs": [issue["id"] for issue in record["issues"]],
        "missing_evidence": missing,
    }


def highest_severity(items: list[dict[str, Any]]) -> str:
    order = {"blocker": 4, "major": 3, "minor": 2, "polish": 1}
    return max((item.get("severity", "none") for item in items), key=lambda value: order.get(value, 0), default="none")


def normalize_observed_sections(record: dict[str, Any]) -> None:
    refs = observed_evidence_refs(record)
    artifact_count = len([artifact for artifact in record["evidence"]["artifacts"] if not artifact["id"].startswith("catalog:")])
    fixed_issues = [issue for issue in record["issues"] if issue["status"] == "fixed"]
    open_issues = [issue for issue in record["issues"] if issue["status"] == "open"]
    record["sections"]["product_coherence_in_context"] = section(
        record["coherence"]["individual_judgment"]["summary"],
        refs,
        record["coherence"]["individual_judgment"]["gaps"],
    )
    record["sections"]["product_cluster_coherence"] = section(
        record["coherence"]["group_judgment"]["summary"],
        record["coherence"]["group_judgment"]["evidence_refs"],
        record["coherence"]["group_judgment"]["gaps"],
    )
    record["sections"]["readiness_gates"] = section(
        f"Ship gate is {record['readiness']['ship_gate']}; gates now reflect attached evidence, remaining missing evidence, linked defects, and cluster-level follow-up.",
        refs,
        record["readiness"]["blocking_reasons"],
    )
    record["sections"]["evidence_provenance"] = section(
        f"{artifact_count} non-catalog artifact(s) are attached with their published IDs, paths, redaction metadata, and run context.",
        refs,
        missing_evidence_labels(record),
    )
    record["sections"]["before_after_deltas"] = section(
        "Current evidence must be read as before/action/after deltas across the flow steps; any missing pair remains listed in instrumentation gaps.",
        refs,
        missing_evidence_labels(record),
    )
    record["sections"]["revalidation_status"] = section(
        f"{len(fixed_issues)} fixed issue(s) and {len(open_issues)} open issue(s) are linked; fixed issues require revalidation run IDs before closure is considered proven.",
        refs,
        [issue["id"] for issue in record["issues"] if not issue.get("revalidation_run_id")],
    )
    record["sections"]["owner_status"] = section(
        "Readiness gates, instrumentation gaps, risks, issues, and next actions carry owner/status fields for follow-through.",
        refs,
        record["readiness"]["blocking_reasons"],
    )


def observed_evidence_refs(record: dict[str, Any]) -> list[str]:
    refs = []
    for artifact in record["evidence"]["artifacts"]:
        artifact_id = artifact["id"]
        if artifact_id.startswith("catalog:"):
            continue
        refs.append(artifact_id)
    return sorted(set(refs))


def missing_evidence_labels(record: dict[str, Any]) -> list[str]:
    return [item["kind"] for item in record["evidence"]["missing"]]


def section(summary: str, refs: list[str], notes: list[str] | None = None) -> dict[str, Any]:
    return {"summary": summary, "evidence_refs": sorted(set(refs)), "notes": notes or []}


def copy_evidence_assets(evidence_root: Path, out: Path) -> None:
    source = evidence_root / "assets"
    if not source.exists():
        return
    target = out / "assets"
    if target.exists():
        shutil.rmtree(target)
    shutil.copytree(source, target)


def enrich_screenshot_dimensions(artifact: dict[str, Any], evidence_root: Path) -> None:
    if artifact.get("type") != "screenshot" or {"width", "height"}.issubset(artifact):
        return
    path = evidence_root / artifact.get("path", "")
    if not path.exists():
        return
    size = image_dimensions(path)
    if size is not None:
        artifact["width"], artifact["height"] = size


def image_dimensions(path: Path) -> tuple[int, int] | None:
    data = path.read_bytes()
    if data.startswith(b"\x89PNG\r\n\x1a\n") and len(data) >= 24:
        return int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")
    if not data.startswith(b"\xff\xd8"):
        return None
    index = 2
    while index + 9 < len(data):
        if data[index] != 0xFF:
            index += 1
            continue
        marker = data[index + 1]
        index += 2
        if marker in {0xD8, 0xD9}:
            continue
        if index + 2 > len(data):
            return None
        length = int.from_bytes(data[index : index + 2], "big")
        if length < 2 or index + length > len(data):
            return None
        if marker in {0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF}:
            height = int.from_bytes(data[index + 3 : index + 5], "big")
            width = int.from_bytes(data[index + 5 : index + 7], "big")
            return width, height
        index += length
    return None


def deep_merge(target: dict[str, Any], patch: dict[str, Any]) -> None:
    for key, value in patch.items():
        if isinstance(value, dict) and isinstance(target.get(key), dict):
            deep_merge(target[key], value)
        else:
            target[key] = value
