from __future__ import annotations

from typing import Any

from catalog import slugify
from contract import SECTION_LABELS
from next_wave_render import scenario_next_wave_block
from render_html import (
    action_list,
    artifact_count_for_kind,
    artifact_table,
    badge,
    e,
    hero,
    highest_severity,
    key_values,
    list_block,
    nav_bar,
    p,
    page,
    rel,
    score_table,
    screenshot_gallery,
    section,
)
from records import missing_evidence_for


DETAIL_GROUPS = [
    (
        "Functional Validation Detail",
        [
            "persona_job_acceptance",
            "flow",
            "attempted_test",
            "scenario_setup",
            "execution_attempts",
            "expected_behavior",
            "actual_result",
            "error_recovery_behavior",
            "offline_resume_behavior",
        ],
    ),
    (
        "Evidence And Replay Detail",
        [
            "artifacts",
            "evidence_provenance",
            "review_skill_grounding",
            "replayability_cassette_provenance",
            "evidence_confidence",
            "device_os_matrix",
            "device_viewport_coverage",
            "instrumentation_gaps",
        ],
    ),
    (
        "Product Experience Detail",
        [
            "ui_polish_report",
            "ux_polish_report",
            "liquid_glass_ios_primitives",
            "cross_screen_continuity",
            "states_resilience",
            "touch_ergonomics",
            "motion_haptics",
            "information_architecture",
            "content_hierarchy",
            "product_coherence_in_context",
            "product_cluster_coherence",
        ],
    ),
    (
        "Engineering Boundary Detail",
        [
            "performance_metrics",
            "accessibility_dynamic_type",
            "privacy_security",
            "nmp_architecture_cohesiveness",
            "data_integrity_state_sync",
            "navigation_state_restoration",
            "media_session_background_continuity",
            "observability",
            "analytics_privacy_boundaries",
            "sister_app_nmp_chirp_comparison",
        ],
    ),
    (
        "Follow-Through Detail",
        [
            "before_after_deltas",
            "reliability_flakiness",
            "regression_risk",
            "defects_issues_filed",
            "revalidation_status",
            "risks_follow_up",
            "readiness_gates",
            "verdict",
            "next_actions",
            "owner_status",
            "localization_content_quality",
            "controls_gestures_audio",
        ],
    ),
]


def render_scenario_page(record: dict[str, Any], depth: int, next_wave: dict[str, Any] | None = None) -> str:
    scenario = record["scenario"]
    nav = record["page"]["navigation"]
    nav_links = [
        ("All scenarios", rel("scenarios/", depth)),
        ("Category", rel(f"scenarios/{slugify(scenario['category'])}/", depth)),
        ("Data JSON", "data.json"),
    ]
    if nav.get("previous_url"):
        nav_links.append(("Previous", nav["previous_url"]))
    if nav.get("next_url"):
        nav_links.append(("Next", nav["next_url"]))
    body = [
        nav_bar(nav_links),
        hero(f"{scenario['id']} - {scenario['title']}", f"{scenario['category']} - {badge(record['verdict']['overall'])}"),
        section("Scenario Identity And Links", key_values(identity_for(record))),
        section("Product Intent And Acceptance Criteria", product_context_block(record["product_context"])),
        section("Launch Readiness Summary", launch_assessment_block(record["launch_assessment"])),
        section("What Was Attempted And Test Intent", test_intent_block(record)),
        section("Flow Overview And Steps", bdd_block(scenario["bdd"]) + flow_step_table(record["flow_steps"])),
        section("Data And Control-Plane Setup", control_plane_block(record)),
        section("Evidence Readiness Matrix", evidence_readiness_matrix(record)),
        section("Provider Replay Coverage", provider_replay_block(record, depth)),
        section("Preconditions, Fixtures, Cassettes, And Runtime Metadata", key_values(metadata_for(record)) + device_table(record["run"]["device_matrix"]) + cassette_table(record["run"].get("cassettes", []))),
        section("Execution Attempts, Retries, And Branches", attempts_block(record["execution"])),
        section("Results And Verdict", p(record["verdict"]["summary"]) + key_values({"Overall": record["verdict"]["overall"], "Gate explanation": record["verdict"]["score_gate_explanation"]})),
        section("Screenshot Evidence", screenshot_gallery(record["evidence"]["artifacts"], depth) + evidence_placeholder_grid(record["evidence"], {"screenshot"})),
        section("Evidence Inventory", missing_evidence_table(record["evidence"]) + evidence_placeholder_grid(record["evidence"], None) + artifact_table(record["evidence"]["artifacts"], depth)),
        section("Replay, Cassettes, And Provenance Detail", replay_provenance_block(record, depth)),
        section("Skill-Grounded Mobile Design Rubric", review_grounding_block(record)),
        section("Quality Review", quality_table(record["quality_review"])),
        section("UI Polish Report", review_area_block(record, "ui", "ui_polish_report")),
        section("UX Polish Report", review_area_block(record, "ux", "ux_polish_report")),
        section("Accessibility, Dynamic Type, And Touch Ergonomics Detail", accessibility_mobile_block(record)),
        section("Performance Metrics And Interaction Latency", metrics_block(record)),
        section("Liquid Glass And iOS Primitive Detail", liquid_glass_block(record)),
        section("Navigation, Orientation, And Information Architecture", navigation_orientation_block(record)),
        section("Animation, Transition, And Haptics Quality", motion_block(record)),
        section("State, Recovery, And Continuity Coverage", state_recovery_block(record)),
        section("Product Flow Cohesiveness And Group Coherent-Product Judgment", coherence_block(record["coherence"])),
        section("NMP/RMP Boundary And Data Integrity", nmp_boundary_block(record)),
        section("Product-Level Assessment", product_assessment_block(record)),
        section("Readiness Gates", readiness_block(record["readiness"])),
        section("Instrumentation Gaps And Missing Evidence", gap_table(record["instrumentation_gaps"])),
        section("Risk Severity And Validation Confidence", risk_confidence_block(record)),
        section("Issue Filing And Revalidation Ledger", issue_revalidation_block(record)),
        section("Risks, Defects, Issue Links, And Follow-Up", risk_table(record["risks"]) + issue_table(record["issues"]) + action_list(record["next_actions"])),
        section("Grouped Scores And Coherent Product Judgment", score_table(record["group_scores"])),
        section("Individual Dimension Judgments", score_table(record["dimension_scores"])),
        *[section(title, detail_group_block(record, keys)) for title, keys in DETAIL_GROUPS],
    ]
    if next_wave:
        block = scenario_next_wave_block(record, next_wave)
        if block:
            body.insert(5, section("Next-Wave Execution Manifest", block))
    return page(f"{scenario['id']} - {scenario['title']}", depth, "\n".join(body))


def identity_for(record: dict[str, Any]) -> dict[str, str]:
    scenario = record["scenario"]
    nav = record["page"]["navigation"]
    return {
        "Scenario": scenario["id"],
        "Title": scenario["title"],
        "Category": scenario["category"],
        "Tags": ", ".join(scenario["tags"]),
        "Source": scenario["source_path"],
        "Canonical URL": record["page"]["canonical_url"],
        "Previous": nav.get("previous_url", ""),
        "Next": nav.get("next_url", ""),
        "Data": "data.json",
    }


def metadata_for(record: dict[str, Any]) -> dict[str, str]:
    return {
        "Source": record["scenario"]["source_path"],
        "Generated": record["page"]["generated_at"],
        "Commit": record["run"]["source_commit"],
        "Branch": record["run"].get("branch", ""),
        "Provider mode": record["run"]["provider_mode"],
        "Generator": record["run"]["generator_version"],
    }


def bdd_block(bdd: dict[str, list[str]]) -> str:
    return f"<dl><dt>Given</dt><dd>{e('; '.join(bdd['given']))}</dd><dt>When</dt><dd>{e('; '.join(bdd['when']))}</dd><dt>Then</dt><dd>{e('; '.join(bdd['then']))}</dd></dl>"


def product_context_block(context: dict[str, Any]) -> str:
    criteria = "".join(f"<li>{e(item)}</li>" for item in context["acceptance_criteria"])
    expectations = "".join(f"<li>{e(item)}</li>" for item in context["platform_expectations"])
    cluster = context["scenario_cluster"]
    values = {"Persona": context["persona"], "Job": context["job_to_be_done"], "User value": context["user_value"], "Cluster": f"{cluster['title']} ({cluster['id']})", "Related scenarios": ", ".join(cluster["related_scenarios"])}
    return key_values(values) + f"<h3>Acceptance Criteria</h3><ul>{criteria}</ul><h3>Platform Expectations</h3><ul>{expectations}</ul>"


def launch_assessment_block(assessment: dict[str, Any]) -> str:
    cards = [
        ("Launch readiness", badge(assessment["launch_readiness"]), "Current release gate for this scenario."),
        ("Risk class", badge(assessment["risk_classification"]), "Highest scenario risk or linked defect severity."),
        ("Evidence quality", badge(assessment["evidence_quality"]), "Whether evidence can support product judgment."),
        ("Accessibility", badge(assessment["accessibility_status"]), "Assistive-settings coverage status."),
        ("Regression coverage", badge(assessment["regression_coverage"]), "Adjacent-flow rerun coverage status."),
        ("Dependency posture", e(assessment["dependency_posture"]), "Provider, relay, fixture, or cassette state."),
    ]
    html_cards = "".join(f"<article class=\"launch-card\"><span>{e(label)}</span><strong>{value}</strong><p>{e(help_text)}</p></article>" for label, value, help_text in cards)
    values = {
        "Owner": assessment["scenario_owner"],
        "Scenario status": assessment["scenario_status"],
        "Blocking gates": ", ".join(assessment["blocking_gates"]) or "none",
        "Missing evidence": ", ".join(assessment["missing_evidence"]) or "none",
        "Issue links": ", ".join(assessment["issue_refs"]) or "none",
    }
    return f"<div class=\"launch-grid\">{html_cards}</div>" + key_values(values) + p(assessment["product_judgment"]) + p(assessment["whole_product_judgment"])


def test_intent_block(record: dict[str, Any]) -> str:
    scenario = record["scenario"]
    execution = record["execution"]
    return key_values(
        {
            "Intent": record["sections"]["attempted_test"]["summary"],
            "Acceptance target": "; ".join(scenario["bdd"]["then"]),
            "Attempt status": execution["status"],
            "Attempt count": str(len(execution["attempts"])),
            "Branch count": str(len(execution["branch_coverage"])),
            "Evidence confidence": record["dimension_scores"]["evidence_confidence"]["status"],
        }
    )


def control_plane_block(record: dict[str, Any]) -> str:
    run = record["run"]
    environment = run["environment"]
    values = {
        "Seed state / fixtures": ", ".join(run.get("fixtures", [])) or "none declared",
        "Provider mode": run["provider_mode"],
        "Live dependency rationale": run.get("live_dependency_rationale", ""),
        "Locale": environment["locale"],
        "Appearance": environment["appearance"],
        "Network": environment["network_condition"],
        "Launch arguments": ", ".join(environment.get("launch_arguments", [])) or "none",
    }
    return key_values(values) + p(record["sections"]["scenario_setup"]["summary"])


def evidence_readiness_matrix(record: dict[str, Any]) -> str:
    evidence = record["evidence"]
    artifacts = evidence["artifacts"]
    missing_by_kind = {item["kind"]: item for item in evidence["missing"]}
    rows = []
    for kind in evidence["required_kinds"]:
        present = artifact_count_for_kind(artifacts, kind)
        missing = missing_by_kind.get(kind)
        status = "present" if present else ("missing" if missing else "metadata-only")
        blocks = ", ".join(missing.get("blocks_dimensions", [])) if missing else ""
        reason = missing.get("reason", "Evidence exists or this scaffold only requires source metadata.") if missing else "Evidence exists or this scaffold only requires source metadata."
        rows.append(f"<tr><td>{e(kind)}</td><td>{badge(status)}</td><td>{e(str(present))}</td><td>{e(reason)}</td><td>{e(blocks or 'none')}</td></tr>")
    return (
        p("This matrix is mechanical: it reports what the record contains now and keeps absent screenshots, UI trees, logs, metrics, cassettes, generated-page viewport checks, and accessibility audits visible.")
        + "<table><caption>Required evidence by kind</caption><thead><tr><th>Evidence</th><th>Status</th><th>Artifacts</th><th>Reason</th><th>Blocked Dimensions</th></tr></thead>"
        + f"<tbody>{''.join(rows)}</tbody></table>"
        + evidence_placeholder_grid(evidence, None)
    )


def provider_replay_block(record: dict[str, Any], depth: int) -> str:
    cassettes = record["run"].get("cassettes", [])
    provider_mode = record["run"]["provider_mode"]
    if not cassettes:
        if provider_mode == "none":
            return p("This scenario declares no provider, relay, STT, TTS, LLM, search, or replay dependency.")
        return p("No replay cassette is mapped yet. Provider-backed validation remains blocked until a redacted fixture is attached or a live-only rationale is approved.")
    return p("Replay coverage is available for this scenario. The scenario still needs an execution run with POD0_PROVIDER_CASSETTE_DIR so screenshots, UI tree, logs, and metrics prove the provider-backed flow.") + cassette_artifact_table(record, depth)


def flow_step_table(steps: list[dict[str, Any]]) -> str:
    rows = "".join(f"<tr><td>{e(step['id'])}</td><td>{e(step['phase'])}</td><td>{e(step['action'])}</td><td>{e(step['expected'])}</td><td>{badge(step['status'])}</td><td>{e(', '.join(step['evidence_required']))}</td><td>{e(', '.join(step['evidence_refs']) or 'none')}</td></tr>" for step in steps)
    return f"<table><caption>Step-by-step flow</caption><thead><tr><th>Step</th><th>Phase</th><th>Action</th><th>Expected</th><th>Status</th><th>Evidence Needed</th><th>Evidence Refs</th></tr></thead><tbody>{rows}</tbody></table>"


def device_table(devices: list[dict[str, Any]]) -> str:
    rows = "".join(f"<tr><td>{e(item['name'])}</td><td>{e(item['os_version'])}</td><td>{e(item['form_factor'])}</td><td>{e(item.get('udid', ''))}</td></tr>" for item in devices)
    return f"<table><caption>Device, simulator, and runtime matrix</caption><thead><tr><th>Name</th><th>OS</th><th>Form Factor</th><th>UDID</th></tr></thead><tbody>{rows}</tbody></table>"


def cassette_table(cassettes: list[dict[str, Any]]) -> str:
    if not cassettes:
        return p("No cassettes declared for this scaffold.")
    rows = "".join(f"<tr><td>{e(item['id'])}</td><td>{e(item['provider'])}</td><td>{badge(item['mode'])}</td><td>{e(item['redaction_hash'])}</td></tr>" for item in cassettes)
    return f"<table><caption>Provider, relay, replay, and cassette data</caption><thead><tr><th>ID</th><th>Provider</th><th>Mode</th><th>Redaction Hash</th></tr></thead><tbody>{rows}</tbody></table>"


def attempts_block(execution: dict[str, Any]) -> str:
    rows = "".join(f"<tr><td>{e(item['id'])}</td><td>{badge(item['status'])}</td><td>{e(item['executor'])}</td><td>{e(', '.join(item['tools']))}</td><td>{e('; '.join(item['commands']) or 'not run')}</td><td>{e(item['notes'])}</td><td>{e(', '.join(item.get('evidence_refs', [])) or 'none')}</td></tr>" for item in execution["attempts"])
    branches = "".join(f"<li><strong>{e(item['id'])}</strong>: {e(item['trigger'])} -> {e(item['expected'])} ({e(item['status'])})</li>" for item in execution["branch_coverage"])
    return f"<p>{e(execution['retry_policy'])}</p><table><caption>Execution attempts</caption><thead><tr><th>Attempt</th><th>Status</th><th>Executor</th><th>Tools</th><th>Commands</th><th>Notes</th><th>Evidence Refs</th></tr></thead><tbody>{rows}</tbody></table><h3>Branches</h3><ul>{branches}</ul>"


def missing_evidence_table(evidence: dict[str, Any]) -> str:
    rows = "".join(f"<tr><td>{e(item['kind'])}</td><td>{e(item['reason'])}</td><td>{e(', '.join(item['blocks_dimensions']))}</td></tr>" for item in evidence["missing"])
    return f"<p>{e(evidence['redaction_summary'])}</p><p><strong>Required evidence kinds:</strong> {e(', '.join(evidence['required_kinds']))}</p><table><caption>Missing evidence inventory</caption><thead><tr><th>Kind</th><th>Reason</th><th>Blocked Dimensions</th></tr></thead><tbody>{rows}</tbody></table>"


def evidence_placeholder_grid(evidence: dict[str, Any], kinds: set[str] | None) -> str:
    placeholders = [item for item in evidence.get("placeholders", []) if kinds is None or item["kind"] in kinds]
    if not placeholders:
        return ""
    cards = "".join(
        f"<article class=\"evidence-placeholder\"><span>{e(item['kind'])}</span><strong>Missing Evidence</strong><p>{e(item['message'])}</p><p class=\"muted\">Blocks: {e(', '.join(item['blocks_dimensions']))}</p></article>"
        for item in placeholders
    )
    return "<div class=\"placeholder-grid\">" + cards + "</div>"


def replay_provenance_block(record: dict[str, Any], depth: int) -> str:
    keys = ["evidence_provenance", "replayability_cassette_provenance", "evidence_confidence"]
    return detail_group_block(record, keys) + cassette_artifact_table(record, depth) + artifact_table(record["evidence"]["artifacts"], depth)


def review_grounding_block(record: dict[str, Any]) -> str:
    grounding = record["review_grounding"]
    rows = "".join(
        f"<tr><td>{badge('selected' if item['selected'] else 'considered')}</td><td>{e(item['name'])}</td><td>{e(item['search_terms'])}</td><td>{e(item['coverage'])}</td></tr>"
        for item in grounding["all_considered"]
    )
    return (
        key_values({"Search command": grounding["search_command"]})
        + list_block("Template Impact", grounding["template_impact"])
        + "<table><caption>Skill search results and loaded grounding</caption><thead><tr><th>Use</th><th>Skill</th><th>Search Terms</th><th>Review Coverage</th></tr></thead>"
        + f"<tbody>{rows}</tbody></table>"
        + p(record["sections"]["review_skill_grounding"]["summary"])
    )


def quality_table(quality: dict[str, Any]) -> str:
    rows = "".join(f"<tr><td>{e(name)}</td><td>{badge(item['status'])}</td><td>{e(item['summary'])}</td><td>{e(', '.join(item['checks']))}</td><td>{e(', '.join(item['gaps']))}</td></tr>" for name, item in quality.items())
    return f"<table><caption>UI, UX, accessibility, performance, reliability, content, and observability review</caption><thead><tr><th>Area</th><th>Status</th><th>Summary</th><th>Checks</th><th>Gaps</th></tr></thead><tbody>{rows}</tbody></table>"


def review_area_block(record: dict[str, Any], area_key: str, section_key: str) -> str:
    area = record["quality_review"][area_key]
    detail = record["sections"][section_key]
    dimension_key = {"ui_polish_report": "ui_polish", "ux_polish_report": "ux_polish"}[section_key]
    return key_values({"Status": area["status"], "Score": str(record["dimension_scores"][dimension_key]["score"]), "Summary": detail["summary"], "Evidence refs": ", ".join(detail.get("evidence_refs", [])) or "none"}) + list_block("Checks", area["checks"]) + list_block("Gaps", area["gaps"])


def accessibility_mobile_block(record: dict[str, Any]) -> str:
    keys = ["accessibility_dynamic_type", "touch_ergonomics", "controls_gestures_audio"]
    return detail_group_block(record, keys) + quality_area_cards(record, ["accessibility", "controls_gestures"])


def metrics_block(record: dict[str, Any]) -> str:
    rows = "".join(
        f"<tr><td>{e(item['name'])}</td><td>{e(str(item['value']))} {e(item['unit'])}</td><td>{e(item['budget'])}</td><td>{badge(item['status'])}</td><td>{e(item['method'])}</td><td>{e(', '.join(item.get('evidence_refs', [])) or 'none')}</td></tr>"
        for item in record["metrics"]
    )
    metrics = "<p>No metric traces are attached yet. Performance dimensions cannot pass without measured values, budgets, collection methods, and artifacts.</p>" if not rows else f"<table><caption>Measured performance</caption><thead><tr><th>Metric</th><th>Value</th><th>Budget</th><th>Status</th><th>Method</th><th>Evidence Refs</th></tr></thead><tbody>{rows}</tbody></table>"
    return metrics + p(record["sections"]["performance_metrics"]["summary"]) + list_block("Required latency checks", record["quality_review"]["performance"]["checks"]) + evidence_placeholder_grid(record["evidence"], {"metric_trace"})


def liquid_glass_block(record: dict[str, Any]) -> str:
    return detail_group_block(record, ["liquid_glass_ios_primitives", "ui_polish_report", "accessibility_dynamic_type"]) + list_block("Grounded iOS/Liquid Glass Checks", record["quality_review"]["ui"]["checks"])


def navigation_orientation_block(record: dict[str, Any]) -> str:
    sections = record["sections"]
    return p(sections["information_architecture"]["summary"]) + p(sections["cross_screen_continuity"]["summary"]) + p(sections["content_hierarchy"]["summary"]) + list_block("Evidence refs", record["dimension_scores"]["information_architecture"]["evidence_refs"])


def motion_block(record: dict[str, Any]) -> str:
    return p(record["sections"]["motion_haptics"]["summary"]) + p(record["sections"]["controls_gestures_audio"]["summary"]) + list_block("Mobile interaction checks", record["quality_review"]["controls_gestures"]["checks"])


def state_recovery_block(record: dict[str, Any]) -> str:
    keys = ["error_recovery_behavior", "states_resilience", "offline_resume_behavior", "navigation_state_restoration", "media_session_background_continuity", "cross_screen_continuity"]
    return detail_group_block(record, keys)


def nmp_boundary_block(record: dict[str, Any]) -> str:
    keys = ["nmp_architecture_cohesiveness", "data_integrity_state_sync", "privacy_security", "analytics_privacy_boundaries", "observability", "sister_app_nmp_chirp_comparison"]
    return detail_group_block(record, keys) + list_block("NMP/RMP Review Anchors", ["Rust owns state, policy, routing, time, replay, and privacy decisions.", "Native shell renders and executes raw OS capabilities only.", "FFI snapshots stay bounded to open views and do not expose event stores or secret-bearing state.", "Provider, relay, network, STT, TTS, and LLM behavior is replayable or explicitly marked live-only."])


def product_assessment_block(record: dict[str, Any]) -> str:
    readiness = record["readiness"]
    coherence = record["coherence"]
    values = {
        "Product-level status": readiness["ship_gate"],
        "Individual coherence": coherence["individual_judgment"]["status"],
        "Cluster coherence": coherence["group_judgment"]["status"],
        "Validation confidence": record["dimension_scores"]["evidence_confidence"]["status"],
        "Highest risk severity": highest_severity(record["risks"]),
        "Release decision": "blocked until evidence, scores, and linked defects support the grouped assessment",
    }
    return key_values(values) + score_table(record["group_scores"]) + list_block("Product cohesion themes", coherence["themes"])


def risk_confidence_block(record: dict[str, Any]) -> str:
    values = {
        "Highest risk severity": highest_severity(record["risks"]),
        "Open risk count": str(len(record["risks"])),
        "Missing evidence": ", ".join(missing_evidence_for(record)) or "none",
        "Readiness": record["readiness"]["ship_gate"],
        "Evidence confidence": record["sections"]["evidence_confidence"]["summary"],
    }
    return key_values(values) + gap_table(record["instrumentation_gaps"])


def issue_revalidation_block(record: dict[str, Any]) -> str:
    return detail_group_block(record, ["defects_issues_filed", "revalidation_status", "owner_status", "next_actions"]) + issue_table(record["issues"]) + action_list(record["next_actions"])


def coherence_block(coherence: dict[str, Any]) -> str:
    cluster = coherence["cluster"]
    themes = "".join(f"<li>{e(item)}</li>" for item in coherence["themes"])
    risks = "".join(f"<li>{e(item)}</li>" for item in coherence["cross_scenario_risks"])
    return key_values({"Cluster": f"{cluster['title']} ({cluster['id']})", "Related scenarios": ", ".join(cluster["scenario_ids"]), "Individual judgment": coherence["individual_judgment"]["summary"], "Group judgment": coherence["group_judgment"]["summary"]}) + f"<h3>Cross-Scenario Themes</h3><ul>{themes}</ul><h3>Cross-Scenario Risks</h3><ul>{risks}</ul>"


def readiness_block(readiness: dict[str, Any]) -> str:
    rows = "".join(f"<tr><td>{e(item['id'])}</td><td>{badge(item['status'])}</td><td>{e(item['requirement'])}</td><td>{e(item['owner'])}</td><td>{e(', '.join(item['evidence_refs']) or 'none')}</td></tr>" for item in readiness["gates"])
    blockers = "".join(f"<li>{e(item)}</li>" for item in readiness["blocking_reasons"])
    return f"<p><strong>Ship gate:</strong> {badge(readiness['ship_gate'])}</p><ul>{blockers}</ul><table><caption>Readiness gates</caption><thead><tr><th>Gate</th><th>Status</th><th>Requirement</th><th>Owner</th><th>Evidence Refs</th></tr></thead><tbody>{rows}</tbody></table>"


def detail_group_block(record: dict[str, Any], keys: list[str]) -> str:
    return "\n".join(render_required_section(key, record["sections"][key]) for key in keys)


def quality_area_cards(record: dict[str, Any], keys: list[str]) -> str:
    cards = []
    for key in keys:
        item = record["quality_review"][key]
        cards.append(f"<article class=\"evidence-placeholder\"><span>{e(key)}</span><strong>{badge(item['status'])}</strong><p>{e(item['summary'])}</p><p class=\"muted\">Gaps: {e(', '.join(item['gaps']))}</p></article>")
    return "<div class=\"placeholder-grid\">" + "".join(cards) + "</div>"


def gap_table(gaps: list[dict[str, Any]]) -> str:
    rows = "".join(f"<tr><td>{e(item['id'])}</td><td>{badge(item['severity'])}</td><td>{e(item['summary'])}</td><td>{e(', '.join(item['affected_dimensions']))}</td><td>{e(item['owner'])}</td></tr>" for item in gaps)
    return f"<table><caption>Instrumentation gaps</caption><thead><tr><th>Gap</th><th>Severity</th><th>Summary</th><th>Affected Dimensions</th><th>Owner</th></tr></thead><tbody>{rows}</tbody></table>"


def risk_table(risks: list[dict[str, Any]]) -> str:
    rows = "".join(f"<tr><td>{e(item['id'])}</td><td>{badge(item['severity'])}</td><td>{e(item['priority'])}</td><td>{e(item['title'])}</td><td>{e(', '.join(item['affected_dimensions']))}</td><td>{e(item['mitigation'])}</td></tr>" for item in risks)
    return f"<table><caption>Risks and recommended follow-up</caption><thead><tr><th>ID</th><th>Severity</th><th>Priority</th><th>Risk</th><th>Dimensions</th><th>Mitigation</th></tr></thead><tbody>{rows}</tbody></table>"


def issue_table(issues: list[dict[str, Any]]) -> str:
    if not issues:
        return p("No defects are linked yet because this scaffold has not been executed.")
    rows = "".join(f"<tr><td><a href=\"{e(item['url'])}\">{e(item['id'])}</a></td><td>{badge(item['severity'])}</td><td>{e(item['title'])}</td><td>{badge(item['status'])}</td></tr>" for item in issues)
    return f"<table><caption>Defects and issue/PR links</caption><thead><tr><th>Issue</th><th>Severity</th><th>Title</th><th>Status</th></tr></thead><tbody>{rows}</tbody></table>"


def cassette_artifact_table(record: dict[str, Any], depth: int) -> str:
    cassettes = record["run"].get("cassettes", [])
    if not cassettes:
        return ""
    artifact_by_id = {artifact["id"]: artifact for artifact in record["evidence"]["artifacts"]}
    rows = []
    for item in cassettes:
        artifact = artifact_by_id.get(f"cassette:{item['id']}")
        artifact_link = f"<a href=\"{rel(artifact['path'], depth)}\">fixture JSON</a>" if artifact else "pending fixture artifact"
        rows.append(f"<tr><td>{e(item['id'])}</td><td>{e(item['provider'])}</td><td>{badge(item['mode'])}</td><td>{e(item['redaction_hash'])}</td><td>{artifact_link}</td></tr>")
    return "<table><caption>Replay fixtures mapped to this scenario</caption><thead><tr><th>Cassette</th><th>Provider</th><th>Mode</th><th>Redaction hash</th><th>Artifact</th></tr></thead>" + f"<tbody>{''.join(rows)}</tbody></table>"


def render_required_section(key: str, value: dict[str, Any]) -> str:
    notes = "".join(f"<li>{e(note)}</li>" for note in value.get("notes", []))
    refs = ", ".join(value.get("evidence_refs", [])) or "none"
    note_html = f"<ul>{notes}</ul>" if notes else ""
    return f"<article class=\"report-section\"><h3>{e(SECTION_LABELS[key])}</h3><p>{e(value['summary'])}</p><p class=\"muted\">Evidence refs: {e(refs)}</p>{note_html}</article>"
