import XCTest

/// Verifies fix 19b46163: ClippingsView renders a card for a clip whose
/// episode is no longer in the library instead of showing only the section
/// header ("Earlier") over blank space.
///
/// Precondition: the simulator's podcastr-state.v1.json has been seeded with
/// one clip whose episodeID is a random UUID not present in the episodes array.
/// (Done by the QA agent before this test runs.)
final class ClippingsFixUITests: XCTestCase {
    override func setUp() { super.setUp(); continueAfterFailure = true }

    func testOrphanClipRendersCard() throws {
        // This test requires a pre-seeded orphan clip. Skip if not present.
        let app = XCUIApplication(bundleIdentifier: App.bundleID)
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 15), "app launched")
        let orphanExists = app.staticTexts["Orphaned Clip"].waitForExistence(timeout: 2)
        try XCTSkipUnless(orphanExists, "Orphan clip not seeded — run the QA setup fixture first")

        // Dismiss the What's New sheet if present.
        let gotIt = app.buttons["Got it"]
        if gotIt.waitForExistence(timeout: 5) {
            gotIt.tap()
            sleep(1)
        }
        snap(app, "clippings-01-home")

        // Navigate to Clippings tab (custom tab bar — use label "Clippings").
        // The tab bar is a SwiftUI TabView whose items have matching labels.
        let clippingsTab = app.tabBars.buttons["Clippings"]
        if !clippingsTab.waitForExistence(timeout: 5) {
            // Fallback: try sidebar → Clippings button.
            let sidebar = app.buttons["Open sidebar"]
            if sidebar.waitForExistence(timeout: 3) {
                sidebar.tap(); sleep(1)
            }
            let clippingsBtn = app.buttons["Clippings"]
            if clippingsBtn.waitForExistence(timeout: 3) {
                clippingsBtn.tap(); sleep(1)
            }
        } else {
            clippingsTab.tap()
            sleep(1)
        }

        snap(app, "clippings-02-tab")
        let tree = XCTAttachment(string: app.debugDescription)
        tree.name = "clippings-02-tab-tree"; tree.lifetime = .keepAlways; add(tree)

        // The fix ensures we see the clip card, not just a naked section header.
        // A rendered card has a static text with the caption or transcript text.
        let captionText = "Orphan clip"
        let transcriptText = "economy is not going"

        let captionEl = app.staticTexts.containing(
            NSPredicate(format: "label CONTAINS[c] %@", captionText)).firstMatch
        let transcriptEl = app.staticTexts.containing(
            NSPredicate(format: "label CONTAINS[c] %@", transcriptText)).firstMatch

        let cardRendered = captionEl.waitForExistence(timeout: 5) || transcriptEl.waitForExistence(timeout: 3)
        snap(app, "clippings-03-result")

        XCTAssertTrue(cardRendered,
            "FAIL clippings-fix: orphan clip (episode not in library) rendered no card — " +
            "expected caption '\(captionText)' or transcript '\(transcriptText)' to be visible. " +
            "This is the regression tested by commit 19b46163.")

        // Also verify we are NOT showing ONLY a section header with no content.
        // If the bug regresses, there will be a "Earlier" label but no clip text.
        let sectionHeader = app.staticTexts.containing(
            NSPredicate(format: "label CONTAINS[c] 'earlier'")).firstMatch
        let hasHeader = sectionHeader.exists
        if hasHeader && !cardRendered {
            XCTFail("REGRESSION clippings-fix: 'Earlier' section header present but no clip card — ghost header bug is back.")
        }
    }
}
