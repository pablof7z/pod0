from __future__ import annotations

import html
from typing import Any

from catalog import slugify
from next_wave_render import render_next_wave_home
from render_html import media_size_attrs, screenshot_artifacts
from records import count_boundaries, count_by, count_tags, missing_evidence_for, rollups_for
from scenario_render import render_scenario_page


def render_home(records: list[dict[str, Any]], depth: int, next_wave: dict[str, Any] | None = None, architecture_scan: dict[str, Any] | None = None) -> str:
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
        link_list("Indexes", [("All scenarios", rel("scenarios/", depth)), ("Tags", rel("tags/", depth)), ("Issues", rel("issues/", depth)), ("Next execution wave", rel("next-wave/", depth)), ("Architecture scan", rel("architecture-scan/", depth)), ("Provider cassette replay", rel("provider-cassettes/", depth)), ("Performance rollup", rel("rollups/performance/", depth)), ("Raw scenario JSON", rel("data/scenarios.json", depth))]),
    ]
    if next_wave:
        body.insert(3, render_next_wave_home(records, next_wave, depth))
    if architecture_scan:
        body.insert(4, architecture_scan_home_block(architecture_scan, depth))
    return page("Pod0 Validation Report", depth, "\n".join(body))


def architecture_scan_home_block(data: dict[str, Any], depth: int) -> str:
    counts = data["counts"]
    cards = [
        ("Status", badge(data.get("status", "not_recorded")), "Current D0-D10 scan gate."),
        ("Hard errors", e(str(counts.get("hard_errors", 0))), "D3/D8 findings blocking architecture confidence."),
        ("Findings", e(str(counts.get("total", 0))), "Warnings and errors needing review."),
        ("Linked issues", e(str(len(data.get("linked_issues", [])))), "GitHub follow-up entries tied to this scan."),
    ]
    html_cards = "".join(
        f"<article class=\"next-wave-card\"><span>{e(label)}</span><strong>{value}</strong><p>{e(help_text)}</p></article>"
        for label, value, help_text in cards
    )
    return section(
        "NMP Architecture Scan",
        f"<div class=\"next-wave-grid\">{html_cards}</div>"
        + p("This is static triage, not proof of compliance. Hard errors keep the NMP/Chirp parity gate blocked until fixed and re-scanned.")
        + f"<p><a href=\"{rel('architecture-scan/', depth)}\">Open the architecture scan report</a> &middot; <a href=\"{rel('data/architecture-scan.json', depth)}\">Raw JSON</a></p>",
    )


def render_scenario_index(records: list[dict[str, Any]], depth: int, title: str) -> str:
    rows = "\n".join(
        f"<tr><td><a href=\"{rel('scenarios/' + r['scenario']['slug'] + '/', depth)}\">{e(r['scenario']['id'])}</a></td><td>{e(r['scenario']['title'])}</td><td>{e(r['scenario']['category'])}</td><td>{badge(r['verdict']['overall'])}</td><td>{badge(r['readiness']['ship_gate'])}</td><td>{e(r['coherence']['group_judgment']['status'])}</td><td>{e(', '.join(missing_evidence_for(r)))}</td></tr>"
        for r in records
    )
    table = f"<table><caption>{e(title)} scenario records</caption><thead><tr><th>ID</th><th>Scenario</th><th>Category</th><th>Verdict</th><th>Readiness</th><th>Group Coherence</th><th>Missing Evidence</th></tr></thead><tbody>{rows}</tbody></table>"
    return page(title, depth, hero(title, f"{len(records)} scenario page records") + section("Scenario Table", table))


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


def badge(value: str) -> str:
    return f"<span class=\"badge badge-{slugify(value)}\">{e(value)}</span>"


def rel(path: str, depth: int) -> str:
    if path.startswith(("http://", "https://", "../", "./")):
        return path
    return ("../" * depth) + path


def e(value: str) -> str:
    return html.escape(str(value), quote=True)
