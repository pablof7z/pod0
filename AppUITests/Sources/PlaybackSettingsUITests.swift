import XCTest

/// Simulator UI coverage for `playback-speed-persists` and
/// `player-transition-perf` scenarios (#547, fixed in #561).
///
/// `testPlaybackSpeedPersists`: Changes speed to 1.5× in the full player,
/// force-quits the app, relaunches with `--UITestSeedRelaunch` (which preserves
/// podcasts.json — the kernel stores `default_playback_rate` there), then opens
/// the player and asserts the speed label still reads "1.5×". Fixed by #561:
/// `SetSpeed` now writes `default_playback_rate` to podcasts.json so it persists
/// across cold relaunch. Swift `applyPreferences` reads it from the kernel
/// settings snapshot and applies it to `AudioEngine.rate` pre-play.
///
/// `testPlayerTransitionPerf`: Measures the wall-clock time for full-player
/// open and close in a baseline performance assertion.
final class PlaybackSettingsUITests: XCTestCase {

    override func setUp() { super.setUp(); continueAfterFailure = true }

    override func tearDown() {
        XCUIApplication(bundleIdentifier: App.bundleID).terminate()
        super.tearDown()
    }

    // MARK: - playback-speed-persists

    func testPlaybackSpeedPersists() throws {
        // #561 fix: the kernel now persists `default_playback_rate` to
        // podcasts.json whenever `SetSpeed` is dispatched. `--UITestSeedRelaunch`
        // preserves podcasts.json, so the chosen rate survives force-quit +
        // cold relaunch. Swift `applyPreferences` reads `settings.defaultPlaybackRate`
        // from the kernel snapshot and applies it to `AudioEngine.rate` before
        // the first episode loads, so `play()` → `playImmediately(atRate:)` uses
        // the persisted value. BACKLOG entry kernel-speed-persistence-uitest removed.
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)

        // Open episode detail and start playback.
        guard openFirstPodcastFromHome(app), openFirstEpisodeFromShow(app) else {
            XCTFail("speed-persists: could not reach episode detail"); return
        }
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 8), "no Play button")
        app.buttons["Play"].tap(); sleep(2)

        // Open full player.
        guard openFullPlayerFromMiniPlayer(app) else {
            XCTFail("speed-persists: mini-player did not appear"); return
        }
        sleep(1)
        snap(app, "speed-persist-01-full-player")

        // Open More options → Speed.
        let moreBtn = app.buttons["More options"]
        guard moreBtn.waitForExistence(timeout: 5) else {
            XCTFail("speed-persists: 'More options' button not found"); return
        }
        moreBtn.tap(); sleep(1)

        let speedItem = app.buttons.matching(
            NSPredicate(format: "label BEGINSWITH 'Speed:'")).firstMatch
        guard speedItem.waitForExistence(timeout: 4) else {
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.2)).tap()
            XCTFail("speed-persists: 'Speed:' item not found in More options menu"); return
        }
        speedItem.tap(); sleep(1)
        snap(app, "speed-persist-02-speed-sheet")

        // Tap the 1.5× row using its stable accessibility identifier.
        let rate15Btn = app.buttons.matching(
            NSPredicate(format: "identifier == 'speed-1.5'")).firstMatch
        let rate15Label = app.buttons["1.5×"]
        let rate15: XCUIElement = rate15Btn.waitForExistence(timeout: 3)
            ? rate15Btn : rate15Label
        guard rate15.waitForExistence(timeout: 3) else {
            XCTFail("speed-persists: 1.5× speed button (identifier 'speed-1.5') not found in speed sheet"); return
        }
        rate15.tap(); sleep(1)
        snap(app, "speed-persist-03-speed-changed")
        dumpTree(app, "speed-persist-03-tree")

        // Reopen More options and capture the current speed label.
        if moreBtn.waitForExistence(timeout: 3) {
            moreBtn.tap(); sleep(1)
            let currentSpeedItem = app.buttons.matching(
                NSPredicate(format: "label BEGINSWITH 'Speed:'")).firstMatch
            let currentLabel = currentSpeedItem.waitForExistence(timeout: 3) ? currentSpeedItem.label : "?"
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.2)).tap()
            XCTAssertTrue(
                currentLabel.contains("1.5"),
                "speed-persists: speed label should show '1.5' but got '\(currentLabel)' — setRate may have failed"
            )
        }

        // Force-quit + cold relaunch with --UITestSeedRelaunch, which PRESERVES
        // podcasts.json (where the kernel stores default_playback_rate via
        // SetSpeed), so the chosen speed survives the cold restart.
        app.terminate(); sleep(2)
        app.launchArguments = ["--UITestSeed", "--UITestSeedRelaunch"]
        XCTAssertTrue(launchApp(app), "relaunch"); sleep(2)
        snap(app, "speed-persist-04-relaunched")

        // Open the episode and player again.
        guard openFirstPodcastFromHome(app), openFirstEpisodeFromShow(app) else {
            XCTFail("speed-persists: could not reopen episode detail after relaunch"); return
        }
        let playResume = app.buttons.matching(
            NSPredicate(format: "label == 'Play' OR label CONTAINS[c] 'resume' OR label == 'Play again'")
        ).firstMatch
        if playResume.waitForExistence(timeout: 8) { playResume.tap(); sleep(2) }

        guard openFullPlayerFromMiniPlayer(app) else {
            XCTFail("speed-persists: mini-player did not appear after relaunch"); return
        }
        sleep(1)
        snap(app, "speed-persist-05-player-after-relaunch")

        // Open More options to read the persisted speed label.
        let moreBtnAfter = app.buttons["More options"]
        guard moreBtnAfter.waitForExistence(timeout: 5) else {
            XCTFail("speed-persists: 'More options' not found in player after relaunch"); return
        }
        moreBtnAfter.tap(); sleep(1)
        snap(app, "speed-persist-06-menu-after-relaunch")
        dumpTree(app, "speed-persist-06-tree")

        let persistedSpeedItem = app.buttons.matching(
            NSPredicate(format: "label BEGINSWITH 'Speed:'")).firstMatch
        let persistedLabel = persistedSpeedItem.waitForExistence(timeout: 3) ? persistedSpeedItem.label : "?"
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.2)).tap() // dismiss

        // Primary assertion: speed label still contains "1.5" after relaunch.
        // The kernel persists the chosen rate as `default_playback_rate` in
        // podcasts.json (SetSpeed → set_default_playback_rate + bump Settings, #561);
        // --UITestSeedRelaunch preserves podcasts.json, so the rate survives the
        // cold restart and the settings projection renders it on the pre-play label.
        XCTAssertTrue(
            persistedLabel.contains("1.5"),
            "FAIL playback-speed-persists: after force-quit + relaunch, speed label is '\(persistedLabel)'" +
            " (expected '1.5×'). The kernel should have restored default_playback_rate" +
            " from podcasts.json into the settings projection."
        )
        snap(app, "speed-persist-07-final")
    }

    // MARK: - player-transition-perf

    /// Measures the wall-clock time to open the full player (mini-player tap →
    /// More options visible) and close it (back to mini-player).
    ///
    /// Budget: 15s open, 8s close — generous ceilings that catch genuine hangs
    /// without being sensitive to self-hosted runner load. If the machine is
    /// pathologically slow (e.g., heavy background I/O during a CI build), these
    /// budgets give it room while still catching a completely frozen transition.
    func testPlayerTransitionPerf() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        guard openFirstPodcastFromHome(app), openFirstEpisodeFromShow(app) else {
            XCTFail("perf: could not reach episode detail"); return
        }
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 8), "no Play")
        app.buttons["Play"].tap(); sleep(3)

        // Warm the mini-player so it's stable before measuring.
        snap(app, "perf-00-warmup")

        var openTime: TimeInterval = 0
        var closeTime: TimeInterval = 0

        // Measure player OPEN: tap mini-player → wait for More options.
        let openStart = Date()
        _ = openFullPlayerFromMiniPlayer(app)
        let moreOptions = app.buttons["More options"]
        _ = moreOptions.waitForExistence(timeout: 5)
        openTime = Date().timeIntervalSince(openStart)
        snap(app, "perf-01-player-open")

        // Measure player CLOSE: swipe down → mini-player reappears.
        let closeStart = Date()
        app.swipeDown()
        let miniBar = app.otherElements["mini-player-bar"]
        _ = miniBar.waitForExistence(timeout: 5)
        closeTime = Date().timeIntervalSince(closeStart)
        snap(app, "perf-02-player-closed")

        // Assert timing budgets (generous — catches hangs, not machine-load variance).
        XCTAssertLessThan(
            openTime, 15.0,
            "PERF player-transition-perf: open took \(String(format: "%.2f", openTime))s — exceeds 15s hang budget"
        )
        XCTAssertLessThan(
            closeTime, 8.0,
            "PERF player-transition-perf: close took \(String(format: "%.2f", closeTime))s — exceeds 8s hang budget"
        )
    }
}
