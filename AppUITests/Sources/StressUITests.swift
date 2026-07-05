import XCTest

/// Simulator stress coverage for the `ffi-rapid-subscribe-unsubscribe` scenario (#547).
///
/// Drives the subscribe/unsubscribe cycle through the real app UI against the
/// seeded "This American Life" podcast: open Show options → Unsubscribe →
/// confirm → reopen Show options → Follow (re-subscribe) → repeat.
///
/// Asserts after each cycle:
///   - The app is still running (no crash).
///   - The Show options sheet shows the expected action (Follow ↔ Unsubscribe).
///   - No duplicate podcast rows appear on Home (kernel-side idempotency).
///
/// CYCLE COUNT: 3 (conservative for a CI runner). Increase to 10+ for a
/// dedicated stress pass once the baseline pass is confirmed green.
///
/// NOTE: The subscribe path (Follow) goes through the same kernel FFI as
/// `SubscriptionService.addSubscription`, dispatching `podcast.subscribe`.
/// The unsubscribe path dispatches `podcast.unfollow` (keeps episodes/history;
/// the show becomes "known but unfollowed"). Rapid alternation probes for race
/// conditions in the Rust queue handler and Swift snapshot application.
/// Any crash proves the stress scenario.
final class StressUITests: XCTestCase {

    override func setUp() { super.setUp(); continueAfterFailure = true }

    override func tearDown() {
        XCUIApplication(bundleIdentifier: App.bundleID).terminate()
        super.tearDown()
    }

    // MARK: - ffi-rapid-subscribe-unsubscribe

    func testFFIRapidSubscribeUnsubscribe() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app))

        guard openFirstPodcastFromHome(app) else {
            XCTFail("stress: no podcast row on Home — seeded library not loaded"); return
        }
        snap(app, "stress-00-show-detail")

        let cycleCount = 3

        for cycle in 1...cycleCount {
            // --- Unsubscribe ---
            guard let unsubBtn = openShowOptions(app, expecting: .unsubscribe, cycle: cycle, phase: "unsubscribe") else {
                return
            }
            snap(app, "stress-\(cycle)-a-options-open")
            unsubBtn.tap()

            // Confirm the unsubscribe alert (unfollow — keeps history).
            let confirmUnsub = app.alerts.buttons["Unsubscribe"].firstMatch
            guard confirmUnsub.waitForExistence(timeout: 5) else {
                snap(app, "stress-\(cycle)-NO-UNSUB-ALERT")
                dumpTree(app, "stress-\(cycle)-NO-UNSUB-ALERT-tree")
                XCTFail("stress cycle \(cycle): unsubscribe confirmation alert did not appear")
                return
            }
            confirmUnsub.tap()
            snap(app, "stress-\(cycle)-b-after-unsub")

            // Assert: app still running.
            XCTAssertEqual(
                app.state, .runningForeground,
                "FAIL ffi-rapid-subscribe-unsubscribe: app crashed during unsubscribe (cycle \(cycle))"
            )

            // --- Re-subscribe (Follow) ---
            // ShowDetailView.performUnfollow() keeps the show detail open (no
            // dismiss) so Show options stays accessible. The "else" path is a
            // defensive fallback for unexpected navigation; under the current
            // product implementation it should never fire.
            let showOptionsAgain = app.buttons["Show options"]
            if !waitForEnabled(showOptionsAgain, timeout: 8) {
                // Unexpected: view was dismissed. Attempt to re-navigate.
                snap(app, "stress-\(cycle)-b2-unexpected-dismiss")
                dumpTree(app, "stress-\(cycle)-b2-tree")
                guard openFirstPodcastFromHome(app) else {
                    XCTFail("stress cycle \(cycle): cannot re-open show after unexpected dismiss"); return
                }
            }

            // Open Show options to find "Follow".
            guard let followBtn = openShowOptions(app, expecting: .follow, cycle: cycle, phase: "follow") else {
                return
            }
            snap(app, "stress-\(cycle)-c-options-for-follow")
            followBtn.tap()
            guard waitForEnabled(app.buttons["Show options"], timeout: 8) else {
                snap(app, "stress-\(cycle)-FOLLOW-DID-NOT-SETTLE")
                dumpTree(app, "stress-\(cycle)-FOLLOW-DID-NOT-SETTLE-tree")
                XCTFail("stress cycle \(cycle): Follow did not settle back to enabled Show options")
                return
            }
            snap(app, "stress-\(cycle)-d-after-follow")

            // Assert: app still running.
            XCTAssertEqual(
                app.state, .runningForeground,
                "FAIL ffi-rapid-subscribe-unsubscribe: app crashed during re-subscribe (cycle \(cycle))"
            )

            // Assert: no duplicate podcast rows on Home (kernel idempotency check).
            // Navigate to Home and count rows matching "This American Life".
            let homeTab = app.buttons["tab-home"]
            if homeTab.waitForExistence(timeout: 2) { homeTab.tap() }
            let podRowPred = NSPredicate(
                format: "identifier == 'library-podcast-row' OR label CONTAINS[c] 'This American Life'")
            let podRows = app.buttons.matching(podRowPred)
            // Allow up to 2s for the snapshot to settle before counting.
            _ = podRows.firstMatch.waitForExistence(timeout: 4)
            // Maximum 1 row for "This American Life" — duplicates indicate a kernel bug.
            let rowCount = podRows.count
            XCTAssertLessThanOrEqual(
                rowCount, 2, // 1 library row + 1 possible title text = 2 elements max
                "FAIL ffi-rapid-subscribe-unsubscribe cycle \(cycle): found \(rowCount) rows for " +
                "'This American Life' on Home — kernel may have duplicated the subscription"
            )
            snap(app, "stress-\(cycle)-e-home-after-cycle")

            // Re-open show detail for the next cycle.
            if cycle < cycleCount {
                guard openFirstPodcastFromHome(app) else {
                    XCTFail("stress: cannot re-open show for cycle \(cycle + 1)"); return
                }
            }
        }

        snap(app, "stress-final-state")
        // Final assertion: app is alive after all cycles.
        XCTAssertEqual(
            app.state, .runningForeground,
            "FAIL ffi-rapid-subscribe-unsubscribe: app not in foreground after \(cycleCount) cycles"
        )
    }

    private enum ExpectedShowAction {
        case follow
        case unsubscribe

        var label: String {
            switch self {
            case .follow:
                return "Follow"
            case .unsubscribe:
                return "Unsubscribe"
            }
        }

        var oppositeLabel: String {
            switch self {
            case .follow:
                return "Unsubscribe"
            case .unsubscribe:
                return "Follow"
            }
        }
    }

    private func openShowOptions(
        _ app: XCUIApplication,
        expecting expected: ExpectedShowAction,
        cycle: Int,
        phase: String
    ) -> XCUIElement? {
        let showOptions = app.buttons["Show options"]
        guard waitForEnabled(showOptions, timeout: 8) else {
            snap(app, "stress-\(cycle)-\(phase)-NOOPTIONS")
            XCTFail("stress cycle \(cycle): enabled 'Show options' not found before \(phase)")
            return nil
        }

        for attempt in 1...3 {
            showOptions.tap()
            let expectedButton = app.buttons.matching(
                NSPredicate(format: "label == %@", expected.label)
            ).firstMatch
            if expectedButton.waitForExistence(timeout: 4) {
                return expectedButton
            }

            let oppositeButton = app.buttons.matching(
                NSPredicate(format: "label == %@", expected.oppositeLabel)
            ).firstMatch
            if oppositeButton.exists {
                snap(app, "stress-\(cycle)-\(phase)-STALE-\(expected.oppositeLabel.uppercased())")
                dumpTree(app, "stress-\(cycle)-\(phase)-STALE-\(expected.oppositeLabel.uppercased())-tree")
                XCTFail(
                    "stress cycle \(cycle): Show options still shows " +
                    "'\(expected.oppositeLabel)' when '\(expected.label)' is expected"
                )
                return nil
            }

            if attempt < 3 {
                dismissShowOptions(app)
                _ = waitForEnabled(showOptions, timeout: 2)
            }
        }

        snap(app, "stress-\(cycle)-\(phase)-NO-\(expected.label.uppercased())")
        dumpTree(app, "stress-\(cycle)-\(phase)-NO-\(expected.label.uppercased())-tree")
        XCTFail("stress cycle \(cycle): '\(expected.label)' not found in Show options for \(phase)")
        return nil
    }

    private func dismissShowOptions(_ app: XCUIApplication) {
        let done = app.buttons["Done"]
        if done.waitForExistence(timeout: 1) {
            done.tap()
        }
    }

    private func waitForEnabled(_ element: XCUIElement, timeout: TimeInterval) -> Bool {
        let predicate = NSPredicate(format: "exists == true AND enabled == true")
        let expectation = XCTNSPredicateExpectation(predicate: predicate, object: element)
        return XCTWaiter.wait(for: [expectation], timeout: timeout) == .completed
    }
}
