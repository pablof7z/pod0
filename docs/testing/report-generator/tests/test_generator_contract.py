from __future__ import annotations

import copy
import json
import sys
import tempfile
import unittest
from pathlib import Path

GENERATOR_DIR = Path(__file__).resolve().parents[1]
DOCS_TESTING_DIR = GENERATOR_DIR.parent
sys.path.insert(0, str(GENERATOR_DIR))

from catalog import parse_catalog  # noqa: E402
from contract import SKILL_GROUNDING  # noqa: E402
from generate_scenario_report import write_site  # noqa: E402
from provider_cassettes_report import provider_cassette_data, render_provider_cassette_page  # noqa: E402
from records import build_report, validate_schema_contract  # noqa: E402


GENERATED_AT = "2026-07-05T00:00:00Z"
SITE_BASE = "https://example.test/podcast-player/"

REQUIRED_SCENARIO_SECTIONS = [
    "What Was Attempted And Test Intent",
    "Launch Readiness Summary",
    "Flow Overview And Steps",
    "Results And Verdict",
    "UI Polish Report",
    "UX Polish Report",
    "Performance Metrics And Interaction Latency",
    "Product Flow Cohesiveness And Group Coherent-Product Judgment",
    "Screenshot Evidence",
    "Product-Level Assessment",
    "Grouped Scores And Coherent Product Judgment",
    "Individual Dimension Judgments",
    "Data And Control-Plane Setup",
    "Navigation, Orientation, And Information Architecture",
    "Animation, Transition, And Haptics Quality",
    "Risk Severity And Validation Confidence",
    "Evidence Provenance",
    "Before/After Deltas",
    "Revalidation Status",
    "Owner And Status",
    "Data Integrity And State Sync",
    "Navigation State And Restoration",
    "Device, Viewport, And Generated-Page Coverage",
    "Media Session And Background Audio Continuity",
]


class ScenarioReportGeneratorTests(unittest.TestCase):
    def test_generates_index_and_required_per_scenario_pages(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            catalog = write_catalog(root / "catalog")
            out = root / "site"

            records = build_records(catalog, root)
            validate_schema_contract(records, DOCS_TESTING_DIR / "scenario-report.schema.json")
            write_site(records, out, catalog, root)

            self.assertTrue((out / "index.html").exists())
            self.assertTrue((out / "provider-cassettes" / "index.html").exists())
            self.assertTrue((out / "data" / "provider-cassettes.json").exists())
            self.assertTrue((out / "scenarios" / "index.html").exists())
            self.assertTrue((out / "scenarios" / "smoke-001" / "index.html").exists())
            self.assertTrue((out / "scenarios" / "smoke-001" / "data.json").exists())
            self.assertTrue((out / "scenarios" / "smoke-002" / "index.html").exists())
            self.assertEqual(len(list((out / "scenarios").glob("*/data.json"))), 2)

            home = (out / "index.html").read_text()
            scenario_page = (out / "scenarios" / "smoke-001" / "index.html").read_text()
            self.assertNotIn("Required Detailed Sections", home)
            self.assertIn("Provider cassette replay", home)
            self.assertIn('rel="icon" href="assets/favicon.svg"', home)
            self.assertIn('rel="icon" href="../../assets/favicon.svg"', scenario_page)
            self.assertIn("Provider Cassette Replay Coverage", (out / "provider-cassettes" / "index.html").read_text())
            for section in REQUIRED_SCENARIO_SECTIONS:
                self.assertIn(section, scenario_page)

            data = json.loads((out / "scenarios" / "smoke-001" / "data.json").read_text())
            self.assertEqual(data["page"]["canonical_url"], "https://example.test/podcast-player/scenarios/smoke-001/")
            self.assertEqual(data["review_grounding"]["search_command"], 'npx skills search "Liquid Glass iOS mobile UI UX polish accessibility frontend design"')
            self.assertEqual(
                [skill["name"] for skill in SKILL_GROUNDING if skill["selected"]],
                [
                    "heyman333/atelier-ui@ios-glass-ui-designer",
                    "phazurlabs/ux-ui-mastery@Mobile UX Design",
                ],
            )
            self.assertEqual(data["launch_assessment"]["launch_readiness"], "incomplete")
            self.assertEqual(data["launch_assessment"]["issue_refs"], ["GH-702", "GH-730", "GH-733", "GH-700"])
            self.assertIn("placeholder:screenshot", {item["id"] for item in data["evidence"]["placeholders"]})
            self.assertIn("GH-702", {issue["id"] for issue in data["issues"]})

    def test_preserves_existing_pages_assets_and_issue_index(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            catalog = write_catalog(root / "catalog")
            out = root / "site"
            old_asset = out / "assets" / "scenarios" / "smoke-001" / "old-shot.jpg"
            old_asset.parent.mkdir(parents=True)
            old_asset.write_bytes(b"old image")
            (out / ".git").write_text("gitdir: ../.git/worktrees/site\n")
            (out / ".nojekyll").write_text("")
            (out / "stale.html").write_text("stale")
            old_issues = {"issues": [{"id": "ISSUE-1", "url": "https://example.test/1"}], "counts": {"open": 1, "fixed": 0}}
            (out / "data").mkdir()
            (out / "data" / "issues.json").write_text(json.dumps(old_issues))

            records = build_records(catalog, root)
            old_record = copy.deepcopy(records[0])
            old_record["execution"] = {**old_record["execution"], "status": "pass"}
            old_record["verdict"] = {
                **old_record["verdict"],
                "overall": "pass_with_issues",
                "summary": "Previous run passed with one issue.",
            }
            old_record["coherence"] = {
                **old_record["coherence"],
                "group_judgment": {
                    **old_record["coherence"]["group_judgment"],
                    "status": "pass_with_issues",
                },
            }
            old_record["review_grounding"] = {
                **old_record["review_grounding"],
                "selected_skills": [
                    {
                        "selected": True,
                        "name": "obsolete/old-skill@legacy",
                        "search_terms": "old terms",
                        "coverage": "Old grounding should not survive regeneration.",
                    }
                ],
            }
            old_record["sections"]["review_skill_grounding"] = {
                **old_record["sections"]["review_skill_grounding"],
                "notes": ["Selected skills: obsolete/old-skill@legacy."],
            }
            old_record["dimension_scores"] = {
                **old_record["dimension_scores"],
                "actual_result": {
                    **old_record["dimension_scores"]["actual_result"],
                    "score": 3,
                    "status": "pass_with_issues",
                    "evidence_refs": ["artifact:old-shot"],
                },
                "ui_polish": {
                    **old_record["dimension_scores"]["ui_polish"],
                    "score": 2,
                    "status": "pass_with_issues",
                    "evidence_refs": ["artifact:old-shot"],
                },
            }
            old_record["group_scores"] = {
                **old_record["group_scores"],
                "functional_correctness": {
                    **old_record["group_scores"]["functional_correctness"],
                    "score": 3,
                    "status": "pass_with_issues",
                },
                "product_experience": {
                    **old_record["group_scores"]["product_experience"],
                    "score": 2,
                    "status": "pass_with_issues",
                },
            }
            old_record["metrics"] = [
                {
                    "id": "metric-screen-settle",
                    "name": "Screen settle",
                    "value": 120,
                    "unit": "ms",
                    "budget": "<200ms",
                    "status": "pass",
                    "method": "fixture",
                    "evidence_refs": ["artifact:old-shot"],
                }
            ]
            old_record["evidence"] = {
                **old_record["evidence"],
                "artifacts": [
                    *old_record["evidence"]["artifacts"],
                    {
                        "id": "artifact:old-shot",
                        "type": "screenshot",
                        "path": "assets/scenarios/smoke-001/old-shot.jpg",
                        "description": "Previous validated screenshot.",
                        "step_id": "step-02-action",
                        "captured_at": GENERATED_AT,
                        "device": "iPhone 16 Pro",
                        "os_version": "iOS 26.0",
                        "sha256": "a" * 64,
                        "alt": "Smoke scenario screenshot",
                        "caption": "Previous validated screenshot.",
                        "width": 368,
                        "height": 800,
                        "required": True,
                        "redaction": {"status": "not_needed"},
                    },
                ],
            }
            old_record["issues"] = [
                {
                    "id": "ISSUE-1",
                    "url": "https://example.test/issues/1",
                    "severity": "major",
                    "title": "Previous issue",
                    "affected_dimensions": ["ui_polish"],
                    "status": "open",
                }
            ]
            old_record["sections"] = {
                **old_record["sections"],
                "review_skill_grounding": {
                    **old_record["sections"]["review_skill_grounding"],
                    "summary": "Old skill grounding used phazurlabs/ux-ui-mastery@Mobile UX Design.",
                    "notes": ["Selected skills: phazurlabs/ux-ui-mastery@Mobile UX Design."],
                },
            }
            stale_contract_keys = {
                "evidence_provenance": "evidence_provenance",
                "before_after_deltas": "before_after_deltas",
                "revalidation_status": "revalidation_status",
                "owner_status": "owner_status",
            }
            for section_key, dimension_key in stale_contract_keys.items():
                old_record["sections"].pop(section_key)
                old_record["dimension_scores"].pop(dimension_key)
            stale_group_refs = {
                "evidence_reproducibility": "evidence_provenance",
                "product_experience": "before_after_deltas",
                "follow_through": "revalidation_status",
            }
            for group_key, dimension_key in stale_group_refs.items():
                old_record["group_scores"][group_key]["dimension_refs"] = [
                    item for item in old_record["group_scores"][group_key]["dimension_refs"] if item != dimension_key
                ]
            old_record["group_scores"]["follow_through"]["dimension_refs"] = [
                item for item in old_record["group_scores"]["follow_through"]["dimension_refs"] if item != "owner_status"
            ]
            old_record["next_actions"] = [
                item for item in old_record["next_actions"] if item["id"] not in {"revalidate-defects", "assign-owners"}
            ]
            previous_record_path = out / "scenarios" / "smoke-001" / "data.json"
            previous_record_path.parent.mkdir(parents=True)
            previous_record_path.write_text(json.dumps(old_record))

            write_site(records, out, catalog, root)

            self.assertEqual(old_asset.read_bytes(), b"old image")
            self.assertEqual((out / ".git").read_text(), "gitdir: ../.git/worktrees/site\n")
            self.assertTrue((out / ".nojekyll").exists())
            self.assertFalse((out / "stale.html").exists())
            issue_index = json.loads((out / "data" / "issues.json").read_text())
            self.assertEqual(issue_index["counts"], {"open": 8})
            indexed_issues = {(item["scenario_id"], item["id"]): item for item in issue_index["issues"]}
            self.assertIn(("SMOKE-001", "ISSUE-1"), indexed_issues)
            self.assertIn(("SMOKE-001", "GH-700"), indexed_issues)
            self.assertIn(("SMOKE-002", "GH-702"), indexed_issues)
            self.assertEqual(indexed_issues[("SMOKE-001", "ISSUE-1")]["scenario_slug"], "smoke-001")
            merged = json.loads(previous_record_path.read_text())
            self.assertEqual(merged["execution"]["status"], "pass")
            self.assertEqual(merged["verdict"]["overall"], "pass_with_issues")
            self.assertEqual(merged["coherence"]["group_judgment"]["status"], "pass_with_issues")
            self.assertEqual(merged["metrics"][0]["id"], "metric-screen-settle")
            self.assertIn("artifact:old-shot", {artifact["id"] for artifact in merged["evidence"]["artifacts"]})
            self.assertEqual(
                [skill["name"] for skill in merged["review_grounding"]["selected_skills"]],
                [
                    "heyman333/atelier-ui@ios-glass-ui-designer",
                    "phazurlabs/ux-ui-mastery@Mobile UX Design",
                ],
            )
            self.assertNotIn("obsolete/old-skill", json.dumps(merged["sections"]["review_skill_grounding"]))
            self.assertIn("heyman333/atelier-ui@ios-glass-ui-designer", merged["sections"]["review_skill_grounding"]["notes"][0])
            self.assertNotIn("obsolete", merged["sections"]["review_skill_grounding"]["summary"])
            for section_key, dimension_key in stale_contract_keys.items():
                self.assertIn(section_key, merged["sections"])
                self.assertIn(dimension_key, merged["dimension_scores"])
            self.assertIn("evidence_provenance", merged["group_scores"]["evidence_reproducibility"]["dimension_refs"])
            self.assertIn("before_after_deltas", merged["group_scores"]["product_experience"]["dimension_refs"])
            self.assertIn("revalidation_status", merged["group_scores"]["follow_through"]["dimension_refs"])
            self.assertIn("owner_status", merged["group_scores"]["follow_through"]["dimension_refs"])
            self.assertTrue({"revalidate-defects", "assign-owners"}.issubset({item["id"] for item in merged["next_actions"]}))
            rollups = json.loads((out / "data" / "rollups.json").read_text())
            self.assertEqual(rollups["average_dimension_scores"]["actual_result"], 1.5)
            self.assertEqual(rollups["average_dimension_scores"]["ui_polish"], 1)
            self.assertEqual(rollups["average_group_scores"]["functional_correctness"], 1.5)
            self.assertEqual(rollups["issues_by_severity"], {"major": 8})
            self.assertEqual(rollups["open_issues_by_severity"], {"major": 8})
            scenario_page = previous_record_path.with_name("index.html").read_text()
            home = (out / "index.html").read_text()
            self.assertIn("Screenshot Evidence", scenario_page)
            self.assertIn("<img", scenario_page)
            self.assertIn('width="368" height="800"', scenario_page)
            self.assertIn('width="368" height="800"', home)
            self.assertIn("Evidence-Backed Scenarios", home)
            self.assertIn("old-shot.jpg", home)

    def test_provider_cassette_refs_are_checked_against_catalog_pages(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            catalog = write_catalog(root / "catalog")
            cassette_dir = root / "tests" / "fixtures" / "provider_cassettes"
            cassette_dir.mkdir(parents=True)
            (cassette_dir / "smoke-provider.json").write_text(
                json.dumps(
                    {
                        "id": "smoke-provider",
                        "provider": "openrouter",
                        "operation": "chat_completion",
                        "scenario_refs": ["SMOKE-001", "E2"],
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

            data = provider_cassette_data(root, root / "evidence", catalog)
            self.assertEqual(data["catalog_scenario_count"], 2)
            self.assertEqual(data["mapped_scenario_refs"], ["SMOKE-001"])
            self.assertEqual(data["unmapped_scenario_refs"], ["E2"])
            self.assertFalse(data["all_refs_current_catalog_ids"])
            self.assertEqual(data["cassettes"][0]["nmp_rules"], ["D7"])
            links = {item["id"]: item for item in data["cassettes"][0]["scenario_ref_links"]}
            self.assertEqual(links["SMOKE-001"]["slug"], "smoke-001")
            self.assertTrue(links["SMOKE-001"]["mapped"])
            self.assertFalse(links["E2"]["mapped"])

            page = render_provider_cassette_page(data, 1)
            self.assertIn("../scenarios/smoke-001/", page)
            self.assertIn("E2 unmapped", page)

    def test_schema_requires_quality_coherence_and_screenshot_evidence(self) -> None:
        schema = json.loads((DOCS_TESTING_DIR / "scenario-report.schema.json").read_text())
        quality = schema["properties"]["quality_review"]
        self.assertFalse(quality["additionalProperties"])
        self.assertEqual(
            set(quality["required"]),
            {
                "ui",
                "ux",
                "performance",
                "accessibility",
                "reliability",
                "privacy_security",
                "content_localization",
                "controls_gestures",
                "offline_resume",
                "observability",
            },
        )
        self.assertEqual(schema["$defs"]["coherence"]["properties"]["cluster"]["$ref"], "#/$defs/coherence_cluster")
        self.assertIn("launch_assessment", schema["required"])
        self.assertIn("placeholders", schema["properties"]["evidence"]["required"])
        self.assertTrue(
            {
                "data_integrity_state_sync",
                "navigation_state_restoration",
                "device_viewport_coverage",
                "media_session_background_continuity",
            }.issubset(set(schema["$defs"]["dimension_scores"]["required"]))
        )
        screenshot_rule = schema["$defs"]["artifact"]["allOf"][0]["then"]["required"]
        self.assertTrue({"alt", "caption", "step_id", "captured_at", "device", "os_version", "sha256", "width", "height"}.issubset(screenshot_rule))


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
                "| SMOKE-001 | Given a seeded library, when the listener opens Library, then subscribed shows render with stable navigation. | ss: library screenshot; perf: screen settle metric; deps: seeded library; boundary: D1,D5. |",
                "| SMOKE-002 | Given offline mode, when the listener opens a cached show, then cached episodes remain available. | ss: offline screenshot; perf: none; deps: offline fixture; boundary: D5. |",
            ]
        )
        + "\n"
    )
    return catalog


def build_records(catalog: Path, repo: Path) -> list[dict[str, object]]:
    scenarios = parse_catalog(catalog)
    return [build_report(scenario, scenarios, repo, GENERATED_AT, SITE_BASE) for scenario in scenarios]


if __name__ == "__main__":
    unittest.main()
