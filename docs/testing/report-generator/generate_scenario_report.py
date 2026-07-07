#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import shutil
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

from catalog import parse_catalog, slugify  # noqa: E402
from contract import GENERATOR_VERSION, SCHEMA_VERSION, SITE_BASE, SKILL_GROUNDING  # noqa: E402
from evidence import apply_evidence_overlays, copy_evidence_assets, load_evidence_overlays  # noqa: E402
from issue_ledger import merge_issue_lists  # noqa: E402
from provider_cassettes_report import provider_cassette_data, render_provider_cassette_page  # noqa: E402
from records import build_report, has_observed_data, rollups_for, summary_for, tags_for_records, validate_output, validate_schema_contract  # noqa: E402
from render import render_home, render_scenario_index, render_scenario_page, write_rollup_pages, write_tag_pages  # noqa: E402
from styles import stylesheet  # noqa: E402

PRESERVED_RECORD_FIELDS = [
    "run",
    "execution",
    "launch_assessment",
    "verdict",
    "sections",
    "dimension_scores",
    "group_scores",
    "quality_review",
    "coherence",
    "readiness",
    "evidence",
    "metrics",
    "instrumentation_gaps",
    "risks",
    "issues",
    "next_actions",
]
PRESERVED_OUTPUT_NAMES = {".git", ".gitignore", ".nojekyll", "CNAME", "assets"}


def write_site(records: list[dict[str, Any]], out: Path, catalog: Path, repo: Path | None = None, evidence: Path | None = None) -> None:
    previous_records = read_previous_records(out)
    records = [merge_previous_record(record, previous_records.get(record["scenario"]["slug"])) for record in records]
    issues = issues_for_records(records)
    if not issues["issues"]:
        issues = read_json(out / "data" / "issues.json", {"issues": [], "counts": {"open": 0, "fixed": 0}})
    clean_output(out)
    copy_sources(catalog, out)
    if evidence is not None:
        copy_evidence_assets(evidence, out)
    if repo is not None:
        copy_repo_assets(repo, out)
    write_text(out / "styles.css", stylesheet())
    write_json(out / "data" / "skill-grounding.json", {"generated_by": GENERATOR_VERSION, "skills": SKILL_GROUNDING})
    cassette_data = provider_cassette_data(repo, evidence or repo / "docs" / "testing" / "evidence", catalog)
    write_json(out / "data" / "provider-cassettes.json", cassette_data)
    write_data_files(records, out, issues)
    write_text(out / "index.html", render_home(records, 0))
    write_text(out / "provider-cassettes" / "index.html", render_provider_cassette_page(cassette_data, 1))
    write_text(out / "scenarios" / "index.html", render_scenario_index(records, 1, "All Scenarios"))
    for slug, grouped in group_by_category(records).items():
        write_text(out / "scenarios" / slug / "index.html", render_scenario_index(grouped, 2, grouped[0]["scenario"]["category"]))
    for record in records:
        scenario_dir = out / "scenarios" / record["scenario"]["slug"]
        write_json(scenario_dir / "data.json", record)
        write_text(scenario_dir / "index.html", render_scenario_page(record, 2))
    for path, content in {**write_tag_pages(records, out), **write_rollup_pages(records, out)}.items():
        write_text(path, content)
    validate_output(records, out)


def clean_output(out: Path) -> None:
    resolved = out.resolve()
    if resolved in {Path("/").resolve(), Path.home().resolve(), Path.cwd().resolve()}:
        raise ValueError(f"Refusing to clear dangerous output path: {resolved}")
    if out.exists():
        for child in out.iterdir():
            if child.name in PRESERVED_OUTPUT_NAMES:
                continue
            if child.is_dir():
                shutil.rmtree(child)
            else:
                child.unlink()
    else:
        out.mkdir(parents=True)


def copy_sources(catalog: Path, out: Path) -> None:
    target = out / "sources" / "catalog"
    target.mkdir(parents=True)
    for path in catalog.glob("*.md"):
        shutil.copy2(path, target / path.name)


def copy_repo_assets(repo: Path, out: Path) -> None:
    product = repo / "docs" / "images"
    if product.exists():
        copy_tree_contents(product, out / "assets" / "product")
    scenarios = repo / "docs" / "testing" / "scenarios"
    for image in scenarios.glob("*.jpg"):
        copy_file(image, out / "assets" / "scenario-screenshots" / image.name)
    screenshots = scenarios / "screenshots"
    if screenshots.exists():
        copy_tree_contents(screenshots, out / "assets" / "scenario-screenshots")


def copy_tree_contents(source: Path, target: Path) -> None:
    for path in source.rglob("*"):
        if path.is_file():
            copy_file(path, target / path.relative_to(source))


def copy_file(source: Path, target: Path) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, target)


def write_data_files(records: list[dict[str, Any]], out: Path, issues: dict[str, Any]) -> None:
    write_json(out / "data" / "scenarios.json", [summary_for(record) for record in records])
    write_json(out / "data" / "rollups.json", rollups_for(records))
    write_json(out / "data" / "tags.json", tags_for_records(records))
    write_json(out / "data" / "issues.json", issues)
    write_json(out / "data" / "schema-version.json", {"schema_version": SCHEMA_VERSION, "generator_version": GENERATOR_VERSION})


def issues_for_records(records: list[dict[str, Any]]) -> dict[str, Any]:
    issues: list[dict[str, Any]] = []
    counts: dict[str, int] = {}
    for record in records:
        for issue in record["issues"]:
            item = {**issue, "scenario_id": record["scenario"]["id"], "scenario_slug": record["scenario"]["slug"]}
            issues.append(item)
            counts[item["status"]] = counts.get(item["status"], 0) + 1
    return {"issues": issues, "counts": dict(sorted(counts.items()))}


def read_previous_records(out: Path) -> dict[str, dict[str, Any]]:
    previous: dict[str, dict[str, Any]] = {}
    for path in (out / "scenarios").glob("*/data.json"):
        record = read_json(path, {})
        slug = record.get("scenario", {}).get("slug")
        if isinstance(slug, str):
            previous[slug] = record
    return previous


def merge_previous_record(current: dict[str, Any], previous: dict[str, Any] | None) -> dict[str, Any]:
    if not previous or set(previous) != set(current) or previous.get("schema_version") != current["schema_version"]:
        return current
    if not has_observed_data(previous):
        return current
    merged = dict(current)
    for field in PRESERVED_RECORD_FIELDS:
        merged[field] = previous[field]
    merged["sections"] = merge_keyed_dict(current["sections"], previous["sections"])
    merged["sections"]["review_skill_grounding"] = current["sections"]["review_skill_grounding"]
    merged["dimension_scores"] = merge_keyed_dict(current["dimension_scores"], previous["dimension_scores"])
    merged["group_scores"] = merge_group_scores(current["group_scores"], previous["group_scores"])
    merged["evidence"] = merge_evidence(current["evidence"], previous["evidence"])
    merged["issues"] = merge_issue_lists(current["issues"], previous["issues"])
    merged["review_grounding"] = current["review_grounding"]
    merged["next_actions"] = merge_actions(current["next_actions"], previous["next_actions"])
    return merged


def merge_keyed_dict(current: dict[str, Any], previous: dict[str, Any]) -> dict[str, Any]:
    return {key: previous.get(key, value) for key, value in current.items()}


def merge_group_scores(current: dict[str, Any], previous: dict[str, Any]) -> dict[str, Any]:
    merged: dict[str, Any] = {}
    for group, value in current.items():
        previous_value = previous.get(group)
        if isinstance(previous_value, dict):
            item = {**value, **previous_value}
            item["dimension_refs"] = value["dimension_refs"]
            merged[group] = item
        else:
            merged[group] = value
    return merged


def merge_evidence(current: dict[str, Any], previous: dict[str, Any]) -> dict[str, Any]:
    merged = dict(previous)
    artifacts = {item["id"]: item for item in current.get("artifacts", [])}
    for artifact in previous.get("artifacts", []):
        artifacts.setdefault(artifact["id"], artifact)
    merged["artifacts"] = list(artifacts.values())
    return merged


def merge_actions(current: list[dict[str, Any]], previous: list[dict[str, Any]]) -> list[dict[str, Any]]:
    previous_by_id = {item["id"]: item for item in previous if isinstance(item.get("id"), str)}
    merged = []
    seen = set()
    for item in current:
        action_id = item["id"]
        merged.append({**item, **previous_by_id.get(action_id, {})})
        seen.add(action_id)
    merged.extend(item for item in previous if item.get("id") not in seen)
    return merged


def group_by_category(records: list[dict[str, Any]]) -> dict[str, list[dict[str, Any]]]:
    grouped: dict[str, list[dict[str, Any]]] = {}
    for record in records:
        grouped.setdefault(slugify(record["scenario"]["category"]), []).append(record)
    return grouped


def write_json(path: Path, data: Any) -> None:
    write_text(path, json.dumps(data, indent=2, sort_keys=True) + "\n")


def read_json(path: Path, fallback: dict[str, Any]) -> dict[str, Any]:
    if not path.exists():
        return fallback
    try:
        return json.loads(path.read_text())
    except json.JSONDecodeError:
        return fallback


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Generate static Pod0 scenario validation report pages from the BDD catalog.")
    parser.add_argument("--catalog", default="docs/testing/scenarios/catalog", type=Path)
    parser.add_argument("--out", default="build/pod0-scenario-report", type=Path)
    parser.add_argument("--schema", default="docs/testing/scenario-report.schema.json", type=Path)
    parser.add_argument("--evidence", default="docs/testing/evidence", type=Path)
    parser.add_argument("--site-base", default=SITE_BASE)
    args = parser.parse_args(argv)
    generated_at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    scenarios = parse_catalog(args.catalog)
    records = [build_report(scenario, scenarios, Path.cwd(), generated_at, args.site_base) for scenario in scenarios]
    apply_evidence_overlays(records, load_evidence_overlays(args.evidence))
    validate_schema_contract(records, args.schema)
    write_site(records, args.out, args.catalog, Path.cwd(), args.evidence)
    digest = hashlib.sha256(json.dumps([summary_for(record) for record in records], sort_keys=True).encode()).hexdigest()[:12]
    print(f"Generated {len(records)} scenario pages at {args.out} ({digest})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
