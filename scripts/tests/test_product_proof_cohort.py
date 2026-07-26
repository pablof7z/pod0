from datetime import datetime, timedelta, timezone
import unittest
from uuid import UUID

from scripts.product_proof_cohort import analyze
from scripts.product_proof_export import CohortInputError, PRIVACY_STATEMENT


BASE = datetime(2026, 7, 1, tzinfo=timezone.utc)
INSTALL_A = UUID("11111111-1111-1111-1111-111111111111")
INSTALL_B = UUID("22222222-2222-2222-2222-222222222222")


def signal(
    index: int,
    install: UUID,
    name: str,
    outcome: str,
    occurred_at: datetime,
    latency: str = None,
) -> dict:
    value = {
        "schemaVersion": 1,
        "id": str(UUID(int=index)),
        "anonymousInstallID": str(install),
        "occurredAt": occurred_at.isoformat().replace("+00:00", "Z"),
        "name": name,
        "outcome": outcome,
    }
    if latency is not None:
        value["latencyBucket"] = latency
    return value


def archive(signals: list[dict]) -> dict:
    return {
        "schemaVersion": 1,
        "privacy": PRIVACY_STATEMENT,
        "report": {
            "schemaVersion": 1,
            "generatedAt": BASE.isoformat().replace("+00:00", "Z"),
            "signalCount": len(signals),
            "distinctActiveDays": 1,
            "counts": [],
        },
        "signals": signals,
    }


def measure(report: dict, key: str) -> dict:
    return next(item for item in report["measures"] if item["key"] == key)


class ProductProofCohortTests(unittest.TestCase):
    def test_groups_exports_and_deduplicates_overlapping_signals(self) -> None:
        launch = signal(1, INSTALL_A, "appLaunch", "started", BASE)
        first = archive([launch])
        second = archive([
            launch,
            signal(
                2,
                INSTALL_A,
                "firstSubscription",
                "created",
                BASE + timedelta(hours=1),
            ),
            signal(
                3,
                INSTALL_A,
                "playStarted",
                "succeeded",
                BASE + timedelta(hours=2),
            ),
            signal(
                4,
                INSTALL_A,
                "appLaunch",
                "started",
                BASE + timedelta(days=1),
            ),
        ])

        report = analyze(
            [("first.json", first), ("second.json", second)],
            generated_at=BASE,
        )

        self.assertEqual(report["sourceArchiveCount"], 2)
        self.assertEqual(report["uniqueSignalCount"], 4)
        self.assertEqual(report["evaluatedInstallCount"], 1)
        self.assertEqual(measure(report, "activation")["numerator"], 1)
        self.assertEqual(measure(report, "repeat_use")["numerator"], 1)

    def test_activation_must_complete_within_first_24_hours(self) -> None:
        signals = [
            signal(1, INSTALL_A, "appLaunch", "started", BASE),
            signal(
                2,
                INSTALL_A,
                "firstSubscription",
                "created",
                BASE + timedelta(hours=1),
            ),
            signal(
                3,
                INSTALL_A,
                "playStarted",
                "succeeded",
                BASE + timedelta(hours=25),
            ),
        ]

        report = analyze([("late.json", archive(signals))], generated_at=BASE)

        self.assertEqual(measure(report, "activation")["numerator"], 0)

    def test_recall_minimum_uses_asks_and_grounded_denominator(self) -> None:
        signals = [signal(1, INSTALL_A, "appLaunch", "started", BASE)]
        next_id = 2
        for offset in range(50):
            when = BASE + timedelta(minutes=offset)
            signals.append(
                signal(next_id, INSTALL_A, "recallAsked", "started", when)
            )
            next_id += 1
            signals.append(signal(
                next_id,
                INSTALL_A,
                "recallGrounded",
                "grounded",
                when,
                latency="milliseconds250To749",
            ))
            next_id += 1
        for offset in range(15):
            signals.append(signal(
                next_id,
                INSTALL_A,
                "recallCitationOpened",
                "opened",
                BASE + timedelta(minutes=offset),
            ))
            next_id += 1

        report = analyze(
            [("recall.json", archive(signals))],
            generated_at=BASE,
        )
        citation = measure(report, "citation_use")

        self.assertEqual(citation["denominator"], 50)
        self.assertEqual(citation["evidenceSampleSize"], 50)
        self.assertEqual(citation["pointEstimate"], 0.3)
        self.assertEqual(measure(report, "recall_latency")["status"], "passes")

    def test_wilson_interval_and_minimum_are_reported(self) -> None:
        signals = [signal(1, INSTALL_A, "appLaunch", "started", BASE)]
        for index in range(2, 102):
            outcome = "succeeded" if index <= 51 else "failed"
            signals.append(
                signal(index, INSTALL_A, "resumeAttempt", outcome, BASE)
            )

        report = analyze(
            [("rates.json", archive(signals))],
            generated_at=BASE,
        )
        resume = measure(report, "resume_reliability")

        self.assertEqual(resume["pointEstimate"], 0.5)
        self.assertEqual(resume["wilson95"]["lower"], 0.403832)
        self.assertEqual(resume["status"], "misses_threshold")

    def test_unknown_fields_are_rejected(self) -> None:
        item = signal(1, INSTALL_A, "appLaunch", "started", BASE)
        item["query"] = "content must not enter an export"

        with self.assertRaisesRegex(CohortInputError, "unexpected fields: query"):
            analyze([("private.json", archive([item]))], generated_at=BASE)

    def test_unknown_nested_report_fields_are_rejected(self) -> None:
        exported = archive([
            signal(1, INSTALL_A, "appLaunch", "started", BASE),
        ])
        exported["report"]["counts"] = [{
            "name": "appLaunch",
            "outcome": "started",
            "count": 1,
            "title": "content must not enter a report",
        }]

        with self.assertRaisesRegex(CohortInputError, "unexpected fields: title"):
            analyze([("private.json", exported)], generated_at=BASE)

    def test_conflicting_duplicate_signal_is_rejected(self) -> None:
        first = signal(1, INSTALL_A, "appLaunch", "started", BASE)
        conflict = signal(
            1,
            INSTALL_A,
            "appLaunch",
            "started",
            BASE + timedelta(seconds=1),
        )

        with self.assertRaisesRegex(CohortInputError, "conflicts across exports"):
            analyze(
                [
                    ("first.json", archive([first])),
                    ("second.json", archive([conflict])),
                ],
                generated_at=BASE,
            )

    def test_data_loss_is_a_stop_when_cohorts_are_insufficient(self) -> None:
        signals = [
            signal(1, INSTALL_A, "appLaunch", "started", BASE),
            signal(2, INSTALL_A, "dataLossEvidence", "detected", BASE),
        ]

        report = analyze(
            [("loss.json", archive(signals))],
            generated_at=BASE,
        )

        self.assertEqual(report["dataSafety"]["dataLossEvidenceDetected"], 1)
        self.assertEqual(
            report["productEvidenceStatus"],
            "stop_data_loss_detected",
        )

    def test_data_loss_on_ignored_install_still_stops(self) -> None:
        report = analyze(
            [("loss.json", archive([
                signal(1, INSTALL_B, "dataLossEvidence", "detected", BASE),
            ]))],
            generated_at=BASE,
        )

        self.assertEqual(report["evaluatedInstallCount"], 0)
        self.assertEqual(report["dataSafety"]["dataLossEvidenceDetected"], 1)

    def test_install_without_launch_is_not_evaluated(self) -> None:
        report = analyze(
            [
                ("evaluated.json", archive([
                    signal(1, INSTALL_A, "appLaunch", "started", BASE),
                ])),
                ("ignored.json", archive([
                    signal(2, INSTALL_B, "playStarted", "succeeded", BASE),
                ])),
            ],
            generated_at=BASE,
        )

        self.assertEqual(report["evaluatedInstallCount"], 1)
        self.assertEqual(report["ignoredInstallCount"], 1)
        self.assertEqual(
            measure(report, "play_reliability")["denominator"],
            0,
        )


if __name__ == "__main__":
    unittest.main()
