from __future__ import annotations

import json
from pathlib import Path

from catalog import parse_catalog
from records import build_report


GENERATED_AT = "2026-07-05T00:00:00Z"
SITE_BASE = "https://example.test/podcast-player/"


def write_catalog(catalog: Path) -> Path:
    catalog.mkdir(parents=True)
    (catalog / "INDEX.md").write_text("| Group | Count |\n|---|---:|\n| **Total** | **2** |\n")
    (catalog / "01-smoke.md").write_text(
        "\n".join(
            [
                "# Smoke Catalog",
                "",
                "## Smoke And Report Shape",
                "",
                "| ID | Scenario | Evidence |",
                "|---|---|---|",
                "| SMOKE-001 | Given a seeded library, when the listener opens Library, then subscribed shows render with stable navigation. | ss: library screenshot; perf: screen settle metric; deps: seeded library plus OpenRouter provider cassette; boundary: D1,D5. |",
                "| SMOKE-002 | Given offline mode, when the listener opens a cached show, then cached episodes remain available. | ss: offline screenshot; perf: none; deps: offline fixture; boundary: D5. |",
            ]
        )
        + "\n"
    )
    return catalog


def build_records(catalog: Path, repo: Path) -> list[dict[str, object]]:
    scenarios = parse_catalog(catalog)
    return [build_report(scenario, scenarios, repo, GENERATED_AT, SITE_BASE) for scenario in scenarios]


def write_provider_cassette(root: Path, refs: list[str]) -> None:
    cassette_dir = root / "tests" / "fixtures" / "provider_cassettes"
    cassette_dir.mkdir(parents=True, exist_ok=True)
    (cassette_dir / "smoke-provider.json").write_text(
        json.dumps(
            {
                "id": "smoke-provider",
                "provider": "openrouter",
                "operation": "chat_completion",
                "scenario_refs": refs,
                "nmp_rules": ["D7"],
                "metrics": {
                    "recorded_latency_ms": 100,
                    "replay_latency_ms": 2,
                    "budget_ms": 500,
                    "acceptable_for_2026_premium": True,
                },
            }
        )
    )
