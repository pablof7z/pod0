from __future__ import annotations

from typing import Any


REPORT_VIEWPORT_MISSING = {
    "kind": "report_viewport",
    "reason": "Generated scenario pages need desktop/mobile viewport smoke evidence so report-page layout is reviewable.",
    "blocks_dimensions": ["device_viewport_coverage", "evidence_confidence"],
    "required": True,
}


def ensure_report_viewport_requirement(record: dict[str, Any]) -> None:
    evidence = record["evidence"]
    required = evidence.setdefault("required_kinds", [])
    if "report_viewport" not in required:
        required.append("report_viewport")
        required.sort()
    has_artifact = any(artifact.get("type") == "report_viewport" for artifact in evidence.get("artifacts", []))
    missing = evidence.setdefault("missing", [])
    has_missing = any(item.get("kind") == "report_viewport" for item in missing)
    if not has_artifact and not has_missing:
        missing.append(dict(REPORT_VIEWPORT_MISSING))
