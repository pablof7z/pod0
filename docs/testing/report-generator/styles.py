from __future__ import annotations


def stylesheet() -> str:
    return """
:root { color-scheme: light; --ink: #182026; --muted: #5d6872; --line: #d7dee5; --soft: #f5f7f9; --accent: #146c94; --warn: #a35400; --ok: #1f7a4d; --risk: #9d2727; }
* { box-sizing: border-box; }
body { margin: 0; font: 16px/1.5 -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif; color: var(--ink); background: #ffffff; }
main { max-width: 1180px; margin: 0 auto; padding: 28px 18px 64px; }
a { color: var(--accent); text-decoration-thickness: 1px; text-underline-offset: 3px; }
a:focus-visible { outline: 3px solid #5aa9d6; outline-offset: 3px; }
.skip-link { position: absolute; left: 12px; top: 8px; transform: translateY(-160%); background: #ffffff; border: 2px solid var(--accent); padding: 8px 10px; z-index: 2; }
.skip-link:focus { transform: translateY(0); }
nav { display: flex; flex-wrap: wrap; gap: 10px; margin-bottom: 18px; }
nav a, .badge { border: 1px solid var(--line); border-radius: 999px; padding: 4px 10px; text-decoration: none; }
.hero { border-bottom: 3px solid var(--ink); padding: 18px 0 22px; margin-bottom: 24px; }
.eyebrow { color: var(--accent); font-weight: 700; text-transform: uppercase; letter-spacing: 0; margin: 0 0 6px; }
h1 { font-size: clamp(2rem, 4vw, 4rem); line-height: 1; margin: 0 0 12px; letter-spacing: 0; }
h2 { font-size: 1.35rem; margin: 36px 0 12px; }
h3 { font-size: 1.05rem; margin: 0 0 8px; }
section { margin: 24px 0; }
table { width: 100%; border-collapse: collapse; border: 1px solid var(--line); margin: 12px 0; table-layout: fixed; }
caption { text-align: left; font-weight: 700; margin: 0 0 6px; }
th, td { text-align: left; vertical-align: top; border-bottom: 1px solid var(--line); padding: 9px 10px; }
td { overflow-wrap: anywhere; }
th { background: var(--soft); font-size: 0.9rem; }
dl { display: grid; grid-template-columns: minmax(120px, 220px) 1fr; gap: 8px 14px; }
dt { font-weight: 700; }
dd { margin: 0; }
.stats, .launch-grid, .placeholder-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; }
.stats div, .launch-card, .evidence-placeholder { border: 1px solid var(--line); padding: 14px; background: #ffffff; }
.stats strong, .launch-card strong, .evidence-placeholder strong { display: block; font-size: 1.35rem; }
.stats strong { font-size: 2rem; }
.stats span, .muted, .launch-card span, .evidence-placeholder span { color: var(--muted); }
.evidence-grid, .screenshot-gallery { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 14px; }
.evidence-card, figure { border: 1px solid var(--line); margin: 0; background: #ffffff; }
.evidence-card a { display: grid; gap: 8px; padding: 10px; color: inherit; text-decoration: none; }
.evidence-card img, figure img { display: block; width: 100%; aspect-ratio: 9 / 16; object-fit: contain; object-position: center; background: #eef2f5; border: 1px solid var(--line); padding: 8px; }
.evidence-card span, figcaption { color: var(--muted); font-size: 0.9rem; }
figcaption { padding: 8px 10px 10px; }
.report-section { border-top: 1px solid var(--line); padding: 16px 0; }
.badge-incomplete { color: var(--warn); border-color: #d98f32; background: #fff7eb; }
.badge-pass { color: var(--ok); border-color: #71b894; background: #effaf4; }
.badge-fail, .badge-blocker { color: var(--risk); border-color: #d78585; background: #fff0f0; }
.badge-major { color: #7d3f00; border-color: #cf9b5a; background: #fff7eb; }
.link-list { columns: 2 280px; }
@media (max-width: 720px) { main { padding: 18px 12px 48px; } table { display: block; overflow-x: auto; } dl { grid-template-columns: 1fr; } }
@media (prefers-reduced-motion: reduce) { * { scroll-behavior: auto; } }
"""
