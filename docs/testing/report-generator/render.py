from __future__ import annotations

import html
from typing import Any

from catalog import slugify
from contract import SECTION_LABELS
from records import count_boundaries, count_by, count_tags, missing_evidence_for, rollups_for


def render_home(records: list[dict[str, Any]], depth: int) -> str:
    rollups = rollups_for(records)
    observed = sum(1 for record in records if record["execution"]["status"] != "not_run")
    body = [
        hero(
            "Pod0 Scenario Validation Report",
            f"{len(records)} generated scenario pages; {observed} now include run evidence. Overall suite readiness remains incomplete until every scenario is evidence-backed and cluster-reviewed.",
        ),
        stat_band(rollups["by_verdict"]),
        section("Scenario Page System", p("Each BDD catalog scenario now has a stable page, JSON record, source link, structured flow steps, attempts, evidence inventory, skill-grounded quality review, product-cluster coherence, readiness gates, and rollup membership.")),
        section("Evidence-Backed Scenarios", evidence_backed_summary(records, depth)),
        link_list("Indexes", [("All scenarios", rel("scenarios/", depth)), ("Tags", rel("tags/", depth)), ("Issues", rel("issues/", depth)), ("Provider cassette replay", rel("provider-cassettes/", depth)), ("Performance rollup", rel("rollups/performance/", depth)), ("Raw scenario JSON", rel("data/scenarios.json", depth))]),
    ]
    return page("Pod0 Validation Report", depth, "\n".join(body))


def render_scenario_index(records: list[dict[str, Any]], depth: int, title: str) -> str:
    rows = "\n".join(
        f"<tr><td><a href=\"{rel('scenarios/' + r['scenario']['slug'] + '/', depth)}\">{e(r['scenario']['id'])}</a></td><td>{e(r['scenario']['title'])}</td><td>{e(r['scenario']['category'])}</td><td>{badge(r['verdict']['overall'])}</td><td>{badge(r['readiness']['ship_gate'])}</td><td>{e(r['coherence']['group_judgment']['status'])}</td><td>{e(', '.join(missing_evidence_for(r)))}</td></tr>"
        for r in records
    )
    table = f"<table><caption>{e(title)} scenario records</caption><thead><tr><th>ID</th><th>Scenario</th><th>Category</th><th>Verdict</th><th>Readiness</th><th>Group Coherence</th><th>Missing Evidence</th></tr></thead><tbody>{rows}</tbody></table>"
    return page(title, depth, hero(title, f"{len(records)} scenario page records") + section("Scenario Table", table))


def render_scenario_page(record: dict[str, Any], depth: int) -> str:
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
        section("Preconditions, Fixtures, Cassettes, And Runtime Metadata", key_values(metadata_for(record)) + device_table(record["run"]["device_matrix"]) + cassette_table(record["run"].get("cassettes", []))),
        section("Execution Attempts, Retries, And Branches", attempts_block(record["execution"])),
        section("Results And Verdict", p(record["verdict"]["summary"]) + key_values({"Overall": record["verdict"]["overall"], "Gate explanation": record["verdict"]["score_gate_explanation"]})),
        section("Screenshot Evidence", screenshot_gallery(record["evidence"]["artifacts"], depth) + evidence_placeholder_grid(record["evidence"], {"screenshot"})),
        section("Evidence Inventory", missing_evidence_table(record["evidence"]) + evidence_placeholder_grid(record["evidence"], None) + artifact_table(record["evidence"]["artifacts"], depth)),
        section("Quality Review", quality_table(record["quality_review"])),
        section("UI Polish Report", review_area_block(record, "ui", "ui_polish_report")),
        section("UX Polish Report", review_area_block(record, "ux", "ux_polish_report")),
        section("Performance Metrics And Interaction Latency", metrics_block(record)),
        section("Navigation, Orientation, And Information Architecture", navigation_orientation_block(record)),
        section("Animation, Transition, And Haptics Quality", motion_block(record)),
        section("Product Flow Cohesiveness And Group Coherent-Product Judgment", coherence_block(record["coherence"])),
        section("Product-Level Assessment", product_assessment_block(record)),
        section("Readiness Gates", readiness_block(record["readiness"])),
        section("Instrumentation Gaps And Missing Evidence", gap_table(record["instrumentation_gaps"])),
        section("Risk Severity And Validation Confidence", risk_confidence_block(record)),
        section("Risks, Defects, Issue Links, And Follow-Up", risk_table(record["risks"]) + issue_table(record["issues"]) + action_list(record["next_actions"])),
        section("Grouped Scores And Coherent Product Judgment", score_table(record["group_scores"])),
        section("Individual Dimension Judgments", score_table(record["dimension_scores"])),
        section("Required Detailed Sections", "\n".join(render_required_section(key, value) for key, value in record["sections"].items())),
    ]
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
    return key_values({"Persona": context["persona"], "Job": context["job_to_be_done"], "User value": context["user_value"], "Cluster": f"{cluster['title']} ({cluster['id']})", "Related scenarios": ", ".join(cluster["related_scenarios"])}) + f"<h3>Acceptance Criteria</h3><ul>{criteria}</ul><h3>Platform Expectations</h3><ul>{expectations}</ul>"


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


def flow_step_table(steps: list[dict[str, Any]]) -> str:
    rows = "".join(f"<tr><td>{e(step['id'])}</td><td>{e(step['phase'])}</td><td>{e(step['action'])}</td><td>{e(step['expected'])}</td><td>{badge(step['status'])}</td><td>{e(', '.join(step['evidence_required']))}</td></tr>" for step in steps)
    return f"<table><caption>Step-by-step flow</caption><thead><tr><th>Step</th><th>Phase</th><th>Action</th><th>Expected</th><th>Status</th><th>Evidence Needed</th></tr></thead><tbody>{rows}</tbody></table>"


def device_table(devices: list[dict[str, Any]]) -> str:
    rows = "".join(f"<tr><td>{e(item['name'])}</td><td>{e(item['os_version'])}</td><td>{e(item['form_factor'])}</td><td>{e(item.get('udid', ''))}</td></tr>" for item in devices)
    return f"<table><caption>Device, simulator, and runtime matrix</caption><thead><tr><th>Name</th><th>OS</th><th>Form Factor</th><th>UDID</th></tr></thead><tbody>{rows}</tbody></table>"


def cassette_table(cassettes: list[dict[str, Any]]) -> str:
    if not cassettes:
        return p("No cassettes declared for this scaffold.")
    rows = "".join(f"<tr><td>{e(item['id'])}</td><td>{e(item['provider'])}</td><td>{badge(item['mode'])}</td><td>{e(item['redaction_hash'])}</td></tr>" for item in cassettes)
    return f"<table><caption>Provider, relay, replay, and cassette data</caption><thead><tr><th>ID</th><th>Provider</th><th>Mode</th><th>Redaction Hash</th></tr></thead><tbody>{rows}</tbody></table>"


def attempts_block(execution: dict[str, Any]) -> str:
    rows = "".join(f"<tr><td>{e(item['id'])}</td><td>{badge(item['status'])}</td><td>{e(item['executor'])}</td><td>{e(', '.join(item['tools']))}</td><td>{e('; '.join(item['commands']) or 'not run')}</td><td>{e(item['notes'])}</td></tr>" for item in execution["attempts"])
    branches = "".join(f"<li><strong>{e(item['id'])}</strong>: {e(item['trigger'])} -> {e(item['expected'])} ({e(item['status'])})</li>" for item in execution["branch_coverage"])
    return f"<p>{e(execution['retry_policy'])}</p><table><caption>Execution attempts</caption><thead><tr><th>Attempt</th><th>Status</th><th>Executor</th><th>Tools</th><th>Commands</th><th>Notes</th></tr></thead><tbody>{rows}</tbody></table><h3>Branches</h3><ul>{branches}</ul>"


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


def quality_table(quality: dict[str, Any]) -> str:
    rows = "".join(f"<tr><td>{e(name)}</td><td>{badge(item['status'])}</td><td>{e(item['summary'])}</td><td>{e(', '.join(item['checks']))}</td><td>{e(', '.join(item['gaps']))}</td></tr>" for name, item in quality.items())
    return f"<table><caption>UI, UX, accessibility, performance, reliability, content, and observability review</caption><thead><tr><th>Area</th><th>Status</th><th>Summary</th><th>Checks</th><th>Gaps</th></tr></thead><tbody>{rows}</tbody></table>"


def review_area_block(record: dict[str, Any], area_key: str, section_key: str) -> str:
    area = record["quality_review"][area_key]
    detail = record["sections"][section_key]
    dimension_key = {"ui_polish_report": "ui_polish", "ux_polish_report": "ux_polish"}[section_key]
    return key_values(
        {
            "Status": area["status"],
            "Score": str(record["dimension_scores"][dimension_key]["score"]),
            "Summary": detail["summary"],
            "Evidence refs": ", ".join(detail.get("evidence_refs", [])) or "none",
        }
    ) + list_block("Checks", area["checks"]) + list_block("Gaps", area["gaps"])


def metrics_block(record: dict[str, Any]) -> str:
    rows = "".join(
        f"<tr><td>{e(item['name'])}</td><td>{e(str(item['value']))} {e(item['unit'])}</td><td>{e(item['budget'])}</td><td>{badge(item['status'])}</td><td>{e(item['method'])}</td></tr>"
        for item in record["metrics"]
    )
    metrics = "<p>No metric traces are attached yet.</p>" if not rows else f"<table><caption>Measured performance</caption><thead><tr><th>Metric</th><th>Value</th><th>Budget</th><th>Status</th><th>Method</th></tr></thead><tbody>{rows}</tbody></table>"
    return metrics + p(record["sections"]["performance_metrics"]["summary"]) + list_block("Required latency checks", record["quality_review"]["performance"]["checks"])


def navigation_orientation_block(record: dict[str, Any]) -> str:
    sections = record["sections"]
    return p(sections["information_architecture"]["summary"]) + p(sections["cross_screen_continuity"]["summary"]) + p(sections["content_hierarchy"]["summary"]) + list_block("Evidence refs", record["dimension_scores"]["information_architecture"]["evidence_refs"])


def motion_block(record: dict[str, Any]) -> str:
    return p(record["sections"]["motion_haptics"]["summary"]) + p(record["sections"]["controls_gestures_audio"]["summary"]) + list_block("Mobile interaction checks", record["quality_review"]["controls_gestures"]["checks"])


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


def coherence_block(coherence: dict[str, Any]) -> str:
    cluster = coherence["cluster"]
    themes = "".join(f"<li>{e(item)}</li>" for item in coherence["themes"])
    risks = "".join(f"<li>{e(item)}</li>" for item in coherence["cross_scenario_risks"])
    return key_values({"Cluster": f"{cluster['title']} ({cluster['id']})", "Related scenarios": ", ".join(cluster["scenario_ids"]), "Individual judgment": coherence["individual_judgment"]["summary"], "Group judgment": coherence["group_judgment"]["summary"]}) + f"<h3>Cross-Scenario Themes</h3><ul>{themes}</ul><h3>Cross-Scenario Risks</h3><ul>{risks}</ul>"


def readiness_block(readiness: dict[str, Any]) -> str:
    rows = "".join(f"<tr><td>{e(item['id'])}</td><td>{badge(item['status'])}</td><td>{e(item['requirement'])}</td><td>{e(item['owner'])}</td></tr>" for item in readiness["gates"])
    blockers = "".join(f"<li>{e(item)}</li>" for item in readiness["blocking_reasons"])
    return f"<p><strong>Ship gate:</strong> {badge(readiness['ship_gate'])}</p><ul>{blockers}</ul><table><caption>Readiness gates</caption><thead><tr><th>Gate</th><th>Status</th><th>Requirement</th><th>Owner</th></tr></thead><tbody>{rows}</tbody></table>"


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


def render_required_section(key: str, value: dict[str, Any]) -> str:
    notes = "".join(f"<li>{e(note)}</li>" for note in value.get("notes", []))
    refs = ", ".join(value.get("evidence_refs", [])) or "none"
    note_html = f"<ul>{notes}</ul>" if notes else ""
    return f"<article class=\"report-section\"><h3>{e(SECTION_LABELS[key])}</h3><p>{e(value['summary'])}</p><p class=\"muted\">Evidence refs: {e(refs)}</p>{note_html}</article>"


def write_tag_pages(records: list[dict[str, Any]], out: Any) -> dict[Any, str]:
    tags = count_tags(records)
    pages = {out / "tags" / "index.html": page("Tags", 1, hero("Tags", "Scenario filters by generated tag") + link_list("Tags", [(f"{tag} ({count})", f"{tag}/") for tag, count in tags.items()]))}
    for tag in tags:
        tagged = [record for record in records if tag in record["scenario"]["tags"]]
        pages[out / "tags" / tag / "index.html"] = render_scenario_index(tagged, 2, f"Tag: {tag}")
    return pages


def write_rollup_pages(records: list[dict[str, Any]], out: Any) -> dict[Any, str]:
    pages = {
        out / "issues" / "index.html": page("Issues", 1, issue_rollup_page(records))
    }
    for verdict in count_by(records, lambda r: r["verdict"]["overall"]):
        filtered = [r for r in records if r["verdict"]["overall"] == verdict]
        pages[out / "rollups" / "verdict" / verdict / "index.html"] = render_scenario_index(filtered, 3, f"Verdict: {verdict}")
    for provider in count_by(records, lambda r: r["run"]["provider_mode"]):
        filtered = [r for r in records if r["run"]["provider_mode"] == provider]
        pages[out / "rollups" / "provider" / provider / "index.html"] = render_scenario_index(filtered, 3, f"Provider mode: {provider}")
    for boundary in count_boundaries(records):
        filtered = [r for r in records if boundary in r["scenario"]["tags"]]
        pages[out / "rollups" / "nmp" / boundary / "index.html"] = render_scenario_index(filtered, 3, f"NMP boundary: {boundary}")
    perf = [r for r in records if "performance-required" in r["scenario"]["tags"]]
    pages[out / "rollups" / "performance" / "index.html"] = render_scenario_index(perf, 2, "Performance Metrics Required")
    return pages


def issue_rollup_page(records: list[dict[str, Any]]) -> str:
    issues = [
        (record["scenario"]["id"], record["scenario"]["slug"], issue)
        for record in records
        for issue in record["issues"]
    ]
    if not issues:
        return hero("Issues", "No observed defects have been linked yet.") + p("Scenario pages remain incomplete until defects discovered during validation are linked here.")
    rows = "".join(
        f"<tr><td><a href=\"../scenarios/{e(slug)}/\">{e(scenario_id)}</a></td><td><a href=\"{e(issue['url'])}\">{e(issue['id'])}</a></td><td>{badge(issue['severity'])}</td><td>{e(issue['title'])}</td><td>{badge(issue['status'])}</td></tr>"
        for scenario_id, slug, issue in issues
    )
    table = f"<table><caption>Linked scenario defects</caption><thead><tr><th>Scenario</th><th>Issue</th><th>Severity</th><th>Title</th><th>Status</th></tr></thead><tbody>{rows}</tbody></table>"
    return hero("Issues", f"{len(issues)} issue links from scenario records.") + section("Linked Defects And Validation Blockers", table)


def page(title: str, depth: int, body: str) -> str:
    return f"""<!doctype html>
<html lang=\"en\">
<head>
  <meta charset=\"utf-8\">
  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">
  <title>{e(title)}</title>
  <link rel=\"stylesheet\" href=\"{rel('styles.css', depth)}\">
  <link rel=\"icon\" href=\"{rel('assets/favicon.svg', depth)}\" type=\"image/svg+xml\">
</head>
<body>
  <a class=\"skip-link\" href=\"#content\">Skip to content</a>
  <main id=\"content\">
    {body}
  </main>
</body>
</html>
"""


def hero(title: str, subtitle: str) -> str:
    return f"<header class=\"hero\"><p class=\"eyebrow\">Pod0 Validation</p><h1>{e(title)}</h1><p>{subtitle}</p></header>"


def section(title: str, body: str) -> str:
    return f"<section id=\"{slugify(title)}\"><h2>{e(title)}</h2>{body}</section>"


def p(text: str) -> str:
    return f"<p>{e(text)}</p>"


def link_list(title: str, links: list[tuple[str, str]]) -> str:
    items = "".join(f"<li><a href=\"{href}\">{e(label)}</a></li>" for label, href in links)
    return section(title, f"<ul class=\"link-list\">{items}</ul>")


def nav_bar(links: list[tuple[str, str]]) -> str:
    return "<nav aria-label=\"Scenario navigation\">" + "".join(f"<a href=\"{href}\">{e(label)}</a>" for label, href in links if href) + "</nav>"


def stat_band(counts: dict[str, int]) -> str:
    stats = "".join(f"<div><strong>{count}</strong><span>{e(name)}</span></div>" for name, count in counts.items())
    return f"<section class=\"stats\">{stats}</section>"


def evidence_backed_summary(records: list[dict[str, Any]], depth: int) -> str:
    backed = [record for record in records if screenshot_artifacts(record["evidence"]["artifacts"])]
    if not backed:
        return p("No scenario screenshots have been published yet.")
    items = []
    for record in backed:
        scenario = record["scenario"]
        shots = screenshot_artifacts(record["evidence"]["artifacts"])
        first = shots[0]
        items.append(
            "<article class=\"evidence-card\">"
            f"<a href=\"{rel('scenarios/' + scenario['slug'] + '/', depth)}\">"
            f"<img src=\"{rel(first['path'], depth)}\" alt=\"{e(first.get('alt', first['description']))}\"{media_size_attrs(first)} loading=\"eager\" decoding=\"async\">"
            f"<strong>{e(scenario['id'])}</strong>"
            f"<span>{e(record['verdict']['overall'])} · {len(shots)} screenshot{'s' if len(shots) != 1 else ''}</span>"
            "</a>"
            "</article>"
        )
    return "<div class=\"evidence-grid\">" + "".join(items) + "</div>"


def key_values(values: dict[str, str]) -> str:
    return "<dl>" + "".join(f"<dt>{e(k)}</dt><dd>{e(str(v))}</dd>" for k, v in values.items()) + "</dl>"


def score_table(scores: dict[str, dict[str, Any]]) -> str:
    rows = "".join(f"<tr><td>{e(name)}</td><td>{e(str(item['score']))}</td><td>{badge(item['status'])}</td><td>{e(item['rationale'])}</td></tr>" for name, item in scores.items())
    return f"<table><caption>Scores and score-gate rationale</caption><thead><tr><th>Dimension</th><th>Score</th><th>Status</th><th>Rationale</th></tr></thead><tbody>{rows}</tbody></table>"


def artifact_table(artifacts: list[dict[str, Any]], depth: int) -> str:
    rows = "".join(f"<tr><td>{e(a['id'])}</td><td>{e(a['type'])}</td><td><a href=\"{rel(a['path'], depth)}\">{e(a['path'])}</a></td><td>{e(a['description'])}</td></tr>" for a in artifacts)
    return f"<table><caption>Published artifact registry</caption><thead><tr><th>ID</th><th>Type</th><th>Path</th><th>Description</th></tr></thead><tbody>{rows}</tbody></table>"


def screenshot_gallery(artifacts: list[dict[str, Any]], depth: int) -> str:
    screenshots = screenshot_artifacts(artifacts)
    if not screenshots:
        return p("No screenshots are attached to this scenario yet.")
    figures = []
    for item in screenshots:
        caption_bits = [item.get("caption") or item["description"]]
        if item.get("step_id"):
            caption_bits.append(f"step: {item['step_id']}")
        if item.get("device") or item.get("os_version"):
            caption_bits.append(" · ".join(part for part in [item.get("device", ""), item.get("os_version", "")] if part))
        figures.append(
            "<figure>"
            f"<a href=\"{rel(item['path'], depth)}\"><img src=\"{rel(item['path'], depth)}\" alt=\"{e(item.get('alt', item['description']))}\"{media_size_attrs(item)} loading=\"eager\" decoding=\"async\"></a>"
            f"<figcaption>{e(' | '.join(caption_bits))}</figcaption>"
            "</figure>"
        )
    return "<div class=\"screenshot-gallery\">" + "".join(figures) + "</div>"


def screenshot_artifacts(artifacts: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [item for item in artifacts if item.get("type") == "screenshot"]


def media_size_attrs(item: dict[str, Any]) -> str:
    return f" width=\"{e(item.get('width', 368))}\" height=\"{e(item.get('height', 800))}\""


def action_list(actions: list[dict[str, str]]) -> str:
    items = "".join(f"<li><strong>{e(a['title'])}</strong><br><span class=\"muted\">{e(a.get('status', ''))} - {e(a.get('owner', ''))}</span></li>" for a in actions)
    return f"<ol>{items}</ol>"


def list_block(title: str, items: list[str]) -> str:
    body = "".join(f"<li>{e(item)}</li>" for item in items)
    return f"<h3>{e(title)}</h3><ul>{body}</ul>"


def highest_severity(risks: list[dict[str, Any]]) -> str:
    order = {"blocker": 4, "major": 3, "minor": 2, "polish": 1}
    return max((risk["severity"] for risk in risks), key=lambda value: order.get(value, 0), default="none")


def badge(value: str) -> str:
    return f"<span class=\"badge badge-{slugify(value)}\">{e(value)}</span>"


def rel(path: str, depth: int) -> str:
    if path.startswith(("http://", "https://", "../", "./")):
        return path
    return ("../" * depth) + path


def e(value: str) -> str:
    return html.escape(str(value), quote=True)
