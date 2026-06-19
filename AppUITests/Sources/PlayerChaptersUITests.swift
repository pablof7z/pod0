import XCTest

/// Simulator UI coverage for `chapter-list-renders` and `chapter-tap-seeks`
/// scenarios (#547).
///
/// The UITestSeeder always embeds 3 publisher chapters in ep1's podcasts.json
/// entry (Introduction 0s–60s, Main Story 60s–180s, Conclusion 180s–300s).
/// The kernel loads them at set_data_dir time and projects them as
/// ChapterSummary → Episode.Chapter in every snapshot tick. PlayerView shows
/// PlayerChaptersScrollView when `navigableChapters` is non-empty; each row
/// gets `.accessibilityIdentifier("chapter-<uuid>")` where the UUID is
/// generated fresh at projection time, so tests match with BEGINSWITH.
///
/// NSPredicate is not Sendable under Swift 6; create a fresh instance at each
/// call site rather than capturing a shared variable (region-isolation rule).
final class PlayerChaptersUITests: XCTestCase {

    override func setUp() { super.setUp(); continueAfterFailure = true }

    override func tearDown() {
        XCUIApplication(bundleIdentifier: App.bundleID).terminate()
        super.tearDown()
    }

    // MARK: - chapter-list-renders

    func testChapterListRenders() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        guard openFirstPodcastFromHome(app), openFirstEpisodeFromShow(app) else {
            XCTFail("chapters: could not reach episode detail"); return
        }

        // Start playback and open the full player.
        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 8), "no Play")
        app.buttons["Play"].tap(); sleep(2)
        guard openFullPlayerFromMiniPlayer(app) else {
            XCTFail("chapters: mini-player did not appear"); return
        }
        sleep(2)
        snap(app, "chapters-01-full-player")
        dumpTree(app, "chapters-01-tree")

        // Chapter rows have accessibilityIdentifier "chapter-<uuid>". The seed
        // embeds 3 chapters; the kernel projects all of them (include_in_toc=true).
        // NSPredicate is not Sendable — create fresh at each call site.
        let firstChapterRow = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH 'chapter-'")).firstMatch
        XCTAssertTrue(
            firstChapterRow.waitForExistence(timeout: 8),
            "FAIL chapter-list-renders: no 'chapter-*' buttons found in full player. " +
            "Check UITestSeeder wrote chapters to ep1 in podcasts.json and " +
            "PlayerChaptersScrollView applies .accessibilityIdentifier(\"chapter-\\(chapter.id)\")."
        )
        snap(app, "chapters-02-chapter-rail")

        // Verify at least 2 chapters are visible (seed has 3: Introduction, Main Story, Conclusion).
        let chapterCount = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH 'chapter-'")).count
        XCTAssertGreaterThan(
            chapterCount, 1,
            "FAIL chapter-list-renders: only \(chapterCount) chapter row found — " +
            "expected 3 (Introduction, Main Story, Conclusion) from the seeded chapters"
        )
        snap(app, "chapters-03-rows-present")
    }

    // MARK: - chapter-tap-seeks

    func testChapterTapSeeks() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        guard openFirstPodcastFromHome(app), openFirstEpisodeFromShow(app) else {
            XCTFail("chapter-seek: could not reach episode detail"); return
        }

        XCTAssertTrue(app.buttons["Play"].waitForExistence(timeout: 8), "no Play")
        app.buttons["Play"].tap(); sleep(2)
        guard openFullPlayerFromMiniPlayer(app) else {
            XCTFail("chapter-seek: mini-player did not appear"); return
        }
        sleep(2)
        snap(app, "chseek-01-full-player")

        // NSPredicate not Sendable — fresh instance at each call.
        let firstRow = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH 'chapter-'")).firstMatch
        XCTAssertTrue(
            firstRow.waitForExistence(timeout: 8),
            "chapter-seek: no chapter rows found — seeder must have written chapters to ep1"
        )

        // Capture current time labels before tapping.
        let timesBefore = currentTimeLabels(app)
        snap(app, "chseek-02-before-tap")

        // Tap the SECOND chapter row ("Main Story", start 60s). Tapping chapter 0
        // at 0:00 on a freshly-started episode may not visibly change the timecode.
        let allChapterRows = app.buttons.matching(
            NSPredicate(format: "identifier BEGINSWITH 'chapter-'"))
        let rowToTap = allChapterRows.count > 1
            ? allChapterRows.element(boundBy: 1)
            : allChapterRows.firstMatch
        robustTap(rowToTap); sleep(2)
        snap(app, "chseek-03-after-tap")

        let timesAfter = currentTimeLabels(app)

        // After tapping the 60s chapter, the position should move.
        XCTAssertNotEqual(
            timesBefore, timesAfter,
            "FAIL chapter-tap-seeks: time labels unchanged after tapping 'Main Story' chapter row. " +
            "navigationalSeek(to:60.0) may not have fired or the timecode update is delayed " +
            "beyond 2s. timesBefore=\(timesBefore) timesAfter=\(timesAfter)"
        )
    }

    // MARK: - Helpers

    private func currentTimeLabels(_ app: XCUIApplication) -> [String] {
        let re = try? NSRegularExpression(pattern: "^-?\\d{1,2}:\\d{2}(:\\d{2})?$")
        return app.staticTexts.allElementsBoundByIndex.compactMap { el in
            let l = el.label
            guard let re, re.firstMatch(in: l, range: NSRange(l.startIndex..., in: l)) != nil else { return nil }
            return l
        }
    }
}
