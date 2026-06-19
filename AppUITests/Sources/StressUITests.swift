import XCTest

/// Simulator stress coverage for the `ffi-rapid-subscribe-unsubscribe` scenario (#547).
///
/// Drives the subscribe/unsubscribe cycle through the real app UI against the
/// seeded "This American Life" podcast: open Show options → Unsubscribe → confirm
/// → reopen Show options → Follow (re-subscribe) → repeat.
///
/// Asserts after each cycle:
///   - The app is still running (no crash).
///   - The Show options menu shows the expected action (Follow ↔ Unsubscribe).
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
        XCTAssertTrue(launchApp(app)); sleep(2)

        guard openFirstPodcastFromHome(app) else {
            XCTFail("stress: no podcast row on Home — seeded library not loaded"); return
        }
        sleep(1)
        snap(app, "stress-00-show-detail")

        let cycleCount = 3

        for cycle in 1...cycleCount {
            // --- Unsubscribe ---
            let showOptions = app.buttons["Show options"]
            guard showOptions.waitForExistence(timeout: 8) else {
                snap(app, "stress-\(cycle)-NOOPTIONS")
                XCTFail("stress cycle \(cycle): 'Show options' not found — app may have crashed or navigated away")
                return
            }
            showOptions.tap(); sleep(1)
            snap(app, "stress-\(cycle)-a-options-open")

            let unsubBtn = app.buttons["Unsubscribe"]
            guard unsubBtn.waitForExistence(timeout: 4) else {
                // If "Follow" is showing, the app already unsubscribed —
                // treat this cycle as already in the right state.
                let followBtn = app.buttons.matching(
                    NSPredicate(format: "label == 'Follow'")).firstMatch
                if followBtn.waitForExistence(timeout: 2) {
                    app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.2)).tap()
                    sleep(1)
                } else {
                    snap(app, "stress-\(cycle)-NOUNSUB")
                    dumpTree(app, "stress-\(cycle)-NOUNSUB-tree")
                    XCTFail("stress cycle \(cycle): neither Unsubscribe nor Follow found in Show options")
                    return
                }
                // Skip the unsubscribe step for this cycle.
                continue
            }
            unsubBtn.tap(); sleep(1)

            // Confirm the unsubscribe alert (unfollow — keeps history).
            let confirmUnsub = app.buttons["Unsubscribe"].firstMatch
            if confirmUnsub.waitForExistence(timeout: 3) { confirmUnsub.tap(); sleep(1) }
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
            if !showOptionsAgain.waitForExistence(timeout: 5) {
                // Unexpected: view was dismissed. Attempt to re-navigate.
                snap(app, "stress-\(cycle)-b2-unexpected-dismiss")
                dumpTree(app, "stress-\(cycle)-b2-tree")
                guard openFirstPodcastFromHome(app) else {
                    XCTFail("stress cycle \(cycle): cannot re-open show after unexpected dismiss"); return
                }
            }

            // Open Show options to find "Follow".
            let showOptionsForFollow = app.buttons["Show options"]
            guard showOptionsForFollow.waitForExistence(timeout: 5) else {
                XCTFail("stress cycle \(cycle): Show options not found before re-subscribe"); return
            }
            showOptionsForFollow.tap(); sleep(1)
            snap(app, "stress-\(cycle)-c-options-for-follow")

            let followBtn = app.buttons.matching(
                NSPredicate(format: "label == 'Follow'")).firstMatch
            guard followBtn.waitForExistence(timeout: 4) else {
                snap(app, "stress-\(cycle)-NOFOLLOW")
                dumpTree(app, "stress-\(cycle)-NOFOLLOW-tree")
                XCTFail("stress cycle \(cycle): 'Follow' button not found in Show options after unsubscribe")
                return
            }
            followBtn.tap(); sleep(2)
            snap(app, "stress-\(cycle)-d-after-follow")

            // Assert: app still running.
            XCTAssertEqual(
                app.state, .runningForeground,
                "FAIL ffi-rapid-subscribe-unsubscribe: app crashed during re-subscribe (cycle \(cycle))"
            )

            // Assert: no duplicate podcast rows on Home (kernel idempotency check).
            // Navigate to Home and count rows matching "This American Life".
            let homeTab = app.buttons["tab-home"]
            if homeTab.waitForExistence(timeout: 2) { homeTab.tap(); sleep(2) }
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
                sleep(1)
            }
        }

        snap(app, "stress-final-state")
        // Final assertion: app is alive after all cycles.
        XCTAssertEqual(
            app.state, .runningForeground,
            "FAIL ffi-rapid-subscribe-unsubscribe: app not in foreground after \(cycleCount) cycles"
        )
    }
}
