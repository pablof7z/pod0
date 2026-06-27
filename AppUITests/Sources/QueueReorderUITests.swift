import XCTest

/// Simulator UI coverage for the `queue-reorder` scenario (#547).
///
/// Drag-to-reorder is not reliably automatable in XCTest because the sheet's
/// pan-gesture recogniser intercepts vertical drags before the List reorder
/// recogniser. Instead, PlayerQueueSheet exposes a named accessibility action
/// "Move to top" (listed first) on each queue row and a context menu with the
/// same action. The test long-presses a row; on iOS 26 this shows the context
/// menu. The test then taps "Move to top" by label (`CONTAINS[c]`). This
/// triggers state.moveQueue → kernelReorderQueue; the kernel matches slot IDs
/// case-insensitively in `reorder_by_slot_ids`.
///
/// The test builds the queue via UI (Queue button on ep2 and ep3 detail views)
/// rather than relying on a pre-seeded queue, because kernel queue projection
/// timing can be nondeterministic on first snapshot. Both ep2 and ep3 are in
/// the standard UITestSeeder seed.
///
/// NSPredicate is not Sendable under Swift 6; all predicates are created fresh
/// at each call site (region-isolation rule — do not extract to a shared `let`).
final class QueueReorderUITests: XCTestCase {

    override func setUp() { super.setUp(); continueAfterFailure = true }

    override func tearDown() {
        XCUIApplication(bundleIdentifier: App.bundleID).terminate()
        super.tearDown()
    }

    // MARK: - queue-reorder (real, via context-menu "Move to top")

    /// Proves queue reorder end-to-end:
    /// 1. Enqueue ep2 and ep3 via their detail views (UI-driven, deterministic).
    /// 2. Play ep1 (then immediately pause to prevent kernel auto-advance) so the mini-player appears.
    /// 3. Open full player → Up Next sheet.
    /// 4. Verify 2 rows: ep2 first, ep3 second.
    /// 5. Long-press ep3 row → iOS 26 shows context menu → tap "Move to top" by label.
    /// 6. Verify order flipped: ep3 first, ep2 second (kernel reorder committed).
    func testQueueReorder() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(2)

        // Step 1: Open show detail.
        guard openFirstPodcastFromHome(app) else {
            XCTFail("queue-reorder: no podcast row on Home"); return
        }
        sleep(1)
        snap(app, "qreorder-00-show-detail")

        // Wait for all episode rows to appear (3 seeded episodes).
        let ep0Row = app.buttons.matching(
            NSPredicate(format: "identifier == 'home-episode-row'")).element(boundBy: 0)
        guard ep0Row.waitForExistence(timeout: 12) else {
            XCTFail("queue-reorder: no episode rows appeared"); return
        }

        // Step 2: Enqueue ep2 (index 1 = second newest).
        let ep2Row = app.buttons.matching(
            NSPredicate(format: "identifier == 'home-episode-row'")).element(boundBy: 1)
        guard ep2Row.waitForExistence(timeout: 6) else {
            XCTFail("queue-reorder: ep2 row not found"); return
        }
        robustTap(ep2Row); sleep(2)
        snap(app, "qreorder-01-ep2-detail")

        let qBtnEp2 = app.buttons.matching(
            NSPredicate(format: "label == 'Queue' OR label == 'Queued'")).firstMatch
        if qBtnEp2.waitForExistence(timeout: 6), qBtnEp2.label == "Queue" {
            qBtnEp2.tap(); sleep(1)
        }
        snap(app, "qreorder-02-ep2-queued")

        let backBtn1 = app.navigationBars.buttons.element(boundBy: 0)
        if backBtn1.waitForExistence(timeout: 4), backBtn1.isHittable {
            backBtn1.tap()
        } else { app.swipeRight() }
        sleep(1)

        // Step 3: Enqueue ep3 (index 2 = third newest).
        let ep3Row = app.buttons.matching(
            NSPredicate(format: "identifier == 'home-episode-row'")).element(boundBy: 2)
        guard ep3Row.waitForExistence(timeout: 6) else {
            XCTFail("queue-reorder: ep3 row not found — seed has only 2 episodes (expected 3)"); return
        }
        robustTap(ep3Row); sleep(2)
        snap(app, "qreorder-03-ep3-detail")

        let qBtnEp3 = app.buttons.matching(
            NSPredicate(format: "label == 'Queue' OR label == 'Queued'")).firstMatch
        if qBtnEp3.waitForExistence(timeout: 6), qBtnEp3.label == "Queue" {
            qBtnEp3.tap(); sleep(1)
        }
        snap(app, "qreorder-04-ep3-queued")

        let backBtn2 = app.navigationBars.buttons.element(boundBy: 0)
        if backBtn2.waitForExistence(timeout: 4), backBtn2.isHittable {
            backBtn2.tap()
        } else { app.swipeRight() }
        sleep(1)

        // Step 4: Play ep1 (index 0 = newest) to activate the mini-player.
        let ep1Row = app.buttons.matching(
            NSPredicate(format: "identifier == 'home-episode-row'")).firstMatch
        guard ep1Row.waitForExistence(timeout: 6) else {
            XCTFail("queue-reorder: ep1 row not found"); return
        }
        robustTap(ep1Row); sleep(2)

        let playBtn = app.buttons.matching(
            NSPredicate(format: "label == 'Play' OR label CONTAINS[c] 'resume'")).firstMatch
        guard playBtn.waitForExistence(timeout: 8) else {
            XCTFail("queue-reorder: Play button not found on ep1 detail"); return
        }
        playBtn.tap(); sleep(1)

        // Pause ep1 immediately after tapping Play. The kernel auto-advances
        // to ep2 when ep1's audio URL (a non-existent test host) times out.
        // Pausing prevents the kernel from loading/timing-out and auto-advancing
        // to ep2, which would remove ep2 from the queue mid-test.
        // NSPredicate not Sendable — fresh at each call site.
        let pauseBtn = app.buttons.matching(
            NSPredicate(format: "label == 'Pause'")).firstMatch
        if pauseBtn.waitForExistence(timeout: 5) {
            pauseBtn.tap(); sleep(1)
        }

        guard openFullPlayerFromMiniPlayer(app) else {
            XCTFail("queue-reorder: mini-player did not appear"); return
        }
        sleep(1)

        // Step 5: Open More options → Up Next.
        let moreBtn = app.buttons["More options"]
        guard moreBtn.waitForExistence(timeout: 5) else {
            XCTFail("queue-reorder: 'More options' not found in full player"); return
        }
        moreBtn.tap(); sleep(1)
        snap(app, "qreorder-05-more-menu")

        let upNextBtn = app.buttons.matching(
            NSPredicate(format: "identifier == 'player-queue-chip' OR label == 'Up Next'")).firstMatch
        guard upNextBtn.waitForExistence(timeout: 4) else {
            dumpTree(app, "qreorder-NOUPNEXT-tree")
            XCTFail("queue-reorder: 'Up Next' button not found in More options"); return
        }
        upNextBtn.tap(); sleep(2)
        snap(app, "qreorder-06-queue-sheet")
        dumpTree(app, "qreorder-06-tree")

        // Step 6: Verify ≥ 2 queue rows.
        // Row predicate: starts with 'queue-row-', excludes swipe-delete handles
        // ('queue-row-remove-') and context-menu buttons ('queue-row-ctx-').
        // NSPredicate not Sendable — fresh at each call site.
        let firstRowCheck = app.buttons.matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'queue-row-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-remove-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-ctx-'"
            )).firstMatch
        guard firstRowCheck.waitForExistence(timeout: 8) else {
            XCTFail("queue-reorder: no queue-row-* buttons — ep2 and ep3 must have been enqueued"); return
        }

        let rowCount = app.buttons.matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'queue-row-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-remove-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-ctx-'"
            )).count
        guard rowCount >= 2 else {
            snap(app, "qreorder-TOOFEWITEMS")
            dumpTree(app, "qreorder-TOOFEWITEMS-tree")
            XCTFail("queue-reorder: expected ≥ 2 queue rows but found \(rowCount). " +
                    "Both ep2 and ep3 must be enqueued before playing ep1."); return
        }

        // Capture identifiers BEFORE reorder.
        let beforeAllRows = app.buttons.matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'queue-row-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-remove-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-ctx-'"
            ))
        let row0IdBefore = beforeAllRows.element(boundBy: 0).identifier
        let row1IdBefore = beforeAllRows.element(boundBy: 1).identifier
        snap(app, "qreorder-07-before-reorder")

        // Step 7: Tap the row's explicit Move to top control. This avoids the
        // brittle XCTest long-press/context-menu path and keeps the row tap
        // action from accidentally playing/removing an item.
        let moveTopButton = app.buttons["queue-move-top-\(row1IdBefore.replacingOccurrences(of: "queue-row-", with: ""))"]
        guard moveTopButton.waitForExistence(timeout: 5) else {
            snap(app, "qreorder-NOMOVEBTN")
            dumpTree(app, "qreorder-NOMOVEBTN-tree")
            XCTFail("queue-reorder: Move to top button not found for second row \(row1IdBefore)")
            return
        }
        moveTopButton.tap()

        // Wait for the kernel round-trip: state.moveQueue → kernelReorderQueue
        // → snapshot → onQueueFromKernel → state.queue update → view re-render.
        // 4 seconds for reliability on slow simulator builds.
        sleep(4)
        snap(app, "qreorder-09-after-reorder")
        dumpTree(app, "qreorder-09-tree")

        // Step 8: Verify the order flipped and both rows are still present.
        let afterFirstRow = app.buttons.matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'queue-row-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-remove-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-ctx-'"
            )).firstMatch
        guard afterFirstRow.waitForExistence(timeout: 5) else {
            XCTFail("queue-reorder: queue rows disappeared after reorder"); return
        }

        let afterRowCount = app.buttons.matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'queue-row-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-remove-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-ctx-'"
            )).count
        guard afterRowCount >= 2 else {
            snap(app, "qreorder-LOST-ITEM")
            dumpTree(app, "qreorder-LOST-ITEM-tree")
            let remainingId = afterFirstRow.identifier
            let ctxBtns = app.buttons.matching(
                NSPredicate(format: "identifier BEGINSWITH 'queue-row-ctx-'")).count
            let rmBtns = app.buttons.matching(
                NSPredicate(format: "identifier BEGINSWITH 'queue-row-remove-'")).count
            let allQRowBtns = app.buttons.matching(
                NSPredicate(format: "identifier BEGINSWITH 'queue-row-'")).count
            let ctxMenuBtns = app.buttons.matching(
                NSPredicate(format: "label CONTAINS[c] 'move to top' OR label CONTAINS[c] 'remove from'")).count
            XCTFail("queue-reorder: only \(afterRowCount) row(s) after reorder. " +
                    "row0Before=\(row0IdBefore) row1Before=\(row1IdBefore) remaining=\(remainingId). " +
                    "allQRowBtns=\(allQRowBtns) ctxBtns=\(ctxBtns) rmBtns=\(rmBtns) " +
                    "ctxMenuVisible=\(ctxMenuBtns)"); return
        }

        let row0IdAfter = app.buttons.matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'queue-row-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-remove-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-ctx-'"
            )).element(boundBy: 0).identifier
        let row1IdAfter = app.buttons.matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'queue-row-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-remove-' " +
                        "AND NOT identifier BEGINSWITH 'queue-row-ctx-'"
            )).element(boundBy: 1).identifier

        // After "Move to top" on the former second row, the former second becomes first.
        XCTAssertEqual(
            row0IdAfter, row1IdBefore,
            "FAIL queue-reorder: first row after 'Move to top' should be former second row " +
            "(\(row1IdBefore)) but got \(row0IdAfter). " +
            "state.moveQueue + kernelReorderQueue may not have committed."
        )
        XCTAssertEqual(
            row1IdAfter, row0IdBefore,
            "FAIL queue-reorder: second row after 'Move to top' should be former first row " +
            "(\(row0IdBefore)) but got \(row1IdAfter)."
        )
        snap(app, "qreorder-10-verified")
    }

    // MARK: - queue-reachable smoke check

    /// Confirms the queue sheet is reachable and shows seeded rows.
    /// Does not test reorder (see `testQueueReorder`).
    func testQueueSheetIsReachableAndShowsRows() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(2)

        // Enqueue ep2 via its detail view.
        guard openFirstPodcastFromHome(app) else {
            XCTFail("queue-smoke: no podcast row"); return
        }
        let allEpRows = app.buttons.matching(
            NSPredicate(format: "identifier == 'home-episode-row'"))
        guard allEpRows.element(boundBy: 0).waitForExistence(timeout: 8) else {
            XCTFail("queue-smoke: no episode rows in show detail"); return
        }
        let ep2Row = allEpRows.element(boundBy: 1)
        if ep2Row.waitForExistence(timeout: 4) {
            robustTap(ep2Row); sleep(2)
            let qBtn = app.buttons.matching(
                NSPredicate(format: "label == 'Queue'")).firstMatch
            if qBtn.waitForExistence(timeout: 5) { qBtn.tap(); sleep(1) }
            let backBtn = app.navigationBars.buttons.element(boundBy: 0)
            if backBtn.waitForExistence(timeout: 3), backBtn.isHittable { backBtn.tap() }
            sleep(1)
        }

        // Play ep1 and open the full player.
        let ep1Row = app.buttons.matching(
            NSPredicate(format: "identifier == 'home-episode-row'")).firstMatch
        if ep1Row.waitForExistence(timeout: 5) { robustTap(ep1Row); sleep(2) }
        let playBtn = app.buttons.matching(
            NSPredicate(format: "label == 'Play' OR label CONTAINS[c] 'resume'")).firstMatch
        if playBtn.waitForExistence(timeout: 8) { playBtn.tap(); sleep(2) }
        guard openFullPlayerFromMiniPlayer(app) else {
            XCTFail("queue-smoke: mini-player did not appear"); return
        }
        sleep(1)

        // Open More options → Up Next.
        let moreBtn = app.buttons["More options"]
        guard moreBtn.waitForExistence(timeout: 5) else {
            XCTFail("queue-smoke: 'More options' not found"); return
        }
        moreBtn.tap(); sleep(1)
        snap(app, "qsmoke-01-more-menu")

        let upNextBtn = app.buttons.matching(
            NSPredicate(format: "identifier == 'player-queue-chip' OR label == 'Up Next'")).firstMatch
        guard upNextBtn.waitForExistence(timeout: 4) else {
            dumpTree(app, "qsmoke-NOUPNEXT-tree")
            XCTFail("queue-smoke: 'Up Next' button not found in More options"); return
        }
        upNextBtn.tap(); sleep(2)
        snap(app, "qsmoke-02-queue-sheet")

        // Assert queue rows are visible.
        let firstRow = app.buttons.matching(
            NSPredicate(
                format: "identifier BEGINSWITH 'queue-row-' AND NOT identifier BEGINSWITH 'queue-row-remove-'"
            )).firstMatch
        XCTAssertTrue(
            firstRow.waitForExistence(timeout: 5),
            "FAIL queue-smoke: queue-row-* button not found — ep2 may not have been enqueued " +
            "or the kernel projection is empty"
        )
        snap(app, "qsmoke-03-rows-present")
    }
}
