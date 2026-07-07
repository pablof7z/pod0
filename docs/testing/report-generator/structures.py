from __future__ import annotations

from typing import Any

from catalog import Scenario
from contract import QUALITY_AREAS, READINESS_GATES, SKILL_GROUNDING, SKILL_SEARCH_QUERY


def product_context_for(scenario: Scenario, scenarios: list[Scenario]) -> dict[str, Any]:
    return {
        "persona": persona_for(scenario),
        "job_to_be_done": job_for(scenario),
        "user_value": user_value_for(scenario),
        "acceptance_criteria": [
            f"Given {scenario.bdd['given'][0]}",
            f"When {scenario.bdd['when'][0]}",
            f"Then {scenario.bdd['then'][0]}",
            f"Architecture boundary remains within {', '.join(scenario.boundaries) or 'the declared product surface'}.",
        ],
        "platform_expectations": [
            "iPhone-first flow uses system navigation/chrome, safe areas, SF typography, semantic color, 44 pt tap targets, and reachable primary controls.",
            "Accessibility evidence covers VoiceOver, Dynamic Type, contrast, motion/transparency settings, and touch target behavior before UI scores can pass.",
            "Provider, relay, transcript, audio, or LLM dependencies are deterministic through fixtures/cassettes or explicitly marked live-only.",
            "State, navigation, queue, playback, and generated report pages are checked across device, viewport, and resume contexts.",
        ],
        "scenario_cluster": {
            "id": scenario.category_slug,
            "title": scenario.category,
            "related_scenarios": related_for(scenario, scenarios, limit=8),
        },
    }


def flow_steps_for(scenario: Scenario) -> list[dict[str, Any]]:
    return [
        step("step-01-preconditions", "Preconditions", f"Prepare {scenario.bdd['given'][0]}", "Fixture, device, provider, account, cassette, and network state match the scenario setup.", scenario, ["source_doc", "command_output"]),
        step("step-02-action", "Primary Action", f"Execute {scenario.bdd['when'][0]}", "The user-visible route follows the intended path without hidden state mutation.", scenario, ["screenshot", "ui_tree", "log"]),
        step("step-03-result", "Expected Result", f"Observe that {scenario.bdd['then'][0]}", "Observed UI, logs, metrics, and state projections agree with the expected behavior.", scenario, required_evidence_kinds(scenario)),
        step("step-04-exit", "Exit State", "Leave the flow through the natural product path", "Adjacent screens, back stack, mini-player, account, transcript, agent, or library state remain coherent.", scenario, ["screenshot", "ui_tree"]),
    ]


def execution_for(scenario: Scenario, generated_at: str) -> dict[str, Any]:
    return {
        "status": "not_run",
        "attempts": [
            {
                "id": "attempt-001",
                "status": "not_run",
                "started_at": generated_at,
                "completed_at": generated_at,
                "executor": "validation-agent",
                "tools": ["not-run"],
                "commands": [],
                "branches": branch_records_for(scenario),
                "notes": "Catalog scaffold only. Replace this attempt with simulator/manual/automation execution metadata.",
                "evidence_refs": [source_ref(scenario)],
            }
        ],
        "retry_policy": "Record every retry as a separate attempt with cause, changed fixture/state, and whether the retry invalidates earlier evidence.",
        "branch_coverage": branch_records_for(scenario),
    }


def evidence_inventory_for(scenario: Scenario) -> dict[str, Any]:
    required = required_evidence_kinds(scenario)
    missing = [
        {
            "kind": kind,
            "reason": missing_reason_for(kind, scenario),
            "blocks_dimensions": blocked_dimensions_for(kind),
            "required": True,
        }
        for kind in required
        if kind not in {"source_doc"}
    ]
    return {
        "required_kinds": required,
        "missing": missing,
        "placeholders": evidence_placeholders_for(missing),
        "redaction_summary": "Only source catalog and rubric metadata are published. Secret-bearing logs, provider payloads, private keys, audio, transcripts, and relay payloads remain forbidden until redacted.",
    }


def evidence_placeholders_for(missing: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [
        {
            "id": f"placeholder:{item['kind']}",
            "kind": item["kind"],
            "message": f"Missing required {item['kind'].replace('_', ' ')} evidence is blocking this report area.",
            "required": item["required"],
            "blocks_dimensions": item["blocks_dimensions"],
        }
        for item in missing
    ]


def review_grounding_for() -> dict[str, Any]:
    selected = [item for item in SKILL_GROUNDING if item.get("selected")]
    return {
        "search_command": f"npx skills search \"{SKILL_SEARCH_QUERY}\"",
        "selected_skills": selected,
        "all_considered": SKILL_GROUNDING,
        "template_impact": [
            "iOS Glass UI Designer drives restrained material judgment: native chrome first, glass only for hierarchy/context, semantic foreground styles, safe areas, and accessibility fallbacks.",
            "Mobile UX Design drives user-goal framing, thumb-zone reachability, touch-target sizing, interruption/resume checks, navigation convention review, and performance-as-UX budgets.",
            "The Liquid Glass reference from Mobile UX Design adds explicit contrast, Reduce Transparency, Increase Contrast, Dynamic Type, and oldest-device performance checks.",
            "The generated page front-loads launch readiness, risk class, evidence quality, accessibility status, regression coverage, and dependency posture before detailed scores.",
            "The report separates individual dimensions from whole-product cluster judgment so an isolated pass cannot hide a weak or inconsistent product flow.",
        ],
    }


def quality_review_for(scenario: Scenario) -> dict[str, Any]:
    return {
        key: {
            "status": "incomplete",
            "summary": f"{description} Current scaffold has no run evidence for {scenario.scenario_id}.",
            "checks": checks_for_quality_area(key),
            "evidence_refs": ["rubric:review-skill-grounding"],
            "gaps": [f"Attach {key.replace('_', ' ')} evidence before scoring this area above 2."],
        }
        for key, description in QUALITY_AREAS.items()
    }


def coherence_for(scenario: Scenario, scenarios: list[Scenario]) -> dict[str, Any]:
    related = related_for(scenario, scenarios, limit=8)
    return {
        "cluster": {"id": scenario.category_slug, "title": scenario.category, "scenario_ids": related},
        "individual_judgment": {
            "status": "incomplete",
            "summary": f"{scenario.scenario_id} has not been judged in isolation because observed product behavior is missing.",
            "checks": ["scenario-level UI polish", "scenario-level UX polish", "scenario-level performance/accessibility evidence", "scenario-level product fit"],
            "evidence_refs": [source_ref(scenario)],
            "gaps": ["Attach observed behavior before making an individual product-coherence judgment."],
        },
        "group_judgment": {
            "status": "incomplete",
            "summary": f"Group-level coherence for {scenario.category} is unproven until adjacent scenarios are reviewed together.",
            "checks": ["related scenario consistency", "shared defect themes", "cluster readiness", "cross-flow state continuity"],
            "evidence_refs": [source_ref(scenario), "rubric:review-skill-grounding"],
            "gaps": ["Review adjacent scenario pages as a cluster before promoting group coherence."],
        },
        "themes": [
            "Does the flow preserve the same product mental model as adjacent scenarios?",
            "Do UI, UX, accessibility, performance, and NMP architecture scores tell the same story?",
            "Do related pages expose shared defects or readiness blockers instead of isolated one-off notes?",
        ],
        "cross_scenario_risks": [
            f"Changes to {scenario.scenario_id} may affect {', '.join(related[:4]) or 'nearby scenarios'} and must be revalidated as a cluster.",
        ],
    }


def readiness_for(scenario: Scenario) -> dict[str, Any]:
    return {
        "ship_gate": "incomplete",
        "blocking_reasons": ["No current execution evidence is attached.", "Product-quality judgments are scaffolded, not observed."],
        "gates": [
            {
                "id": gate_id,
                "status": "blocked",
                "requirement": requirement,
                "owner": "validation-agent",
                "evidence_refs": [source_ref(scenario)] if gate_id == "required_evidence" else ["rubric:review-skill-grounding"],
            }
            for gate_id, requirement in READINESS_GATES.items()
        ],
    }


def launch_assessment_for(
    scenario: Scenario,
    evidence_inventory: dict[str, Any],
    coherence: dict[str, Any],
    readiness: dict[str, Any],
    risks: list[dict[str, Any]],
) -> dict[str, Any]:
    missing = [item["kind"] for item in evidence_inventory["missing"]]
    blocked_gates = [gate["id"] for gate in readiness["gates"] if gate["status"] in {"blocked", "incomplete"}]
    dependency_posture = "blocked" if scenario.provider_mode == "blocked" or scenario.cassettes else "not_run"
    return {
        "launch_readiness": readiness["ship_gate"],
        "risk_classification": highest_risk(risks),
        "evidence_quality": "incomplete" if missing else "pass",
        "accessibility_status": "incomplete",
        "regression_coverage": "incomplete",
        "dependency_posture": dependency_posture,
        "scenario_owner": "validation-agent",
        "scenario_status": "not_run",
        "product_judgment": "Not launch-ready. The page has a complete review structure, but product judgment remains blocked until current evidence supports individual and whole-product coherence.",
        "individual_judgment": coherence["individual_judgment"]["summary"],
        "whole_product_judgment": coherence["group_judgment"]["summary"],
        "blocking_gates": blocked_gates,
        "issue_refs": [],
        "missing_evidence": missing,
    }


def instrumentation_gaps_for(scenario: Scenario) -> list[dict[str, Any]]:
    gaps = [
        gap("screenshots", "major", "No step-by-step screenshots are attached.", ["artifacts", "ui_polish", "actual_result"]),
        gap("ui-tree", "major", "No accessibility/UI tree snapshot is attached.", ["accessibility_dynamic_type", "actual_result"]),
        gap("logs", "major", "No structured logs or command outputs are attached.", ["observability", "reliability_flakiness"]),
        gap("accessibility", "major", "No VoiceOver, Dynamic Type, contrast, Reduce Motion, or Reduce Transparency evidence is attached.", ["accessibility_dynamic_type", "touch_ergonomics"]),
        gap("state-integrity", "major", "No persistence, projection, or data-integrity evidence is attached.", ["data_integrity_state_sync", "actual_result"]),
        gap("viewport-coverage", "minor", "No generated-page viewport or device coverage evidence is attached.", ["device_viewport_coverage", "device_os_matrix"]),
    ]
    if scenario.performance_required:
        gaps.append(gap("performance-metrics", "major", "Catalog requires performance evidence, but no trace or metric is attached.", ["performance", "reliability_flakiness"]))
    if scenario.cassettes or scenario.provider_mode == "blocked":
        gaps.append(gap("cassettes", "major", "Provider, relay, network, STT, TTS, or LLM replay data is missing or blocked.", ["replayability_cassette_provenance", "privacy_security"]))
    return gaps


def risks_for(scenario: Scenario, scenarios: list[Scenario]) -> list[dict[str, Any]]:
    related = related_for(scenario, scenarios, limit=4)
    risks = [
        {
            "id": "risk-missing-evidence",
            "severity": "major",
            "priority": "p0",
            "status": "open",
            "title": "Scenario could be misread as validated without current evidence.",
            "affected_dimensions": ["artifacts", "evidence_confidence", "verdict"],
            "mitigation": "Keep verdict incomplete until run artifacts, scores, and issue links are attached.",
        },
        {
            "id": "risk-cluster-regression",
            "severity": "major",
            "priority": "p1",
            "status": "open",
            "title": f"Fixes for this flow may regress related scenarios: {', '.join(related) or 'none declared'}.",
            "affected_dimensions": ["product_cluster_coherence", "regression_risk"],
            "mitigation": "Revalidate the scenario cluster before promoting group coherence.",
        },
    ]
    if scenario.performance_required:
        risks.append({"id": "risk-performance-budget", "severity": "major", "priority": "p1", "status": "open", "title": "Performance budget is declared but unmeasured.", "affected_dimensions": ["performance"], "mitigation": "Attach metric trace with budget, value, unit, method, and status."})
    return risks


def highest_risk(risks: list[dict[str, Any]]) -> str:
    order = {"blocker": 4, "major": 3, "minor": 2, "polish": 1}
    return max((risk["severity"] for risk in risks), key=lambda value: order.get(value, 0), default="none")


def required_evidence_kinds(scenario: Scenario) -> list[str]:
    kinds = {"source_doc", "screenshot", "ui_tree", "log", "accessibility_audit", "command_output"}
    if scenario.performance_required:
        kinds.add("metric_trace")
    if scenario.cassettes or scenario.provider_mode == "blocked":
        kinds.add("cassette")
    return sorted(kinds)


def related_for(scenario: Scenario, scenarios: list[Scenario], limit: int) -> list[str]:
    same = [item.scenario_id for item in scenarios if item.category_slug == scenario.category_slug and item.scenario_id != scenario.scenario_id]
    return same[:limit]


def source_ref(scenario: Scenario) -> str:
    return f"catalog:{scenario.scenario_id.lower()}"


def step(step_id: str, phase: str, action: str, expected: str, scenario: Scenario, evidence: list[str]) -> dict[str, Any]:
    return {"id": step_id, "phase": phase, "action": action, "expected": expected, "status": "not_run", "evidence_required": evidence, "evidence_refs": [source_ref(scenario)]}


def branch_records_for(scenario: Scenario) -> list[dict[str, Any]]:
    branches = [{"id": "happy-path", "status": "not_run", "trigger": scenario.bdd["when"][0], "expected": scenario.bdd["then"][0]}]
    text = " ".join([scenario.sentence, scenario.evidence.get("deps", "")]).lower()
    for token, branch_id in [("offline", "offline-path"), ("invalid", "invalid-input-path"), ("timeout", "timeout-path"), ("error", "error-path"), ("relaunch", "resume-path")]:
        if token in text:
            branches.append({"id": branch_id, "status": "not_run", "trigger": token, "expected": "Recover without crash, stale state, privacy leak, or permanent spinner."})
    return branches


def checks_for_quality_area(key: str) -> list[str]:
    checks = {
        "ui": ["SF/system typography", "semantic color", "visual hierarchy", "no overlapping text", "platform-native controls"],
        "ux": ["clear user goal", "low cognitive load", "visible feedback", "recoverable errors", "predictable navigation"],
        "performance": ["launch latency", "tap latency", "screen settle", "scroll hitches", "provider/audio/network latency"],
        "accessibility": ["VoiceOver labels", "Dynamic Type", "4.5:1 contrast", "44 pt targets", "Reduce Motion/Transparency"],
        "reliability": ["rerun count", "retry cause", "flake history", "fresh build", "deterministic replay"],
        "privacy_security": ["no secrets in screenshots/logs", "permission clarity", "redaction hashes", "private Nostr material safe"],
        "content_localization": ["copy clarity", "long text", "dates/numbers/locale", "empty/error copy", "transcript readability"],
        "controls_gestures": ["touch reach", "gesture alternative", "audio controls", "haptic expectation", "sheet/menu behavior"],
        "offline_resume": ["offline messaging", "resume after relaunch", "background playback", "download availability", "cancel/retry"],
        "observability": ["metric IDs", "request IDs", "relay IDs", "redacted traces", "issue reproduction context"],
    }
    return checks[key]


def persona_for(scenario: Scenario) -> str:
    category = scenario.category.lower()
    if "playback" in category:
        return "A returning listener trying to continue or control audio quickly."
    if "agent" in category or "transcript" in category:
        return "A power listener using transcripts, highlights, and AI help without losing trust in sources."
    if "identity" in category or "nostr" in category:
        return "A privacy-conscious listener managing identity, relays, and publishing state."
    return "A new or returning podcast listener completing a core Pod0 job on iPhone."


def job_for(scenario: Scenario) -> str:
    return f"Complete {scenario.scenario_id} with confidence: {scenario.bdd['then'][0]}."


def user_value_for(scenario: Scenario) -> str:
    return f"The flow should make {scenario.category.lower()} feel reliable, understandable, and native to Pod0 rather than a test-only path."


def missing_reason_for(kind: str, scenario: Scenario) -> str:
    reasons = {
        "screenshot": "Every meaningful user-visible step needs a screenshot.",
        "ui_tree": "Accessibility labels, hierarchy, and Dynamic Type behavior need UI-tree evidence.",
        "log": "Failures and state transitions need diagnostic context.",
        "accessibility_audit": "Apple HIG evidence is required before accessibility can pass.",
        "command_output": "Attempt commands or manual/tool notes are required for reproducibility.",
        "metric_trace": f"Catalog performance requirement is unmeasured: {scenario.evidence.get('perf', 'metric required')}.",
        "cassette": "Provider, relay, network, STT, TTS, or LLM behavior needs replay provenance or live-only rationale.",
    }
    return reasons.get(kind, "Required evidence is missing.")


def blocked_dimensions_for(kind: str) -> list[str]:
    return {
        "screenshot": ["actual_result", "ui_polish", "content_hierarchy"],
        "ui_tree": ["accessibility_dynamic_type", "information_architecture"],
        "log": ["observability", "reliability_flakiness"],
        "accessibility_audit": ["accessibility_dynamic_type", "touch_ergonomics"],
        "command_output": ["attempted_test", "execution_attempts"],
        "metric_trace": ["performance"],
        "cassette": ["replayability_cassette_provenance", "privacy_security"],
    }.get(kind, ["evidence_confidence"])


def gap(gap_id: str, severity: str, summary: str, dimensions: list[str]) -> dict[str, Any]:
    return {"id": gap_id, "severity": severity, "summary": summary, "affected_dimensions": dimensions, "owner": "validation-agent", "status": "open"}
