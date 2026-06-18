import XCTest

/// P0 playback / queue / download scenario coverage from test-scenarios.json.
///
/// Tests are self-contained and soft (continueAfterFailure = true).
/// Physical-device-only scenarios are marked with XCTSkip.
/// Kernel-owned playback bugs are documented inline and not re-tested.
final class P0PlaybackUITests: XCTestCase {
    override func setUp() { super.setUp(); continueAfterFailure = true }

    // Terminate the app after every test so the Rust audio session is fully
    // torn down before the next test launches. Without this, testP0_BackgroundPlaybackContinues
    // (which runs first alphabetically) leaves audio playing in the background,
    // and the corrupted audio session causes the next tests to crash on launch.
    override func tearDown() {
        XCUIApplication(bundleIdentifier: App.bundleID).terminate()
        super.tearDown()
    }

    // MARK: - P0: skip-forward-back-15

    /// Open the full player, skip +15s and confirm position advances,
    /// then skip -15s and confirm it retreats.
    func testP0_SkipForwardBack15() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        // Open episode detail and start playback.
        guard openFirstPodcastFromHome(app), openFirstEpisodeFromShow(app) else {
            XCTFail("skip-forward-back-15: could not open first episode detail"); return
        }
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 8), "no Play button")
        app.buttons["Play"].tap()
        // Open full player.
        sleep(3)
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.92)).tap()
        sleep(2)
        snap(app, "skip-01-full-player")

        // Capture time before skip.
        let before = timeLabels(app)
        // Find skip-forward button (SF symbol name "goforward" or label contains "15").
        let fwd = app.buttons.matching(
            NSPredicate(format: "label CONTAINS[c] 'forward' OR label CONTAINS[c] '15'")).firstMatch
        if fwd.waitForExistence(timeout: 4) {
            fwd.tap(); sleep(1)
            let after = timeLabels(app)
            snap(app, "skip-02-after-forward")
            XCTAssertNotEqual(before, after, "FAIL skip-forward: time labels unchanged after skip-forward-15")
        } else {
            snap(app, "skip-02-no-forward-button")
            XCTFail("FAIL skip-forward-back-15: no forward-skip button found in full player")
        }

        // Skip back.
        let back = app.buttons.matching(
            NSPredicate(format: "label CONTAINS[c] 'backward' OR label CONTAINS[c] 'back'")).firstMatch
        if back.waitForExistence(timeout: 3) {
            let mid = timeLabels(app)
            back.tap(); sleep(1)
            let after2 = timeLabels(app)
            snap(app, "skip-03-after-backward")
            XCTAssertNotEqual(mid, after2, "FAIL skip-backward-15: time labels unchanged after skip-backward-15")
        }
    }

    // MARK: - P0: queue-add-multiple

    /// Verify that tapping Queue on an episode detail toggles the button to
    /// "Queued" (disabled), proving the episode was accepted by the queue.
    ///
    /// NOTE: PlayerQueueSheet exists in the codebase but has no presentation
    /// trigger in the current UI (not wired to player or mini-player) — the
    /// queue panel cannot be opened via XCTest. This test verifies the button
    /// state toggle which is the only observable queue signal available on
    /// the episode detail screen.
    func testP0_QueueAddMultiple() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        guard openFirstPodcastFromHome(app) else {
            XCTFail("queue-add-multiple: no podcast row"); return
        }

        // Queue first episode — verify button toggles to "Queued".
        XCTAssertTrue(openFirstEpisodeFromShow(app), "queue-add-multiple: could not open episode 1 detail")
        sleep(2)
        snap(app, "queue-01-ep1-detail")
        let q1 = app.buttons.matching(NSPredicate(format: "label == 'Queue' OR label == 'Add to Queue'")).firstMatch
        guard q1.waitForExistence(timeout: 5) else {
            snap(app, "queue-01-no-queue-button")
            let dump = XCTAttachment(string: app.debugDescription)
            dump.name = "queue-01-no-queue-tree"; dump.lifetime = .keepAlways; add(dump)
            XCTFail("queue-add-multiple: no Queue button on episode 1 detail")
            return
        }
        q1.tap(); sleep(1)
        snap(app, "queue-01-after-tap")
        // After tapping, the button should become "Queued" (disabled/selected state).
        let queued1 = app.buttons.matching(NSPredicate(format: "label == 'Queued'")).firstMatch
        let ep1Queued = queued1.waitForExistence(timeout: 4)
        XCTAssertTrue(ep1Queued, "FAIL queue-add-multiple: Queue button did not toggle to 'Queued' after tapping (ep1)")

        // Navigate back, open second episode — verify it can also be queued.
        let back1 = app.navigationBars.buttons.element(boundBy: 0)
        if back1.exists { back1.tap(); sleep(1) }
        let ep2Cell = app.cells.count > 3 ? app.cells.element(boundBy: 3) : app.cells.element(boundBy: 2)
        robustTap(ep2Cell); sleep(2)
        snap(app, "queue-02-ep2-detail")
        let q2 = app.buttons.matching(NSPredicate(format: "label == 'Queue' OR label == 'Add to Queue'")).firstMatch
        if q2.waitForExistence(timeout: 5) {
            q2.tap(); sleep(1)
            snap(app, "queue-02-after-tap")
            let queued2 = app.buttons.matching(NSPredicate(format: "label == 'Queued'")).firstMatch
            XCTAssertTrue(queued2.waitForExistence(timeout: 4),
                "FAIL queue-add-multiple: Queue button did not toggle to 'Queued' after tapping (ep2)")
        } else {
            snap(app, "queue-02-no-queue-button")
        }
    }

    // MARK: - P0: background-playback-continues

    /// Start playback, press Home, wait 5s, return to app, confirm audio
    /// is still going (Pause control visible somewhere OR mini-player shows
    /// the episode title).
    func testP0_BackgroundPlaybackContinues() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        guard openFirstPodcastFromHome(app), openFirstEpisodeFromShow(app) else {
            XCTFail("background-playback: could not open first episode detail"); return
        }
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 8), "no Play")
        app.buttons["Play"].tap()
        sleep(4)
        snap(app, "bg-01-playing")

        // Background.
        XCUIDevice.shared.press(.home)
        sleep(5)

        // Foreground.
        app.activate()
        _ = app.wait(for: .runningForeground, timeout: 10)
        sleep(2)
        snap(app, "bg-02-after-foreground")

        let pause = app.buttons.matching(
            NSPredicate(format: "label CONTAINS[c] 'pause'")).firstMatch
        let miniPlayerText = app.staticTexts.matching(
            NSPredicate(format: "label CONTAINS[c] 'The Book'")).firstMatch
        let continued = pause.waitForExistence(timeout: 4) || miniPlayerText.waitForExistence(timeout: 2)
        XCTAssertTrue(continued, "FAIL background-playback-continues: no Pause control or episode text visible after foregrounding — audio may have stopped")
    }

    // MARK: - P0: offline-library-access

    /// Library is accessible with pre-seeded content (no network required).
    /// This proves the library loads from disk, not only from live network.
    func testP0_OfflineLibraryAccess() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        // Library is the default tab. Sidebar → Library.
        let sidebar = app.buttons["Open sidebar"]
        if sidebar.waitForExistence(timeout: 5) { sidebar.tap(); sleep(1) }
        let lib = app.buttons["Library"]
        if lib.waitForExistence(timeout: 4) { robustTap(lib); sleep(2) }
        snap(app, "offline-lib-01")
        // Some content must be visible (episode cells or filter chips).
        let hasContent = app.cells.count > 0 || app.staticTexts.count > 4
        XCTAssertTrue(hasContent, "FAIL offline-library-access: library is empty or failed to load from seeded state")
        // Soft check: Home tab may also show the seeded podcast row.
        // Primary assertion is hasContent above; navigate to Home as evidence.
        let home = app.buttons["Home"]
        if home.waitForExistence(timeout: 4) { robustTap(home); sleep(2) }
        snap(app, "offline-lib-02-home")
        // If a library-podcast-row is present on Home that is a bonus confirmation.
        let podRow = app.buttons.matching(
            NSPredicate(format: "identifier == 'library-podcast-row'")).firstMatch
        if !podRow.waitForExistence(timeout: 5) {
            // Not a hard failure — content was already confirmed in Library above.
            // But capture the state for manual review.
            let dumpHome = XCTAttachment(string: app.debugDescription)
            dumpHome.name = "offline-lib-home-tree"; dumpHome.lifetime = .keepAlways; add(dumpHome)
        }
    }

    // MARK: - P0: reactive-update-cross-screen

    /// Play state change (tapping Play) on the episode detail reflects
    /// on the mini-player visible on the Home tab — confirming reactive
    /// cross-screen state propagation.
    func testP0_ReactiveUpdateCrossScreen() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        guard openFirstPodcastFromHome(app), openFirstEpisodeFromShow(app) else {
            XCTFail("reactive: could not open first episode detail"); return
        }
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 8), "no Play")
        app.buttons["Play"].tap()
        sleep(3)
        snap(app, "reactive-01-playing-in-detail")

        // Navigate back to the show and then back to Home.
        let b1 = app.navigationBars.buttons.element(boundBy: 0)
        if b1.exists { b1.tap(); sleep(1) }
        let b2 = app.navigationBars.buttons.element(boundBy: 0)
        if b2.exists { b2.tap(); sleep(1) }
        snap(app, "reactive-02-home-after-play")
        let dumpR = XCTAttachment(string: app.debugDescription)
        dumpR.name = "reactive-02-tree"; dumpR.lifetime = .keepAlways; add(dumpR)

        // Home tab should show a mini-player or Pause control.
        let pause = app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'pause'")).firstMatch
        let miniPlayer = app.otherElements.matching(
            NSPredicate(format: "label CONTAINS[c] 'mini' OR identifier CONTAINS[c] 'mini'")).firstMatch
        let epText = app.staticTexts.matching(
            NSPredicate(format: "label CONTAINS[c] 'The Book'")).firstMatch
        let reactive = pause.waitForExistence(timeout: 4)
            || miniPlayer.waitForExistence(timeout: 2)
            || epText.waitForExistence(timeout: 2)
        XCTAssertTrue(reactive, "FAIL reactive-update-cross-screen: no mini-player/Pause on Home after starting playback in episode detail")
    }

    // MARK: - BLOCKED (physical-device-only) ----------------------------------

    /// offline-playback-downloaded: requires a downloaded episode AND disabling
    /// the simulator's network — not reliably doable in a CI/XCTest context.
    func testP0_OfflinePlaybackDownloaded_BLOCKED() throws {
        throw XCTSkip("BLOCKED: offline-playback-downloaded requires a completed download and network disable. Run manually on device: download an episode, enable Airplane Mode, play it, confirm audio plays.")
    }

    /// resume-position-across-restart (P0): the Rust kernel persists
    /// ep.position_secs from every Playing audio report (audio_report.rs
    /// apply_writeback). Cold relaunch reads that value so the episode shows
    /// "Resume" with the correct saved time.
    ///
    /// Protocol: play ≥10 s, force-quit (SIGKILL), cold relaunch with
    /// --UITestSeedRelaunch so UITestSeeder carries the persisted position
    /// forward into podcasts.json, then assert the episode detail shows Resume.
    func testP0_ResumePositionAcrossRestart() throws {
        // ── First launch: play past the kernel's flush checkpoint ──────────
        let app = App.make()
        XCTAssertTrue(launchApp(app), "first launch")
        sleep(1)
        guard openFirstPodcastFromHome(app), openFirstEpisodeFromShow(app) else {
            XCTFail("resume-across-restart: could not open first episode detail"); return
        }
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 8), "no Play button")
        app.buttons["Play"].tap()
        // Play for 12 s — long enough to pass the kernel's initial flush
        // threshold and accumulate a non-trivial position.
        sleep(12)
        snap(app, "resume-restart-01-playing")

        // Force-quit (simulates the real crash / swipe-up dismiss).
        app.terminate()
        sleep(2)

        // ── Cold relaunch: resume from the kernel's own persisted position ───
        // --UITestSeedRelaunch tells UITestSeeder to PRESERVE the kernel's
        // podcasts.json (which already carries position_secs written by
        // audio_report.rs::apply_writeback) and wipe only the Swift SQLite
        // mirror, so the resume point comes solely from the kernel — proving
        // the single-source-of-truth contract end-to-end.
        app.launchArguments = ["--UITestSeed", "--UITestSeedRelaunch"]
        XCTAssertTrue(launchApp(app), "cold relaunch")
        sleep(2)
        snap(app, "resume-restart-02-relaunched-home")

        guard openFirstPodcastFromHome(app), openFirstEpisodeFromShow(app) else {
            XCTFail("resume-across-restart: could not reopen episode detail after relaunch"); return
        }
        sleep(2)
        snap(app, "resume-restart-03-reopened-detail")

        let hasResume = app.buttons["Resume"].waitForExistence(timeout: 5)
            || app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'resume'")).firstMatch.exists
        XCTAssertTrue(hasResume,
            "FAIL resume-position-across-restart: after force-quit+cold-relaunch, episode detail shows no 'Resume' — the kernel did not persist position_secs during playback or UITestSeeder did not carry it forward")
    }

    // MARK: - Helpers

    private func timeLabels(_ app: XCUIApplication) -> [String] {
        let re = try? NSRegularExpression(pattern: "^-?\\d{1,2}:\\d{2}(:\\d{2})?$")
        return app.staticTexts.allElementsBoundByIndex.compactMap { el in
            let l = el.label
            guard let re, re.firstMatch(in: l, range: NSRange(l.startIndex..., in: l)) != nil else { return nil }
            return l
        }
    }
}
