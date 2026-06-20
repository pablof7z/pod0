import XCTest

/// P1 queue scenario: end-to-end removal of a queued episode.
///
/// Separated from P1SettingsUITests to keep both files under the 500-line
/// hard limit. All shared helpers live in UITestSupport.swift.
final class P1QueueUITests: XCTestCase {
    override func setUp() { super.setUp(); continueAfterFailure = true }

    // MARK: - P1: queue-remove-item

    /// Proves queue removal end-to-end by driving the app:
    ///
    /// 1. Open the show detail (home → podcast row).
    /// 2. Open ep2 detail (second episode row) and tap "Queue" to enqueue it
    ///    via the Rust kernel. Then navigate back.
    /// 3. Open ep1 detail, tap Play, and open the full player.
    /// 4. Open More-options → "Up Next" to present `PlayerQueueSheet`.
    /// 5. PROVE ep2's queued row is visible — the test FAILS here if absent
    ///    (no silent NOQUEUECELL pass). Capture the row's identifier.
    /// 6. Long-press the row to trigger the accessibility action / context menu
    ///    removal, then assert that SAME row identifier is gone and
    ///    "Nothing queued" appears (kernel projection emptied the list).
    func testP1_QueueRemoveItem() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(2)

        // Step 1 — Open the show detail.
        guard openFirstPodcastFromHome(app) else {
            XCTFail("queue-remove: no podcast row on home"); return
        }
        snap(app, "q-remove-00-show-detail")
        dumpTree(app, "q-remove-00-tree")

        // Step 2 — Open ep2 detail (the SECOND episode row, boundBy: 1).
        // Episodes are sorted newest-first: ep1 (2026-05-01) is at index 0,
        // ep2 (2026-04-01) is at index 1.
        let allEpRows = app.buttons.matching(
            NSPredicate(format: "identifier == 'home-episode-row'"))
        guard allEpRows.element(boundBy: 0).waitForExistence(timeout: 8) else {
            snap(app, "q-remove-NOEPROWS")
            dumpTree(app, "q-remove-NOEPROWS-tree")
            XCTFail("queue-remove: no episode rows found in show detail"); return
        }
        let ep2Row = allEpRows.element(boundBy: 1)
        guard ep2Row.waitForExistence(timeout: 5) else {
            snap(app, "q-remove-NOEP2ROW")
            XCTFail("queue-remove: ep2 row (boundBy:1) not found"); return
        }
        robustTap(ep2Row)
        sleep(2)
        snap(app, "q-remove-01-ep2-detail")
        dumpTree(app, "q-remove-01-ep2-tree")

        // Step 3 — Tap "Queue" on ep2 detail to enqueue it.
        // The button is in EpisodeDetailHeroView's action row, labelled "Queue"
        // (or "Queued" if already in queue). It dispatches kernelEnqueueLast
        // through PlaybackState.enqueue → the Rust queue handler.
        let queueBtnPred = NSPredicate(format: "label == 'Queue' OR label == 'Queued'")
        let queueBtn = app.buttons.matching(queueBtnPred).firstMatch
        guard queueBtn.waitForExistence(timeout: 6) else {
            snap(app, "q-remove-NOQUEUEBTN")
            dumpTree(app, "q-remove-NOQUEUEBTN-tree")
            XCTFail("queue-remove: 'Queue' button not found on ep2 detail view"); return
        }
        if queueBtn.label == "Queue" {
            queueBtn.tap(); sleep(1)
        }
        snap(app, "q-remove-02-ep2-queued")

        // Step 4 — Navigate back to show detail (ep2 detail was pushed).
        let backBtn = app.navigationBars.buttons.element(boundBy: 0)
        if backBtn.waitForExistence(timeout: 4), backBtn.isHittable {
            backBtn.tap()
        } else {
            app.swipeRight()
        }
        sleep(1)
        snap(app, "q-remove-03-back-to-show")

        // Step 5 — Open ep1 detail (first episode row) and tap Play.
        let ep1Row = app.buttons.matching(
            NSPredicate(format: "identifier == 'home-episode-row'")).firstMatch
        if ep1Row.waitForExistence(timeout: 6) {
            robustTap(ep1Row)
        }
        sleep(2)
        snap(app, "q-remove-04-ep1-detail")
        // The play button may be labelled "Play", "Play again", or "Resume"
        // depending on whether ep1 has been played in a previous test run.
        let playBtnPred = NSPredicate(
            format: "label == 'Play' OR label == 'Play again' OR label CONTAINS[c] 'resume'")
        let playBtn = app.buttons.matching(playBtnPred).firstMatch
        if playBtn.waitForExistence(timeout: 8) {
            playBtn.tap(); sleep(2)
        }
        snap(app, "q-remove-05-ep1-playing")

        // Step 6 — Open the full player via the mini-player bar.
        guard openFullPlayerFromMiniPlayer(app) else {
            snap(app, "q-remove-NOMINIPLAYER")
            XCTFail("queue-remove: mini-player did not appear after tapping Play on ep1")
            return
        }
        sleep(2)
        snap(app, "q-remove-06-full-player")

        // Step 7 — Open More-options and tap "Up Next".
        let moreBtn = app.buttons["More options"]
        guard moreBtn.waitForExistence(timeout: 5) else {
            snap(app, "q-remove-NOMORE")
            XCTFail("queue-remove: 'More options' button not found in full player"); return
        }
        moreBtn.tap(); sleep(1)
        snap(app, "q-remove-07-more-menu")

        let upNextById = app.buttons.matching(
            NSPredicate(format: "identifier == 'player-queue-chip'")).firstMatch
        let upNextByLabel = app.buttons.matching(
            NSPredicate(format: "label == 'Up Next'")).firstMatch
        let upNextBtn: XCUIElement = upNextById.waitForExistence(timeout: 4)
            ? upNextById : upNextByLabel
        guard upNextBtn.waitForExistence(timeout: 4) else {
            snap(app, "q-remove-NOUPNEXT")
            dumpTree(app, "q-remove-NOUPNEXT-tree")
            XCTFail("queue-remove: 'Up Next' button not found in More-options menu"); return
        }
        upNextBtn.tap(); sleep(2)
        snap(app, "q-remove-08-queue-sheet")
        dumpTree(app, "q-remove-08-tree")

        // Step 8 — PROVE a removable queued cell is visible in the queue sheet.
        //
        // iOS 26 SwiftUI `List` does not expose UITableView in the accessibility
        // tree via `app.tables`. Locate the queue row button by its stable
        // accessibility identifier prefix `queue-row-` set in PlayerQueueSheet.
        //
        // This is a hard assertion: if the row isn't found the test FAILS —
        // no silent NOQUEUECELL pass. We capture the row identifier NOW so the
        // final assertion can prove that SAME identifier disappeared (not just
        // that the slot count dropped).
        let queueRowPred = NSPredicate(
            format: "identifier BEGINSWITH 'queue-row-' AND NOT identifier BEGINSWITH 'queue-row-remove-'")
        let queueRowQuery = app.buttons.matching(queueRowPred)
        guard queueRowQuery.element(boundBy: 0).waitForExistence(timeout: 5) else {
            snap(app, "q-remove-NOQUEUECELL")
            dumpTree(app, "q-remove-NOQUEUECELL-tree")
            XCTFail(
                "FAIL queue-remove-item: queue sheet is open but no queue-row-* button found" +
                " — kernel did not enqueue ep2 or sheet is empty")
            return
        }
        // Capture the concrete identifier of the first queue row so the post-
        // removal assertion targets this exact row (not a different slot or a
        // false-positive match elsewhere in the tree).
        let capturedRowID = queueRowQuery.element(boundBy: 0).identifier
        let capturedRow = app.buttons[capturedRowID]
        snap(app, "q-remove-09-before-remove")

        // Step 9 — Remove ep2 from the queue via its accessibility action /
        // context menu.
        //
        // On iOS 26, a SwiftUI sheet at medium detent has a pan-gesture
        // recogniser that intercepts ALL left-drag gestures before the List's
        // swipe-action recogniser, dismissing the sheet instead of revealing
        // the swipe action. Instead:
        //   a. Long-press (1.5s) triggers `.accessibilityAction(named: "Remove")`
        //      directly on iOS 26 — the queue row button disappears immediately.
        //   b. If the a11y action fires and the queue is already empty, done.
        //   c. Otherwise wait for the context menu "Remove from queue" button
        //      and tap it.
        //   d. Last-resort swipe fallback (goes through removeFromQueue → kernel).
        capturedRow.press(forDuration: 1.5)
        sleep(1)
        snap(app, "q-remove-10-context-menu")
        dumpTree(app, "q-remove-10-tree")

        let nothingQueued = app.staticTexts.matching(
            NSPredicate(format: "label == 'Nothing queued'")).firstMatch
        var removalSucceeded = false

        // Check if the a11y action already fired (row gone immediately).
        if nothingQueued.waitForExistence(timeout: 1) {
            removalSucceeded = true
        } else {
            let ctxRemovePred = NSPredicate(
                format: "label == 'Remove from queue' OR label CONTAINS[c] 'remove'")
            let ctxRemoveBtn = app.buttons.matching(ctxRemovePred).firstMatch
            if ctxRemoveBtn.waitForExistence(timeout: 4) {
                ctxRemoveBtn.tap()
                removalSucceeded = true
            } else {
                // Dismiss stale context menu and try swipe as last resort.
                app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.1)).tap()
                sleep(1)
                if capturedRow.exists {
                    capturedRow.swipeLeft()
                    sleep(1)
                    let swipeRemove = app.buttons.matching(
                        NSPredicate(format: "label == 'Remove' OR label == 'Delete'")).firstMatch
                    if swipeRemove.waitForExistence(timeout: 3) {
                        swipeRemove.tap()
                        removalSucceeded = true
                    }
                } else {
                    // Already gone via the a11y action — the capturedRow simply
                    // doesn't exist anymore without us catching it in the 1s window.
                    removalSucceeded = true
                }
            }
        }

        guard removalSucceeded else {
            snap(app, "q-remove-NOREMOVEBTN")
            dumpTree(app, "q-remove-NOREMOVEBTN-tree")
            XCTFail(
                "FAIL queue-remove-item: context-menu Remove not found and swipe fallback failed")
            return
        }

        // Give the kernel projection time to arrive and update the queue.
        // The remove action dispatches to the kernel; `onQueueFromKernel` →
        // `applyKernelQueue` is the sole writer to `PlaybackState.queue` (a pure
        // read-only projection). "Nothing queued" appears only when the kernel
        // projection has an empty list.
        sleep(2)
        snap(app, "q-remove-11-after-delete")
        dumpTree(app, "q-remove-11-tree")

        // Step 10 — Assert: the EXACT captured row is gone and the queue is empty.
        //
        // Using the captured identifier (not a dynamic query) guarantees the
        // assertion proves THIS row disappeared — not that the query simply
        // returned no match at the moment (which could happen if the sheet
        // were dismissed). `nothingQueued` requires the kernel projection to
        // have delivered an empty queue snapshot, ruling out a spurious match.
        let rowStillVisible = capturedRow.waitForExistence(timeout: 3)
        let queueNowEmpty = nothingQueued.waitForExistence(timeout: 5)
        XCTAssertFalse(
            rowStillVisible,
            "FAIL queue-remove-item: captured row '\(capturedRowID)' still present" +
            " — ep2 was not dequeued from the kernel projection")
        XCTAssertTrue(
            queueNowEmpty,
            "FAIL queue-remove-item: 'Nothing queued' not shown after removal" +
            " — kernel projection did not deliver empty queue snapshot")
    }
}
