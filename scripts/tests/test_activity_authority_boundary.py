import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from activity_authority_boundary import validate_privileged_writers


class ActivityAuthorityBoundaryTests(unittest.TestCase):
    def test_legacy_native_drain_and_observation_are_forbidden(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rust_source = root / "rust/crates/fixture/src/lib.rs"
            rust_source.parent.mkdir(parents=True)
            rust_source.write_text("", encoding="utf-8")
            source = root / "App/Sources/Core/Bypass.swift"
            source.parent.mkdir(parents=True)
            source.write_text(
                "facade.nextHostRequests(maximumCount: 1)\n"
                "facade.recordHostObservation(observation: value)\n",
                encoding="utf-8",
            )
            errors = validate_privileged_writers(root)

        self.assertTrue(any("nextHostRequests(" in error for error in errors))
        self.assertTrue(any("recordHostObservation(" in error for error in errors))

    def test_raw_dispatcher_execution_is_forbidden_outside_leased_adapter(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rust_source = root / "rust/crates/fixture/src/lib.rs"
            rust_source.parent.mkdir(parents=True)
            rust_source.write_text("", encoding="utf-8")
            source = root / "App/Sources/Core/Bypass.swift"
            source.parent.mkdir(parents=True)
            source.write_text(
                "dispatcher.executePersistedLeaseRequest(envelope) { _ in }\n",
                encoding="utf-8",
            )
            errors = validate_privileged_writers(root)

        self.assertTrue(
            any("executePersistedLeaseRequest(" in error for error in errors)
        )


if __name__ == "__main__":
    unittest.main()
