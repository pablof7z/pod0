import XCTest

/// Core P0 playback journeys, driven deterministically on the physical device
/// against the real subscribed library. Captures screenshots + tree dumps as
/// evidence and asserts the observable + (where measurable) performance criteria
/// from docs/plan/qa-scenario-tests.md.
final class CoreJourneyUITests: XCTestCase {
    override func setUp() { super.setUp(); continueAfterFailure = true }

    /// Tap the first subscribed podcast (visible on Home) -> first episode detail. Returns true on success.
    @discardableResult
    private func openFirstEpisodeDetail(_ app: XCUIApplication) -> Bool {
        // The Home tab shows podcast rows with identifier 'library-podcast-row' (a Button in SwiftUI).
        let podcastRowBtn = app.buttons.matching(
            NSPredicate(format: "identifier == 'library-podcast-row'")).firstMatch
        let target: XCUIElement
        if podcastRowBtn.waitForExistence(timeout: 6) {
            target = podcastRowBtn
        } else {
            // Fallback: tap by podcast title text.
            target = staticTextContaining(app, "This American Life")
        }
        guard robustTap(target) else { return false }
        // Wait for show detail (Episodes header or at least some episode cells).
        _ = staticTextContaining(app, "Episodes").waitForExistence(timeout: 8)
        // Episode rows are cells after the show-header cell and the "Episodes"
        // label cell. cell[2] is the first real episode.
        let cells = app.cells
        if cells.count > 2 {
            robustTap(cells.element(boundBy: 2))
        } else {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.33)).tap()
        }
        // Episode detail is identified by the Play/Queue/Download triad.
        return app.buttons["Play"].waitForExistence(timeout: 8)
            || app.buttons["Queue"].waitForExistence(timeout: 4)
    }

    /// P0-03 — Tapping Play starts real audio (Pause appears AND position advances).
    func testP0_03_PlayStartsAudio() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app), "launch")
        sleep(1)
        XCTAssertTrue(openFirstEpisodeDetail(app), "could not reach episode detail")
        snap(app, "P0-03-01-episode-detail")
        dumpTree(app, "P0-03-01-episode-tree")

        let playBtn = app.buttons["Play"]
        XCTAssertTrue(playBtn.waitForExistence(timeout: 5), "no Play button on episode detail")

        let t0 = Date()
        playBtn.tap()

        // Playback proof #1: a Pause control appears somewhere (mini-player or detail).
        let pause = app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'pause'")).firstMatch
        let startedUI = pause.waitForExistence(timeout: 6)
        let tapToPauseUI = Date().timeIntervalSince(t0)
        snap(app, "P0-03-02-after-play")
        dumpTree(app, "P0-03-02-after-play-tree")

        // Open the full player by tapping the mini-player strip (bottom).
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.92)).tap()
        sleep(1)
        snap(app, "P0-03-03-full-player")
        dumpTree(app, "P0-03-03-full-player-tree")

        // Playback proof #2: an elapsed-time label advances over ~4s of wall time.
        let elapsed1 = currentTimeLabels(app)
        sleep(4)
        let elapsed2 = currentTimeLabels(app)
        snap(app, "P0-03-04-after-4s")

        XCTAssertTrue(startedUI, "FAIL P0-03: no Pause control appeared within 6s of tapping Play — playback did not start")
        XCTAssertLessThan(tapToPauseUI, 3.0, "PERF P0-03: tap-to-playback-UI \(tapToPauseUI)s exceeds 3.0s budget")
        XCTAssertNotEqual(elapsed1, elapsed2,
            "FAIL P0-03: no time label advanced over 4s (labels1=\(elapsed1) labels2=\(elapsed2)) — audio is not actually progressing")
    }

    /// Play ~35s (crosses the kernel's 30s Playing checkpoint so a disk flush
    /// happens independent of the pause-flush), then pause. Returns once paused.
    private func playPastCheckpointAndPause(_ app: XCUIApplication) {
        app.buttons["Play"].tap()
        sleep(35) // > POSITION_FLUSH_DELTA_SECS (30s)
        let pause = app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'pause'")).firstMatch
        if pause.waitForExistence(timeout: 4) { pause.tap() }
        sleep(1)
    }

    /// True if the reopened episode detail surfaces a saved position
    /// (Resume button or a non-zero timecode), per the contract.
    private func detailShowsResume(_ app: XCUIApplication) -> (Bool, String) {
        let resumeBtn = app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'resume'")).firstMatch
        let hasResume = resumeBtn.waitForExistence(timeout: 5)
        let savedTimes = currentTimeLabels(app).filter { $0 != "0:00" }
        return (hasResume || !savedTimes.isEmpty, "resumeBtn=\(hasResume) times=\(savedTimes)")
    }

    /// P0-04a — Saved position survives BACKGROUND→foreground (realistic flow).
    func testP0_04a_ResumeAfterBackground() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app), "launch")
        sleep(1)
        XCTAssertTrue(openFirstEpisodeDetail(app), "episode detail")
        playPastCheckpointAndPause(app)
        snap(app, "P0-04a-01-paused")

        XCUIDevice.shared.press(.home)
        sleep(3)
        app.activate()
        _ = app.wait(for: .runningForeground, timeout: 10)
        sleep(1)
        snap(app, "P0-04a-02-foregrounded")
        dumpTree(app, "P0-04a-02-detail-tree")

        let (ok, detail) = detailShowsResume(app)
        XCTAssertTrue(ok, "FAIL P0-04a: after background→foreground the episode shows no Resume/saved-time (\(detail))")
    }

    /// P0-04b — Saved position survives a force-quit + cold relaunch.
    /// Contract: "pause or close the app mid-listen and the play button switches
    /// to Resume with the saved time shown in the episode list and detail."
    func testP0_04b_ResumeAfterForceQuit() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app), "launch")
        sleep(1)
        XCTAssertTrue(openFirstEpisodeDetail(app), "episode detail")
        playPastCheckpointAndPause(app)
        snap(app, "P0-04b-01-paused")

        app.terminate()
        sleep(2)
        XCTAssertTrue(launchApp(app), "relaunch")
        sleep(2)
        let hasContinueRow = staticTextContaining(app, "Continue").exists
        snap(app, "P0-04b-02-relaunched-home")
        XCTAssertTrue(openFirstEpisodeDetail(app), "reopen episode detail after relaunch")
        sleep(1)
        snap(app, "P0-04b-03-reopened-detail")
        dumpTree(app, "P0-04b-03-detail-tree")

        let (ok, detail) = detailShowsResume(app)
        XCTAssertTrue(ok || hasContinueRow,
            "FAIL P0-04b: after force-quit+relaunch, no Resume/saved-time/Continue (continueRow=\(hasContinueRow) \(detail)) — position did NOT persist across launch")
    }

    /// P0-04 (confound-free) — Play episode A, navigate back to the show, then
    /// REOPEN A by its title (not by index, to dodge feed reorder). A fresh
    /// detail must show "Resume". This is the true contract test.
    func testP0_04_ResumeReopenByTitle() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        // Open the first subscribed show (visible on Home).
        let showRowBtn = app.buttons.matching(
            NSPredicate(format: "identifier == 'library-podcast-row'")).firstMatch
        let showTarget = showRowBtn.waitForExistence(timeout: 6) ? showRowBtn : staticTextContaining(app, "This American Life")
        XCTAssertTrue(robustTap(showTarget), "open show")
        _ = staticTextContaining(app, "Episodes").waitForExistence(timeout: 8)
        // Capture the first episode's title, then open it.
        let cells = app.cells
        XCTAssertTrue(cells.count > 2, "no episode cells")
        let firstEp = cells.element(boundBy: 2)
        // Episode title = the longest static text in that cell.
        let epTitle = firstEp.staticTexts.allElementsBoundByIndex.map { $0.label }.max(by: { $0.count < $1.count }) ?? ""
        robustTap(firstEp)
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 8), "episode detail")
        app.buttons["Play"].tap()
        sleep(12)
        let pause = app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'pause'")).firstMatch
        if pause.waitForExistence(timeout: 4) { pause.tap() }
        sleep(2)
        // Back to the show list.
        let back = app.navigationBars.buttons.element(boundBy: 0)
        if back.exists { back.tap(); sleep(2) }
        snap(app, "P0-04-back-to-show")
        // Reopen the same episode by title.
        let prefix = String(epTitle.prefix(16))
        XCTAssertTrue(robustTap(staticTextContaining(app, prefix)), "reopen episode by title '\(prefix)'")
        sleep(2)
        snap(app, "P0-04-reopened-by-title")
        dumpTree(app, "P0-04-reopened-tree")
        let hasResume = app.buttons["Resume"].waitForExistence(timeout: 5)
            || app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'resume'")).firstMatch.exists
        XCTAssertTrue(hasResume,
            "FAIL P0-04: reopened detail for '\(prefix)' shows no Resume despite In-Progress proving the store has the position — episode-detail does not read the saved position.")
    }

    /// GROUND TRUTH — after playing, does the store know the position?
    /// The Library "In Progress" filter reads `playbackPosition` from the
    /// snapshot (= kernel store via `position_for`). If the played episode is
    /// absent here, the kernel store never received the position (writeback
    /// id-miss), independent of any detail-view binding.
    func testP0_StoreHasPositionInProgress() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        XCTAssertTrue(openFirstEpisodeDetail(app), "episode detail")
        // Capture the episode title from the detail header for later matching.
        let titleEl = app.staticTexts.allElementsBoundByIndex.first { $0.frame.minY > 120 && $0.frame.minY < 210 && $0.label.count > 8 }
        let title = titleEl?.label ?? "One Town"
        app.buttons["Play"].tap()
        sleep(12) // accrue position; pause flushes
        let pause = app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'pause'")).firstMatch
        if pause.waitForExistence(timeout: 4) { pause.tap() }
        sleep(2)

        // Go to Library via sidebar and select In Progress.
        let sidebar = app.buttons["Open sidebar"]
        if sidebar.waitForExistence(timeout: 5) { sidebar.tap(); sleep(1) }
        let lib = app.buttons["Library"]
        if lib.waitForExistence(timeout: 4) { robustTap(lib); sleep(2) }
        let chip = staticTextContaining(app, "In Progress")
        if chip.waitForExistence(timeout: 4) { robustTap(chip); sleep(2) }
        snap(app, "P0-inprogress")
        dumpTree(app, "P0-inprogress-tree")

        // Does the In Progress list contain our episode (by a title prefix)?
        let prefix = String(title.prefix(12))
        let found = staticTextContaining(app, prefix).waitForExistence(timeout: 4)
        XCTAssertTrue(found,
            "GROUND TRUTH: played episode '\(title)' is ABSENT from Library→In Progress — the kernel store never recorded the position (writeback id-miss).")
    }

    // MARK: - readouts

    private func currentEpisodeTitle(_ app: XCUIApplication) -> String {
        app.staticTexts.allElementsBoundByIndex.first(where: { $0.frame.minY > 110 && $0.frame.minY < 200 })?.label ?? "?"
    }

    /// Collect labels that look like mm:ss timecodes.
    private func currentTimeLabels(_ app: XCUIApplication) -> [String] {
        let re = try? NSRegularExpression(pattern: "^-?\\d{1,2}:\\d{2}(:\\d{2})?$")
        return app.staticTexts.allElementsBoundByIndex.compactMap { el in
            let l = el.label
            guard let re, re.firstMatch(in: l, range: NSRange(l.startIndex..., in: l)) != nil else { return nil }
            return l
        }
    }
}
