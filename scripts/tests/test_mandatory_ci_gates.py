import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class MandatoryCIGateTests(unittest.TestCase):
    def test_missing_owner_row_fails_inventory_gate(self) -> None:
        with tempfile.TemporaryDirectory(prefix="pod0-owner-gate-") as directory:
            root = Path(directory)
            source = root / "App/Sources/Unowned.swift"
            source.parent.mkdir(parents=True)
            source.write_text("struct Unowned {}\n", encoding="utf-8")
            architecture = root / "docs/architecture"
            architecture.mkdir(parents=True)
            (architecture / "ownership-coverage.json").write_text(
                json.dumps(
                    {
                        "production_sources": {".swift": ["App/Sources/"]},
                        "behavioral_inventories": [],
                        "derived_behavior_patterns": [],
                    }
                ),
                encoding="utf-8",
            )
            (architecture / "ownership.json").write_text(
                json.dumps(
                    {
                        "coverage_manifest": "docs/architecture/ownership-coverage.json",
                        "classifications": ["native_by_design"],
                        "entries": [],
                    }
                ),
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "scripts/check_architecture_ownership.py"),
                    "--root",
                    str(root),
                ],
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("uncovered production file: App/Sources/Unowned.swift", result.stderr)

    def test_stale_binding_fails_snapshot_gate(self) -> None:
        with tempfile.TemporaryDirectory(prefix="pod0-binding-gate-") as directory:
            root = Path(directory)
            expected = root / "expected"
            actual = root / "actual"
            for snapshot in (expected, actual):
                (snapshot / "Swift").mkdir(parents=True)
                (snapshot / "Kotlin").mkdir()
                (snapshot / "Swift/core.swift").write_text(
                    "struct Contract {}\n", encoding="utf-8"
                )
                (snapshot / "Kotlin/core.kt").write_text(
                    "class Contract\n", encoding="utf-8"
                )
                (snapshot / "bindings.fingerprint").write_text(
                    "contract-v1\n", encoding="utf-8"
                )
            actual.joinpath("Swift/core.swift").write_text(
                "struct StaleContract {}\n", encoding="utf-8"
            )
            result = subprocess.run(
                [
                    str(ROOT / "scripts/compare_core_binding_snapshot.sh"),
                    str(expected),
                    str(actual),
                ],
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("StaleContract", result.stdout)

    def test_required_workflows_invoke_the_combined_gate(self) -> None:
        command = "./scripts/check_mandatory_ci_gates.sh"
        for relative in (".github/workflows/test.yml", ".github/workflows/testflight.yml"):
            workflow = ROOT.joinpath(relative).read_text(encoding="utf-8")
            self.assertIn(command, workflow, relative)


if __name__ == "__main__":
    unittest.main()
