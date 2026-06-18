import XCTest

/// Verifies fix 19b46163: ClippingsView renders a card for a clip whose
/// episode is no longer in the library instead of showing only the section
/// header ("Earlier") over blank space.
///
/// The test is self-seeding: it launches the app with `--UITestSeed
/// --UITestSeedOrphanClip` so that `UITestSeeder` writes an orphan clip to the
/// kernel-owned `clips.json` sidecar (the authoritative clip source) whose
/// `episode_id` is not present in the episode list. No pre-seeding by an
/// external QA fixture is required.
final class ClippingsFixUITests: XCTestCase {
    override func setUp() { super.setUp(); continueAfterFailure = true }

    func testOrphanClipRendersCard() {
        // Launch with both seed flags: --UITestSeed for the standard library
        // fixture and --UITestSeedOrphanClip for the orphan clip injection.
        // UITestSeeder writes the orphan clip into the kernel-owned clips.json
        // sidecar (the authoritative clip source) so the kernel loads it on start.
        let app = XCUIApplication(bundleIdentifier: App.bundleID)
        app.launchArguments = ["--UITestSeed", "--UITestSeedOrphanClip"]
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 15), "app launched")

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

        // The fix (19b46163) ensures we see the clip card, not just a naked
        // section header. A rendered card exposes the caption and/or transcript
        // text as static text elements.
        // Give the kernel time to emit a snapshot that includes the orphan
        // clip. The clip is seeded to clips.json before kernel start, so it
        // should be in the FIRST snapshot — but the async hash computation and
        // podcastSnapshot commit add latency. 12 s covers cold-start kernel
        // init + first snapshot delivery + SwiftUI re-render.
        let captionText = "Orphan clip"
        let transcriptText = "economy is not going"

        let captionEl = app.staticTexts.containing(
            NSPredicate(format: "label CONTAINS[c] %@", captionText)).firstMatch
        let transcriptEl = app.staticTexts.containing(
            NSPredicate(format: "label CONTAINS[c] %@", transcriptText)).firstMatch

        let cardRendered = captionEl.waitForExistence(timeout: 12) || transcriptEl.waitForExistence(timeout: 5)
        snap(app, "clippings-03-result")

        // Hard fail — no skip. The orphan clip is deterministically seeded by
        // UITestSeeder, so absence of the card is always a regression.
        XCTAssertTrue(cardRendered,
            "FAIL clippings-fix: orphan clip (episode not in library) rendered no card — " +
            "expected caption '\(captionText)' or transcript '\(transcriptText)' to be visible. " +
            "This is the regression tested by commit 19b46163.")

        // Also verify we are NOT showing ONLY a section header with no content.
        // If the bug regresses, there will be an "Earlier" label but no clip text.
        let sectionHeader = app.staticTexts.containing(
            NSPredicate(format: "label CONTAINS[c] 'earlier'")).firstMatch
        let hasHeader = sectionHeader.exists
        if hasHeader && !cardRendered {
            XCTFail("REGRESSION clippings-fix: 'Earlier' section header present but no clip card — ghost header bug is back.")
        }
    }
}
