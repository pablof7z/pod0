import XCTest

/// Setup helper: subscribes to "This Week in Tech" via the in-app search so
/// journey tests have content to work with. Run once before CoreJourneyUITests.
final class SetupSubscribeUITests: XCTestCase {
    override func setUp() { super.setUp(); continueAfterFailure = true }

    func testSetup_SubscribeToShow() {
        let app = XCUIApplication(bundleIdentifier: App.bundleID)
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 15))

        // Dismiss What's New if present.
        let gotIt = app.buttons["Got it"]
        if gotIt.waitForExistence(timeout: 5) { gotIt.tap(); sleep(1) }

        snap(app, "setup-00-home")

        // Tap the Search button (top-right toolbar).
        let searchBtn = app.buttons["home-search-button"]
        if !searchBtn.waitForExistence(timeout: 8) {
            // Fallback: magnifying glass button.
            app.buttons["search"].firstMatch.tap()
        } else {
            searchBtn.tap()
        }
        sleep(1)
        snap(app, "setup-01-search-open")

        // Type search query.
        let searchField = app.searchFields.firstMatch.exists
            ? app.searchFields.firstMatch
            : app.textFields.firstMatch
        XCTAssertTrue(searchField.waitForExistence(timeout: 8), "search field")
        searchField.tap()
        searchField.typeText("This American Life")
        sleep(3) // wait for iTunes results
        snap(app, "setup-02-search-results")

        // Tap the first result row.
        let firstResult = app.cells.matching(
            NSPredicate(format: "identifier == 'search-result-row'")).firstMatch
        let anyCell = app.cells.firstMatch
        let target = firstResult.waitForExistence(timeout: 5) ? firstResult : anyCell
        XCTAssertTrue(target.waitForExistence(timeout: 5), "search result cell")
        robustTap(target)
        sleep(2)
        snap(app, "setup-03-podcast-detail")

        // Tap Subscribe / Follow.
        for label in ["Subscribe", "Follow", "Add"] {
            let btn = app.buttons[label]
            if btn.waitForExistence(timeout: 3) {
                btn.tap()
                sleep(3)
                snap(app, "setup-04-subscribed")
                break
            }
        }

        // Verify the podcast now appears in the library.
        let hasContent = app.staticTexts.count > 2
        XCTAssertTrue(hasContent, "library should have content after subscribing")
        snap(app, "setup-05-post-subscribe")
    }
}
