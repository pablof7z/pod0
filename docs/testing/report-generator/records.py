from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path
from typing import Any

from catalog import Scenario
from cassettes import cassette_artifacts, cassettes_for_scenario, effective_provider_mode, run_cassette_records
from contract import (
    GENERATOR_VERSION,
    GROUPS,
    SCHEMA_VERSION,
    SECTION_LABELS,
    SECTION_TO_DIMENSION,
    SKILL_GROUNDING,
)
from issue_ledger import generated_validation_issue, issues_for_scenario
from structures import (
    coherence_for,
    evidence_inventory_for,
    execution_for,
    flow_steps_for,
    instrumentation_gaps_for,
    launch_assessment_for,
    product_context_for,
    quality_review_for,
    readiness_for,
    review_grounding_for,
    risks_for,
)


def run_git(args: list[str], cwd: Path, fallback: str) -> str:
    try:
        return subprocess.check_output(["git", *args], cwd=cwd, text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        return fallback


def build_report(scenario: Scenario, scenarios: list[Scenario], repo: Path, generated_at: str, site_base: str) -> dict[str, Any]:
    commit = run_git(["rev-parse", "HEAD"], repo, "0" * 40)
    branch = run_git(["branch", "--show-current"], repo, "unknown")
    source_ref = f"catalog:{scenario.scenario_id.lower()}"
    skill_ref = "rubric:review-skill-grounding"
    previous = scenarios[scenario.order - 2] if scenario.order > 1 else None
    next_scenario = scenarios[scenario.order] if scenario.order < len(scenarios) else None
    scenario_cassettes = cassettes_for_scenario(repo, scenario.scenario_id)
    cassette_available = bool(scenario_cassettes)
    provider_mode = effective_provider_mode(scenario.provider_mode, scenario_cassettes)
    sections = section_text(scenario, scenario_cassettes)
    evidence_inventory = evidence_inventory_for(scenario, cassette_available)
    coherence = coherence_for(scenario, scenarios)
    issues = issues_for_scenario(scenario)
    readiness = readiness_for(scenario, issues)
    risks = risks_for(scenario, scenarios, cassette_available)
    dimensions = {
        dimension: {
            "score": 0,
            "status": "incomplete",
            "rationale": f"{SECTION_LABELS[section_id]} has not been validated with current run evidence.",
            "evidence_refs": sections[section_id]["evidence_refs"],
        }
        for section_id, dimension in SECTION_TO_DIMENSION.items()
    }
    return {
        "schema_version": SCHEMA_VERSION,
        "scenario": scenario_identity(scenario, scenarios),
        "run": run_record(scenario, generated_at, commit, branch, scenario_cassettes, provider_mode),
        "page": page_record(scenario, generated_at, previous, next_scenario, site_base),
        "product_context": product_context_for(scenario, scenarios),
        "flow_steps": flow_steps_for(scenario),
        "execution": execution_for(scenario, generated_at),
        "review_grounding": review_grounding_for(),
        "launch_assessment": launch_assessment_for(scenario, evidence_inventory, coherence, readiness, risks, issues, provider_mode),
        "verdict": {
            "overall": "incomplete",
            "summary": "Generated catalog scaffold only. Required execution evidence and critique are missing.",
            "score_gate_explanation": "Every dimension is scored 0, so the mechanical verdict is incomplete.",
        },
        "sections": sections,
        "dimension_scores": dimensions,
        "group_scores": group_scores(),
        "quality_review": quality_review_for(scenario),
        "coherence": coherence,
        "readiness": readiness,
        "evidence": {"artifacts": artifacts_for(scenario, source_ref, skill_ref, scenario_cassettes), **evidence_inventory},
        "metrics": [],
        "instrumentation_gaps": instrumentation_gaps_for(scenario, cassette_available),
        "risks": risks,
        "issues": issues,
        "next_actions": next_actions_for(scenario, cassette_available),
    }


def scenario_identity(scenario: Scenario, scenarios: list[Scenario]) -> dict[str, Any]:
    return {
        "id": scenario.scenario_id,
        "slug": scenario.slug,
        "title": scenario.title,
        "category": scenario.category,
        "source_path": scenario.source_path,
        "bdd": scenario.bdd,
        "tags": scenario.tags,
        "related_scenarios": related_for(scenario, scenarios),
        "dependencies": scenario.dependencies,
    }


def run_record(
    scenario: Scenario,
    generated_at: str,
    commit: str,
    branch: str,
    scenario_cassettes: list[dict[str, Any]],
    provider_mode: str,
) -> dict[str, Any]:
    return {
        "run_id": f"catalog-scaffold-{generated_at.replace(':', '').replace('-', '')}-{scenario.slug}",
        "source_commit": commit,
        "branch": branch,
        "app_build": "not-built-catalog-scaffold",
        "started_at": generated_at,
        "completed_at": generated_at,
        "executor": "static catalog generator",
        "generator_version": GENERATOR_VERSION,
        "provider_mode": provider_mode,
        "device_matrix": [{"name": "not-run", "os_version": "not-run", "form_factor": "other"}],
        "environment": {"locale": "unspecified", "appearance": "unspecified", "network_condition": "not-run", "launch_arguments": []},
        "fixtures": scenario.dependencies,
        "cassettes": run_cassette_records(scenario_cassettes)
        or [{"id": cassette, "provider": provider_from(cassette), "mode": "blocked", "redaction_hash": "pending-redaction-hash"} for cassette in scenario.cassettes],
        "live_dependency_rationale": live_dependency_rationale(scenario, scenario_cassettes),
    }


def page_record(scenario: Scenario, generated_at: str, previous: Scenario | None, next_scenario: Scenario | None, site_base: str) -> dict[str, Any]:
    return {
        "canonical_url": f"{site_base.rstrip('/')}/scenarios/{scenario.slug}/",
        "generated_at": generated_at,
        "assets_base": f"assets/scenarios/{scenario.slug}/",
        "navigation": {
            "scenario_index_url": "../",
            "category_url": f"../{scenario.category_slug}/",
            "previous_url": f"../{previous.slug}/" if previous else "",
            "next_url": f"../{next_scenario.slug}/" if next_scenario else "",
            "rollup_url": "../../data/rollups.json",
        },
    }


def section_text(scenario: Scenario, scenario_cassettes: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    source_ref = f"catalog:{scenario.scenario_id.lower()}"
    skill_ref = "rubric:review-skill-grounding"
    deps = scenario.evidence.get("deps", "none")
    perf = scenario.evidence.get("perf", "none")
    screenshots = scenario.evidence.get("ss", "none")
    boundary = scenario.evidence.get("boundary", "none")
    missing = "No run evidence has been attached yet; this generated page is intentionally incomplete."
    cassette_ids = [cassette["id"] for cassette in scenario_cassettes]
    cassette_refs = [f"cassette:{cassette_id}" for cassette_id in cassette_ids]
    cassette_summary = (
        f"Replay fixture(s) mapped to this scenario: {', '.join(cassette_ids)}."
        if cassette_ids
        else f"Declared dependencies: {deps}. Provider, relay, STT, TTS, LLM, and network behavior must use replay cassettes or explain live-only evidence."
    )
    return {
        "persona_job_acceptance": s(f"Persona: {product_context_for(scenario, [scenario])['persona']} Acceptance criteria are the scenario Given/When/Then plus the declared architecture boundary {boundary}.", [source_ref]),
        "flow": s(f"Catalog flow for {scenario.category}: Given {scenario.bdd['given'][0]}, when {scenario.bdd['when'][0]}, then {scenario.bdd['then'][0]}.", [source_ref]),
        "attempted_test": s(f"{missing} A simulator, manual, or automation run must execute scenario {scenario.scenario_id}.", [source_ref]),
        "scenario_setup": s(f"Declared setup dependencies: {deps}. Provider mode is {effective_provider_mode(scenario.provider_mode, scenario_cassettes)}; {cassette_summary}", [source_ref, *cassette_refs]),
        "execution_attempts": s("No attempts, retries, or branch executions have run. The page reserves structured attempt records for manual, simulator, CI, and replay executions.", [source_ref]),
        "expected_behavior": s(f"Expected behavior from BDD: {scenario.bdd['then'][0]}. NMP/RMP boundary declared by the catalog: {boundary}.", [source_ref]),
        "actual_result": s(missing, [source_ref], ["Attach step-by-step observed behavior before changing this verdict."]),
        "artifacts": s(f"Required visual/raw evidence from the catalog: {screenshots}. Only the source catalog and rubric metadata exist right now.", [source_ref, skill_ref]),
        "evidence_provenance": s("Not assessed. Every artifact must identify its source command/tool, capture context, source commit, branch, device/OS, SHA/path, redaction state, freshness, and whether it is live, replayed, generated, or copied from prior evidence.", [source_ref, skill_ref]),
        "review_skill_grounding": s("Review must be grounded in loaded skills, not generic taste. Selected grounding covers Liquid Glass restraint, HIG factual checks, safe areas, SF typography, Dynamic Type, touch targets, accessibility settings, native navigation, mobile thumb-zone UX, material fallbacks, and measured performance-as-UX review.", [skill_ref], [f"Selected skills: {', '.join(item['name'] for item in SKILL_GROUNDING if item.get('selected'))}."]),
        "ui_polish_report": s("Not assessed. Requires annotated screenshots for layout, spacing, typography, color, symbols, component state, and platform-native finish.", [skill_ref]),
        "ux_polish_report": s("Not assessed. Requires notes on task clarity, user effort, feedback, interruption/resume, recovery, and cognitive load.", [skill_ref]),
        "performance_metrics": s(f"Not measured. Required performance evidence: {perf}.", [source_ref]),
        "accessibility_dynamic_type": s("Not assessed. Requires VoiceOver/UI tree labels, Dynamic Type, contrast, Reduce Motion/Transparency, and touch target evidence.", [skill_ref]),
        "liquid_glass_ios_primitives": s("Not assessed. Requires evidence that iOS primitives and glass-like materials are used as functional navigation/control chrome, not decorative content material, with semantic colors and accessibility fallbacks.", [skill_ref]),
        "error_recovery_behavior": s("Not assessed. Error, offline, retry, cancellation, and recovery paths must be captured where the scenario can fail.", [source_ref]),
        "privacy_security": s("Not assessed. Validation must scan screenshots, logs, cassettes, relays, and exports for leaked keys, tokens, private audio, or private Nostr material.", [source_ref]),
        "nmp_architecture_cohesiveness": s(f"Not assessed. Declared NMP/RMP boundary: {boundary}. Review must check Rust-owned state/policy, thin native renderers, bounded FFI, replay clocks, and privacy fail-closed behavior.", [source_ref, skill_ref]),
        "product_coherence_in_context": s("Not assessed. The page must explain how this flow fits nearby Pod0 surfaces such as library, player, transcript, agent, settings, and Nostr/social state.", [source_ref]),
        "product_cluster_coherence": s("Not assessed. Related scenarios must be judged as a group so UI/UX/product coherence does not pass in isolation while the cluster remains inconsistent.", [source_ref, skill_ref]),
        "before_after_deltas": s("Not assessed. Capture the before state, user action, after state, expected delta, unexpected regression, and screenshot pair for every visible product change.", [source_ref]),
        "reliability_flakiness": s("Not assessed. Validation must record rerun count, retry causes, flake history, stale-build risk, timeout behavior, and deterministic replay coverage.", [source_ref]),
        "regression_risk": s("Not assessed. Validation must list adjacent scenarios and modules to rerun after any fix.", [source_ref]),
        "defects_issues_filed": s("No defects have been observed yet because the scenario has not run. Any actionable defect found later must link a GitHub issue before the page can leave incomplete.", [source_ref]),
        "revalidation_status": s("No fix or revalidation cycle exists yet. Every fixed defect must link the fix PR, revalidation run ID, rerun commit, affected dimensions, and remaining open gaps.", [source_ref]),
        "risks_follow_up": s("Current risks are scaffolded from missing evidence and cluster regression exposure. Actionable defects must gain issue/PR links and revalidation run IDs.", [source_ref]),
        "instrumentation_gaps": s("Screenshots, UI trees, logs, metrics, cassettes, and accessibility audits are listed as explicit missing evidence so gaps cannot be hidden in prose.", [source_ref, skill_ref]),
        "localization_content_quality": s("Not assessed. Review must check locale-sensitive strings, podcast metadata, transcript/agent content, truncation, empty/error copy, and translation safety.", [skill_ref]),
        "controls_gestures_audio": s("Not assessed. Review must check buttons, menus, sheets, gestures, audio route controls, haptics, keyboard alternatives, and one-handed reach.", [skill_ref]),
        "offline_resume_behavior": s("Not assessed. Offline, interruption, relaunch, background playback, cancellation, retry, and resume behavior must be captured where applicable.", [source_ref]),
        "readiness_gates": s("All readiness gates are blocked until evidence, skill-grounded scoring, product-cluster coherence, defect tracking, and release readiness are populated.", [source_ref, skill_ref]),
        "verdict": s("Overall verdict is incomplete because required screenshots, metrics, cassettes, accessibility evidence, critique, and issue back-links are missing.", [source_ref, skill_ref]),
        "next_actions": s("Run the scenario, attach evidence, score every dimension, file defects, and republish this page from validated JSON.", [source_ref]),
        "owner_status": s("Default ownership is validation-agent for evidence and review-agent for scoring/defects until a human or agent owner is assigned. Every blocker, gate, action, defect, and revalidation item must carry owner and status.", [source_ref]),
        "data_integrity_state_sync": s("Not assessed. Verify persisted state, Rust projections, Swift render state, queue/download/transcript/index data, and exported/imported records agree before and after the user-visible action.", [source_ref]),
        "navigation_state_restoration": s("Not assessed. Capture tab, sheet, detail path, back stack, deep-link, and relaunch restoration state so navigation success cannot hide stale or contradictory product state.", [source_ref, skill_ref]),
        "device_viewport_coverage": s("Not assessed. Record iPhone size class, Dynamic Type, orientation when relevant, and generated GitHub Pages desktop/mobile viewport checks for the published report page.", [skill_ref]),
        "media_session_background_continuity": s("Not assessed. For podcast-facing flows, verify audio session route, mini-player/full-player continuity, queue state, lock-screen/background behavior, interruptions, and remote commands or mark them N/A with rationale.", [source_ref]),
        "cross_screen_continuity": s("Not assessed. Capture before/after navigation state for adjacent surfaces and ensure mini-player, queue, transcript, agent, and settings remain coherent.", [source_ref]),
        "states_resilience": s("Not assessed. Capture empty, loading, denied, unavailable, offline, retry, and recovery states reached by this scenario.", [source_ref]),
        "touch_ergonomics": s("Not assessed. Audit primary controls for reachability, 44x44 pt targets, spacing, and gesture alternatives.", [skill_ref]),
        "motion_haptics": s("Not assessed. Record transition, haptic, and Reduce Motion behavior when motion is part of the flow.", [skill_ref]),
        "information_architecture": s("Not assessed. Confirm the user knows location, current mode, and next action from titles, tabs, bars, breadcrumbs, and hierarchy.", [skill_ref]),
        "content_hierarchy": s("Not assessed. Verify podcast/show/episode/transcript/agent content outranks chrome and does not truncate critical meaning.", [skill_ref]),
        "observability": s("Not assessed. Attach structured logs, metric IDs, provider/request IDs, relay IDs, and redacted traces needed to diagnose failures.", [source_ref]),
        "analytics_privacy_boundaries": s("Not assessed. Confirm useful event boundaries without PII, private keys, raw transcripts/audio, or provider secrets.", [source_ref]),
        "replayability_cassette_provenance": s(f"Not executed yet. {cassette_summary}", [source_ref, *cassette_refs]),
        "evidence_confidence": s("Evidence confidence is zero until a current build, device/OS, rerun count, freshness, and flake notes are attached.", [source_ref]),
        "device_os_matrix": s("No device/OS matrix has run. The first validation should record simulator/device, OS, locale, appearance, Dynamic Type, and network state.", [source_ref]),
        "sister_app_nmp_chirp_comparison": s("Not assessed. Review must compare applicable NMP doctrine and known Chirp/sister-app fixes before marking this flow cohesive.", [skill_ref]),
    }


def s(summary: str, refs: list[str], notes: list[str] | None = None) -> dict[str, Any]:
    return {"summary": summary, "evidence_refs": sorted(set(refs)), "notes": notes or []}


def related_for(scenario: Scenario, scenarios: list[Scenario]) -> list[str]:
    same = [item.scenario_id for item in scenarios if item.category_slug == scenario.category_slug and item.scenario_id != scenario.scenario_id]
    return same[:4]


def provider_from(cassette: str) -> str:
    for provider in ["openrouter", "ollama", "elevenlabs", "assemblyai", "perplexity"]:
        if provider in cassette.lower():
            return provider
    return "provider"


def artifacts_for(scenario: Scenario, source_ref: str, skill_ref: str, scenario_cassettes: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [
        {"id": source_ref, "type": "source_doc", "path": f"sources/catalog/{scenario.source_file}", "description": f"Catalog source row for {scenario.scenario_id} at line {scenario.source_line}.", "required": True, "redaction": {"status": "not_needed"}},
        {"id": skill_ref, "type": "source_doc", "path": "data/skill-grounding.json", "description": "Required review skills and skill-search terms for scenario critique.", "required": True, "redaction": {"status": "not_needed"}},
        *cassette_artifacts(scenario_cassettes),
    ]


def group_scores() -> dict[str, dict[str, Any]]:
    return {
        group: {
            "score": 0,
            "status": "incomplete",
            "rationale": "The group contains unassessed dimensions without current run evidence; product experience additionally requires individual and cluster-level coherence judgments.",
            "dimension_refs": dimensions,
        }
        for group, dimensions in GROUPS.items()
    }


def next_actions_for(scenario: Scenario, cassette_available: bool = False) -> list[dict[str, str]]:
    actions = [
        {"id": "run-scenario", "title": f"Execute {scenario.scenario_id} on the target simulator/device.", "status": "open", "owner": "validation-agent"},
        {"id": "attach-evidence", "title": "Attach screenshots, UI trees, logs, metrics, accessibility evidence, generated-page viewport checks, and redaction metadata.", "status": "open", "owner": "validation-agent"},
        {"id": "score-dimensions", "title": "Score every individual and grouped dimension from evidence.", "status": "open", "owner": "review-agent"},
        {"id": "file-defects", "title": "File GitHub issues for every actionable defect before leaving incomplete.", "status": "open", "owner": "review-agent"},
        {"id": "revalidate-defects", "title": "Re-run this scenario after each fix PR and attach revalidation run IDs.", "status": "open", "owner": "validation-agent"},
        {"id": "assign-owners", "title": "Assign owner and status for every blocker, gate, action, defect, and revalidation item.", "status": "open", "owner": "review-agent"},
    ]
    if (scenario.cassettes or scenario.provider_mode == "blocked") and cassette_available:
        actions.append({"id": "execute-with-cassettes", "title": "Run this provider-backed scenario in deterministic replay mode with the mapped cassette fixture(s).", "status": "open", "owner": "validation-agent"})
    elif scenario.cassettes or scenario.provider_mode == "blocked":
        actions.append({"id": "attach-cassettes", "title": "Attach deterministic provider, relay, STT, TTS, LLM, or network cassettes.", "status": "open", "owner": "validation-agent"})
    return actions


def live_dependency_rationale(scenario: Scenario, scenario_cassettes: list[dict[str, Any]]) -> str:
    if scenario_cassettes:
        ids = ", ".join(cassette["id"] for cassette in scenario_cassettes)
        return f"Replay fixture(s) are available for provider-backed validation: {ids}. The scenario still needs an execution run using those fixtures."
    if scenario.provider_mode == "blocked" or scenario.cassettes:
        return "No live run has been performed; dependencies are blocked until cassette or live-run evidence is attached."
    return "No live provider dependency is declared for this catalog scaffold."


def summary_for(record: dict[str, Any]) -> dict[str, Any]:
    return {
        "id": record["scenario"]["id"],
        "slug": record["scenario"]["slug"],
        "title": record["scenario"]["title"],
        "category": record["scenario"]["category"],
        "verdict": record["verdict"]["overall"],
        "provider_mode": record["run"]["provider_mode"],
        "readiness": record["readiness"]["ship_gate"],
        "launch_readiness": record["launch_assessment"]["launch_readiness"],
        "risk_classification": record["launch_assessment"]["risk_classification"],
        "evidence_quality": record["launch_assessment"]["evidence_quality"],
        "product_cluster": record["coherence"]["cluster"]["id"],
        "group_coherence": record["coherence"]["group_judgment"]["status"],
        "tags": record["scenario"]["tags"],
        "missing_evidence": missing_evidence_for(record),
        "url": f"scenarios/{record['scenario']['slug']}/",
        "data_url": f"scenarios/{record['scenario']['slug']}/data.json",
    }


def rollups_for(records: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "scenario_count": len(records),
        "by_verdict": count_by(records, lambda r: r["verdict"]["overall"]),
        "by_category": count_by(records, lambda r: r["scenario"]["category"]),
        "by_provider_mode": count_by(records, lambda r: r["run"]["provider_mode"]),
        "by_readiness": count_by(records, lambda r: r["readiness"]["ship_gate"]),
        "by_launch_readiness": count_by(records, lambda r: r["launch_assessment"]["launch_readiness"]),
        "by_risk_classification": count_by(records, lambda r: r["launch_assessment"]["risk_classification"]),
        "by_evidence_quality": count_by(records, lambda r: r["launch_assessment"]["evidence_quality"]),
        "by_product_cluster": count_by(records, lambda r: r["coherence"]["cluster"]["id"]),
        "by_group_coherence": count_by(records, lambda r: r["coherence"]["group_judgment"]["status"]),
        "by_tag": count_tags(records),
        "by_nmp_boundary": count_boundaries(records),
        "performance_required": sum(1 for r in records if "performance-required" in r["scenario"]["tags"]),
        "missing_evidence": missing_evidence_rollup(records),
        "instrumentation_gaps": count_gap_ids(records),
        "issues_by_severity": count_issues_by_severity(records),
        "open_issues_by_severity": count_issues_by_severity(records, open_only=True),
        "average_dimension_scores": average_scores(records, "dimension_scores", SECTION_TO_DIMENSION.values()),
        "average_group_scores": average_scores(records, "group_scores", GROUPS.keys()),
        "sources": [f"scenarios/{r['scenario']['slug']}/data.json" for r in records],
    }


def tags_for_records(records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [{"tag": tag, "count": count} for tag, count in sorted(count_tags(records).items())]


def count_by(records: list[dict[str, Any]], key_fn: Any) -> dict[str, int]:
    counts: dict[str, int] = {}
    for record in records:
        key = key_fn(record)
        counts[key] = counts.get(key, 0) + 1
    return dict(sorted(counts.items()))


def average_scores(records: list[dict[str, Any]], score_field: str, score_keys: Any) -> dict[str, float | int]:
    averages: dict[str, float | int] = {}
    for score_key in score_keys:
        values: list[float] = []
        for record in records:
            score = record.get(score_field, {}).get(score_key, {}).get("score")
            if isinstance(score, (int, float)) and not isinstance(score, bool):
                values.append(float(score))
        averages[score_key] = round(sum(values) / len(values), 2) if values else 0
    return averages


def count_tags(records: list[dict[str, Any]]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for record in records:
        for tag in record["scenario"]["tags"]:
            counts[tag] = counts.get(tag, 0) + 1
    return dict(sorted(counts.items()))


def count_issues_by_severity(records: list[dict[str, Any]], open_only: bool = False) -> dict[str, int]:
    counts: dict[str, int] = {}
    for record in records:
        for issue in record.get("issues", []):
            if open_only and issue.get("status") != "open":
                continue
            severity = issue.get("severity", "unknown")
            counts[severity] = counts.get(severity, 0) + 1
    return dict(sorted(counts.items()))


def count_boundaries(records: list[dict[str, Any]]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for record in records:
        for tag in record["scenario"]["tags"]:
            if re.fullmatch(r"d\d+", tag) or tag == "native-render":
                counts[tag] = counts.get(tag, 0) + 1
    return dict(sorted(counts.items()))


def missing_evidence_for(record: dict[str, Any]) -> list[str]:
    return [item["kind"] for item in record["evidence"]["missing"]]


def missing_evidence_rollup(records: list[dict[str, Any]]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for record in records:
        for kind in missing_evidence_for(record):
            counts[kind] = counts.get(kind, 0) + 1
    return dict(sorted(counts.items()))


def count_gap_ids(records: list[dict[str, Any]]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for record in records:
        for gap_item in record["instrumentation_gaps"]:
            counts[gap_item["id"]] = counts.get(gap_item["id"], 0) + 1
    return dict(sorted(counts.items()))


def average_dimension_scores(records: list[dict[str, Any]]) -> dict[str, float]:
    return {
        dimension: average_score(record["dimension_scores"][dimension]["score"] for record in records)
        for dimension in SECTION_TO_DIMENSION.values()
    }


def average_group_scores(records: list[dict[str, Any]]) -> dict[str, float]:
    return {
        group: average_score(record["group_scores"][group]["score"] for record in records)
        for group in GROUPS
    }


def average_score(values: Any) -> float:
    numeric = [value for value in values if isinstance(value, (int, float))]
    if not numeric:
        return 0.0
    return round(sum(numeric) / len(numeric), 2)


def validate_output(records: list[dict[str, Any]], out: Path) -> None:
    section_keys = set(SECTION_TO_DIMENSION.keys())
    dimension_keys = set(SECTION_TO_DIMENSION.values())
    data_files = list((out / "scenarios").glob("*/data.json"))
    if len(data_files) != len(records):
        raise ValueError(f"Expected {len(records)} data files, found {len(data_files)}")
    for record in records:
        observed = has_observed_data(record)
        if set(record["sections"]) != section_keys:
            raise ValueError(f"{record['scenario']['id']} section key mismatch")
        if set(record["dimension_scores"]) != dimension_keys:
            raise ValueError(f"{record['scenario']['id']} dimension key mismatch")
        if not observed and record["verdict"]["overall"] != "incomplete":
            raise ValueError(f"{record['scenario']['id']} scaffold must be incomplete")
        if not record["flow_steps"] or not record["execution"]["attempts"]:
            raise ValueError(f"{record['scenario']['id']} missing structured flow or attempt records")
        if not observed and record["coherence"]["group_judgment"]["status"] != "incomplete":
            raise ValueError(f"{record['scenario']['id']} scaffold group coherence must be incomplete")
        if set(record["coherence"]["cluster"]) != {"id", "title", "scenario_ids"}:
            raise ValueError(f"{record['scenario']['id']} coherence cluster key mismatch")
        missing_kinds = {item["kind"] for item in record["evidence"]["missing"]}
        placeholder_kinds = {item["kind"] for item in record["evidence"].get("placeholders", [])}
        if missing_kinds != placeholder_kinds:
            raise ValueError(f"{record['scenario']['id']} evidence placeholders do not match missing evidence")
        if set(record["quality_review"]) != {"ui", "ux", "performance", "accessibility", "reliability", "privacy_security", "content_localization", "controls_gestures", "offline_resume", "observability"}:
            raise ValueError(f"{record['scenario']['id']} quality review key mismatch")
        if not observed and not record["instrumentation_gaps"]:
            raise ValueError(f"{record['scenario']['id']} must expose instrumentation gaps")
        for artifact in record["evidence"]["artifacts"]:
            if not (out / artifact["path"]).exists():
                raise ValueError(f"{record['scenario']['id']} missing artifact path {artifact['path']}")
            if artifact["type"] == "screenshot":
                required = {"alt", "caption", "step_id", "captured_at", "device", "os_version", "sha256", "required"}
                if not required.issubset(artifact):
                    raise ValueError(f"{record['scenario']['id']} screenshot artifact {artifact['id']} missing metadata")
    if json.loads((out / "data" / "rollups.json").read_text())["scenario_count"] != len(records):
        raise ValueError("Rollup scenario count disagrees with records")


def has_observed_data(record: dict[str, Any]) -> bool:
    evidence = record.get("evidence", {})
    execution = record.get("execution", {})
    verdict = record.get("verdict", {})
    return bool(
        record.get("metrics")
        or any(not generated_validation_issue(issue) for issue in record.get("issues", []))
        or len(evidence.get("artifacts", [])) > 2
        or execution.get("status") not in {None, "not_run"}
        or verdict.get("overall") not in {None, "incomplete"}
    )


def validate_schema_contract(records: list[dict[str, Any]], schema_path: Path) -> None:
    schema = json.loads(schema_path.read_text())
    required_top = set(schema["required"])
    required_sections = set(schema["properties"]["sections"]["required"])
    required_dimensions = set(schema["$defs"]["dimension_scores"]["required"])
    if required_sections != set(SECTION_TO_DIMENSION.keys()):
        raise ValueError("Generator section contract disagrees with JSON Schema")
    if required_dimensions != set(SECTION_TO_DIMENSION.values()):
        raise ValueError("Generator dimension contract disagrees with JSON Schema")
    for record in records:
        if set(record) != required_top:
            raise ValueError(f"{record['scenario']['id']} top-level schema keys mismatch")
        if set(record["sections"]) != required_sections:
            raise ValueError(f"{record['scenario']['id']} schema section keys mismatch")
        if set(record["dimension_scores"]) != required_dimensions:
            raise ValueError(f"{record['scenario']['id']} schema dimension keys mismatch")
