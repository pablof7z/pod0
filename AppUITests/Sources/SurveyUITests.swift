import XCTest

/// Breadth survey: navigate every major surface on the device build, capture a
/// screenshot + tree dump of each, and make soft observations. One run produces
/// the defect-inventory evidence for areas across the app. Soft by design
/// (continueAfterFailure) so one bad surface doesn't hide the rest.
final class SurveyUITests: XCTestCase {
    override func setUp() { super.setUp(); continueAfterFailure = true }

    private func dump(_ app: XCUIApplication, _ name: String) {
        let t = XCTAttachment(string: app.debugDescription)
        t.name = name; t.lifetime = .keepAlways; add(t)
    }

    private func openSidebar(_ app: XCUIApplication) {
        let btn = app.buttons["Open sidebar"]
        if btn.waitForExistence(timeout: 5) { btn.tap(); sleep(1) }
    }

    /// Root tabs reachable from the avatar sidebar.
    func testSurvey_01_SidebarTabs() {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        for tab in ["Library", "Bookmarks", "Clippings", "Wiki", "Podcasts"] {
            openSidebar(app)
            let row = app.buttons[tab]
            if row.waitForExistence(timeout: 4) { robustTap(row) } else { snap(app, "survey-\(tab)-NOSIDEBAR") }
            sleep(2)
            snap(app, "survey-tab-\(tab)")
            dump(app, "survey-tab-\(tab)-tree")
        }
    }

    /// Library search across episodes.
    func testSurvey_02_Search() {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        let search = app.buttons["home-search-button"]
        XCTAssertTrue(search.waitForExistence(timeout: 5), "no search button")
        search.tap(); sleep(1)
        snap(app, "survey-search-empty"); dump(app, "survey-search-tree")
        // Type a query into the first text field.
        let field = app.textFields.firstMatch.exists ? app.textFields.firstMatch : app.searchFields.firstMatch
        if field.waitForExistence(timeout: 4) {
            field.tap(); field.typeText("AI")
            sleep(3)
            snap(app, "survey-search-results")
            dump(app, "survey-search-results-tree")
        } else {
            snap(app, "survey-search-NOFIELD")
        }
    }

    /// Agent chat tab.
    func testSurvey_03_Agent() {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        let agent = app.buttons["agent.open"]
        XCTAssertTrue(agent.waitForExistence(timeout: 5), "no agent button")
        agent.tap(); sleep(2)
        snap(app, "survey-agent"); dump(app, "survey-agent-tree")
    }

    /// Settings, then drill into Models / Local Models and the Debug log viewer.
    func testSurvey_04_SettingsModelsDebug() {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        let gear = app.buttons["gear"]
        XCTAssertTrue(gear.waitForExistence(timeout: 5), "no settings gear")
        gear.tap(); sleep(2)
        snap(app, "survey-settings"); dump(app, "survey-settings-tree")

        // Drill into a few promised settings surfaces by visible label.
        for label in ["Models", "Local Models", "Debug", "Networking", "Transcription"] {
            let row = staticTextContaining(app, label)
            if row.waitForExistence(timeout: 2) {
                robustTap(row); sleep(2)
                snap(app, "survey-settings-\(label)")
                dump(app, "survey-settings-\(label)-tree")
                // Return to settings root if a back button exists.
                let back = app.navigationBars.buttons.element(boundBy: 0)
                if back.exists { back.tap(); sleep(1) }
            }
        }
    }

    /// Episode actions: open an episode, exercise Download + transcript surfaces.
    func testSurvey_05_EpisodeActions() {
        let app = App.make()
        XCTAssertTrue(launchApp(app)); sleep(1)
        // Home → first podcast → first episode.
        robustTap(app.buttons.matching(NSPredicate(format: "identifier == 'library-podcast-row'")).firstMatch)
        sleep(2)
        let cells = app.cells
        if cells.count > 2 { robustTap(cells.element(boundBy: 2)) }
        sleep(2)
        snap(app, "survey-episode-detail"); dump(app, "survey-episode-tree")

        // Tap Download and watch ~6s for progress state.
        let dl = app.buttons["Download"]
        if dl.waitForExistence(timeout: 4) {
            dl.tap(); sleep(2); snap(app, "survey-episode-download-1")
            sleep(4); snap(app, "survey-episode-download-2")
            dump(app, "survey-episode-download-tree")
        }
    }
}
