from __future__ import annotations

import html
from typing import Any

from catalog import slugify


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


def nav_bar(links: list[tuple[str, str]]) -> str:
    return "<nav aria-label=\"Scenario navigation\">" + "".join(f"<a href=\"{href}\">{e(label)}</a>" for label, href in links if href) + "</nav>"


def key_values(values: dict[str, str]) -> str:
    return "<dl>" + "".join(f"<dt>{e(k)}</dt><dd>{e(str(v))}</dd>" for k, v in values.items()) + "</dl>"


def score_table(scores: dict[str, dict[str, Any]]) -> str:
    rows = "".join(f"<tr><td>{e(name)}</td><td>{e(str(item['score']))}</td><td>{badge(item['status'])}</td><td>{e(item['rationale'])}</td><td>{e(', '.join(item.get('evidence_refs', item.get('dimension_refs', []))) or 'none')}</td></tr>" for name, item in scores.items())
    return f"<table><caption>Scores and score-gate rationale</caption><thead><tr><th>Dimension</th><th>Score</th><th>Status</th><th>Rationale</th><th>Refs</th></tr></thead><tbody>{rows}</tbody></table>"


def artifact_table(artifacts: list[dict[str, Any]], depth: int) -> str:
    rows = "".join(f"<tr><td>{e(a['id'])}</td><td>{e(a['type'])}</td><td><a href=\"{rel(a['path'], depth)}\">{e(a['path'])}</a></td><td>{e(a['description'])}</td><td>{e(str(a.get('required', False)))}</td></tr>" for a in artifacts)
    return f"<table><caption>Published artifact registry</caption><thead><tr><th>ID</th><th>Type</th><th>Path</th><th>Description</th><th>Required</th></tr></thead><tbody>{rows}</tbody></table>"


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
            caption_bits.append(" / ".join(part for part in [item.get("device", ""), item.get("os_version", "")] if part))
        figures.append("<figure>" + f"<a href=\"{rel(item['path'], depth)}\"><img src=\"{rel(item['path'], depth)}\" alt=\"{e(item.get('alt', item['description']))}\"{media_size_attrs(item)} loading=\"eager\" decoding=\"async\"></a>" + f"<figcaption>{e(' | '.join(caption_bits))}</figcaption></figure>")
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


def artifact_count_for_kind(artifacts: list[dict[str, Any]], kind: str) -> int:
    aliases = {"metric_trace": {"metric", "metric_trace"}, "accessibility_audit": {"accessibility_audit", "ui_tree"}, "command_output": {"command_output", "log"}}
    accepted = aliases.get(kind, {kind})
    return sum(1 for artifact in artifacts if artifact.get("type") in accepted)


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
