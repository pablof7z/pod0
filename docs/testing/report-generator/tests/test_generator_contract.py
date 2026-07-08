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
from generator_contract_fixtures import GENERATED_AT, SITE_BASE, build_records, write_catalog, write_provider_cassette  # noqa: E402
from next_wave import load_next_wave, next_wave_export  # noqa: E402
from provider_cassettes_report import provider_cassette_data, render_provider_cassette_page  # noqa: E402
from records import validate_schema_contract  # noqa: E402

REQUIRED_SCENARIO_SECTIONS = [
    "What Was Attempted And Test Intent",
    "Launch Readiness Summary",
    "Flow Overview And Steps",
    "Evidence Readiness Matrix",
    "Provider Replay Coverage",
    "Results And Verdict",
    "Replay, Cassettes, And Provenance Detail",
    "Skill-Grounded Mobile Design Rubric",
    "UI Polish Report",
    "UX Polish Report",
    "Accessibility, Dynamic Type, And Touch Ergonomics Detail",
    "Performance Metrics And Interaction Latency",
    "Liquid Glass And iOS Primitive Detail",
    "Product Flow Cohesiveness And Group Coherent-Product Judgment",
    "NMP/RMP Boundary And Data Integrity",
    "Screenshot Evidence",
    "Product-Level Assessment",
    "Issue Filing And Revalidation Ledger",
    "Grouped Scores And Coherent Product Judgment",
    "Individual Dimension Judgments",
    "Functional Validation Detail",
    "Evidence And Replay Detail",
    "Product Experience Detail",
    "Engineering Boundary Detail",
    "Follow-Through Detail",
    "Data And Control-Plane Setup",
    "Navigation, Orientation, And Information Architecture",
    "Animation, Transition, And Haptics Quality",
    "State, Recovery, And Continuity Coverage",
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
            write_provider_cassette(root, ["SMOKE-001"])
            out = root / "site"

            records = build_records(catalog, root)
            validate_schema_contract(records, DOCS_TESTING_DIR / "scenario-report.schema.json")
            write_site(records, out, catalog, root)

            self.assertTrue((out / "index.html").exists())
            self.assertTrue((out / "provider-cassettes" / "index.html").exists())
            self.assertTrue((out / "data" / "provider-cassettes.json").exists())
            self.assertTrue((out / "architecture-scan" / "index.html").exists())
            self.assertTrue((out / "data" / "architecture-scan.json").exists())
            self.assertTrue((out / "next-wave" / "index.html").exists())
            self.assertTrue((out / "data" / "next-wave.json").exists())
            self.assertTrue((out / "scenarios" / "index.html").exists())
            self.assertTrue((out / "scenarios" / "smoke-001" / "index.html").exists())
            self.assertTrue((out / "scenarios" / "smoke-001" / "data.json").exists())
            self.assertTrue((out / "scenarios" / "smoke-002" / "index.html").exists())
            self.assertEqual(len(list((out / "scenarios").glob("*/data.json"))), 2)

            home = (out / "index.html").read_text()
            scenario_page = (out / "scenarios" / "smoke-001" / "index.html").read_text()
            self.assertNotIn("Required Detailed Sections", home)
            self.assertIn("Provider cassette replay", home)
            self.assertIn("Next Execution Wave", home)
            self.assertIn("Architecture scan", home)
            self.assertIn('rel="icon" href="assets/favicon.svg"', home)
            self.assertIn('rel="icon" href="../../assets/favicon.svg"', scenario_page)
            self.assertIn("Provider Cassette Replay Coverage", (out / "provider-cassettes" / "index.html").read_text())
            self.assertIn("NMP Architecture Scan", (out / "architecture-scan" / "index.html").read_text())
            self.assertIn("Foundation Onboarding Screenshot Wave 001", (out / "next-wave" / "index.html").read_text())
            for section in REQUIRED_SCENARIO_SECTIONS:
                self.assertIn(section, scenario_page)

            data = json.loads((out / "scenarios" / "smoke-001" / "data.json").read_text())
            self.assertEqual(data["page"]["canonical_url"], "https://example.test/podcast-player/scenarios/smoke-001/")
            scan = json.loads((out / "data" / "architecture-scan.json").read_text())
            self.assertEqual(scan["status"], "not_recorded")
            next_wave = json.loads((out / "data" / "next-wave.json").read_text())
            self.assertEqual(next_wave["wave_id"], "foundation-onboarding-wave-001")
            self.assertEqual(next_wave["target_count"], 8)
            self.assertEqual(next_wave["mapped_count"], 0)
            self.assertEqual(data["review_grounding"]["search_command"], 'npx skills search "UI UX mobile Liquid Glass design review accessibility"')
            self.assertEqual(
                [skill["name"] for skill in SKILL_GROUNDING if skill["selected"]],
                [
                    "casper-studios/casper-marketplace@liquid-glass",
                    "charleswiltgen/axiom@axiom-design",
                    "charleswiltgen/axiom@axiom-accessibility",
                    "charleswiltgen/axiom@axiom-performance",
                    "phazurlabs/ux-ui-mastery@mobile ux design",
                ],
            )
            self.assertEqual(data["launch_assessment"]["launch_readiness"], "incomplete")
            self.assertEqual(data["launch_assessment"]["issue_refs"], ["GH-702", "GH-730", "GH-733", "GH-700", "GH-731"])
            self.assertEqual(data["run"]["provider_mode"], "replay")
            self.assertEqual(data["run"]["cassettes"][0]["id"], "smoke-provider")
            self.assertIn("cassette:smoke-provider", {item["id"] for item in data["evidence"]["artifacts"]})
            self.assertNotIn("cassette", {item["kind"] for item in data["evidence"]["missing"]})
            self.assertIn("placeholder:screenshot", {item["id"] for item in data["evidence"]["placeholders"]})
            self.assertIn("report_viewport", data["evidence"]["required_kinds"])
            self.assertIn("placeholder:report_viewport", {item["id"] for item in data["evidence"]["placeholders"]})
            self.assertIn("GH-702", {issue["id"] for issue in data["issues"]})
            self.assertIn("Replay fixtures mapped to this scenario", scenario_page)
            self.assertIn("smoke-provider", scenario_page)

    def test_preserves_existing_pages_assets_and_issue_index(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            catalog = write_catalog(root / "catalog")
            out = root / "site"
            old_asset = out / "assets" / "scenarios" / "smoke-001" / "old-shot.jpg"
            old_asset.parent.mkdir(parents=True)
            old_asset.write_bytes(b"old image")
            old_live_asset = out / "assets" / "live-verification" / "old-live-check.png"
            old_live_asset.parent.mkdir(parents=True)
            old_live_asset.write_bytes(b"old live screenshot")
            evidence_root = root / "evidence"
            new_evidence_asset = evidence_root / "assets" / "scenarios" / "smoke-002" / "new-shot.jpg"
            new_evidence_asset.parent.mkdir(parents=True)
            new_evidence_asset.write_bytes(b"new image")
            (out / ".git").write_text("gitdir: ../.git/worktrees/site\n")
            (out / ".nojekyll").write_text("")
            (out / "stale.html").write_text("stale")
            legacy_page = out / "workstreams" / "index.html"
            legacy_page.parent.mkdir(parents=True)
            legacy_page.write_text("legacy workstream page")
            legacy_screenshot_page = out / "screenshots" / "index.html"
            legacy_screenshot_page.parent.mkdir(parents=True)
            legacy_screenshot_page.write_text("legacy screenshot page")
            old_issues = {"issues": [{"id": "ISSUE-1", "url": "https://example.test/1"}], "counts": {"open": 1, "fixed": 0}}
            (out / "data").mkdir()
            (out / "data" / "issues.json").write_text(json.dumps(old_issues))
            (out / "data" / "workstreams.json").write_text(json.dumps({"status": "legacy"}))

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
            old_record["revalidation_update"] = {
                "status": "historical_extension",
                "summary": "Published page carried an additive field from a manual revalidation note.",
            }
            previous_record_path = out / "scenarios" / "smoke-001" / "data.json"
            previous_record_path.parent.mkdir(parents=True)
            previous_record_path.write_text(json.dumps(old_record))

            write_site(records, out, catalog, root, evidence_root)

            self.assertEqual(old_asset.read_bytes(), b"old image")
            self.assertEqual(old_live_asset.read_bytes(), b"old live screenshot")
            self.assertEqual((out / "assets" / "scenarios" / "smoke-002" / "new-shot.jpg").read_bytes(), b"new image")
            self.assertEqual((out / ".git").read_text(), "gitdir: ../.git/worktrees/site\n")
            self.assertTrue((out / ".nojekyll").exists())
            self.assertFalse((out / "stale.html").exists())
            self.assertEqual(legacy_page.read_text(), "legacy workstream page")
            self.assertEqual(legacy_screenshot_page.read_text(), "legacy screenshot page")
            self.assertEqual(json.loads((out / "data" / "workstreams.json").read_text()), {"status": "legacy"})
            issue_index = json.loads((out / "data" / "issues.json").read_text())
            self.assertEqual(issue_index["counts"], {"open": 9})
            indexed_issues = {(item["scenario_id"], item["id"]): item for item in issue_index["issues"]}
            self.assertIn(("SMOKE-001", "ISSUE-1"), indexed_issues)
            self.assertIn(("SMOKE-001", "GH-700"), indexed_issues)
            self.assertIn(("SMOKE-001", "GH-731"), indexed_issues)
            self.assertIn(("SMOKE-002", "GH-702"), indexed_issues)
            self.assertEqual(indexed_issues[("SMOKE-001", "ISSUE-1")]["scenario_slug"], "smoke-001")
            merged = json.loads(previous_record_path.read_text())
            self.assertEqual(merged["execution"]["status"], "pass")
            self.assertEqual(merged["verdict"]["overall"], "pass_with_issues")
            self.assertEqual(merged["coherence"]["group_judgment"]["status"], "pass_with_issues")
            self.assertEqual(merged["metrics"][0]["id"], "metric-screen-settle")
            self.assertNotIn("revalidation_update", merged)
            self.assertIn("artifact:old-shot", {artifact["id"] for artifact in merged["evidence"]["artifacts"]})
            self.assertIn("report_viewport", merged["evidence"]["required_kinds"])
            self.assertIn("placeholder:report_viewport", {item["id"] for item in merged["evidence"]["placeholders"]})
            self.assertEqual(
                [skill["name"] for skill in merged["review_grounding"]["selected_skills"]],
                [
                    "casper-studios/casper-marketplace@liquid-glass",
                    "charleswiltgen/axiom@axiom-design",
                    "charleswiltgen/axiom@axiom-accessibility",
                    "charleswiltgen/axiom@axiom-performance",
                    "phazurlabs/ux-ui-mastery@mobile ux design",
                ],
            )
            self.assertNotIn("obsolete/old-skill", json.dumps(merged["sections"]["review_skill_grounding"]))
            self.assertIn("casper-studios/casper-marketplace@liquid-glass", merged["sections"]["review_skill_grounding"]["notes"][0])
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
            self.assertEqual(rollups["issues_by_severity"], {"major": 9})
            self.assertEqual(rollups["open_issues_by_severity"], {"major": 9})
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

    def test_next_wave_manifest_maps_foundation_targets(self) -> None:
        scenarios = parse_catalog(DOCS_TESTING_DIR / "scenarios" / "catalog")
        records = [
            {
                "scenario": {
                    "id": scenario.scenario_id,
                    "slug": scenario.slug,
                    "title": scenario.title,
                    "category": scenario.category,
                },
                "execution": {"status": "not_run"},
                "verdict": {"overall": "incomplete"},
                "readiness": {"ship_gate": "incomplete"},
                "evidence": {"missing": []},
            }
            for scenario in scenarios
        ]
        data = next_wave_export(records, load_next_wave())
        self.assertEqual(data["target_count"], 8)
        self.assertEqual(data["mapped_count"], 8)
        self.assertEqual(data["not_run_count"], 8)
        fnd001 = next(item for item in data["scenarios"] if item["scenario_id"] == "FND-001")
        self.assertEqual(fnd001["catalog_status"], "mapped")
        self.assertTrue(fnd001["screenshot_requirements"])
        self.assertTrue(fnd001["performance_metrics"])
        self.assertTrue(fnd001["ui_ux_liquid_glass_checks"])
        self.assertTrue(fnd001["issue_filing_gates"])

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


if __name__ == "__main__":
    unittest.main()
