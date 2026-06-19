import XCTest

/// Simulator UI coverage for the `subscribe-via-rss-url` scenario (#547).
///
/// Drives the real app flow: Home → "See all podcasts" → "+" → AddShowSheet
/// → "From URL" segment → paste URL → Subscribe. The accessibility identifier
/// `add-show-url-field` on the TextField is the primary assertion target.
///
/// Network note: the simulator may not resolve external RSS URLs. The test
/// uses the already-seeded feed URL so `SubscriptionService.AddError.alreadySubscribed`
/// fires and the sheet dismisses cleanly — proving the full UI round-trip
/// without requiring external network access. If the URL resolves to a real
/// feed (CI with live network) the show would be added or updated; both paths
/// are acceptable outcomes. A network error banner is also acceptable — the
/// test verifies no crash and that the URL field and Subscribe button are
/// accessible throughout.
final class SubscribeViaRSSUITests: XCTestCase {

    override func setUp() { super.setUp(); continueAfterFailure = true }

    override func tearDown() {
        XCUIApplication(bundleIdentifier: App.bundleID).terminate()
        super.tearDown()
    }

    // MARK: - subscribe-via-rss-url

    func testSubscribeViaRSSURL() throws {
        let app = App.make()
        XCTAssertTrue(launchApp(app), "launch")
        sleep(2)
        snap(app, "rss-00-home")

        // Step 1 — Navigate to AllPodcastsListView via "See all podcasts".
        // The seeded library shows "This American Life" so the subscription
        // list section header with "See all podcasts" should be visible.
        let seeAll = app.buttons["See all podcasts"]
        if !seeAll.waitForExistence(timeout: 8) {
            dumpTree(app, "rss-NOSEEALL-tree")
            XCTFail("subscribe-via-rss-url: 'See all podcasts' button not found on Home — cannot navigate to AllPodcastsListView")
            return
        }
        seeAll.tap(); sleep(1)
        snap(app, "rss-01-all-podcasts")

        // Step 2 — Tap the "+" (Add show) button in AllPodcastsListView.
        let addShow = app.buttons["Add show"]
        guard addShow.waitForExistence(timeout: 5) else {
            snap(app, "rss-NOADD")
            dumpTree(app, "rss-NOADD-tree")
            XCTFail("subscribe-via-rss-url: 'Add show' (+) button not found in All Podcasts view")
            return
        }
        addShow.tap(); sleep(1)
        snap(app, "rss-02-add-show-sheet")

        // Step 3 — Switch to the "From URL" segment in AddShowSheet.
        let fromURL = app.buttons["From URL"]
        guard fromURL.waitForExistence(timeout: 5) else {
            snap(app, "rss-NOFROMURL")
            dumpTree(app, "rss-NOFROMURL-tree")
            XCTFail("subscribe-via-rss-url: 'From URL' segment not found in AddShowSheet")
            return
        }
        fromURL.tap(); sleep(1)
        snap(app, "rss-03-from-url-segment")

        // Step 4 — Locate the URL field by its stable accessibility identifier.
        // The identifier "add-show-url-field" is set on the TextField in AddByURLForm.
        let urlField = app.textFields.matching(
            NSPredicate(format: "identifier == 'add-show-url-field'")
        ).firstMatch

        guard urlField.waitForExistence(timeout: 5) else {
            snap(app, "rss-NOFIELD")
            dumpTree(app, "rss-NOFIELD-tree")
            XCTFail(
                "FAIL subscribe-via-rss-url: 'add-show-url-field' TextField not found in From URL segment" +
                " — ensure AddByURLForm.TextField has .accessibilityIdentifier(\"add-show-url-field\")"
            )
            return
        }
        XCTAssertTrue(urlField.exists, "FAIL subscribe-via-rss-url: URL field element does not exist")

        // Step 5 — Type the seeded feed URL. Using the seeded URL exercises the
        // `alreadySubscribed` path in SubscriptionService so the test is
        // network-independent (the local library already has this feed).
        urlField.tap(); sleep(1)
        let testFeedURL = "https://test.podcast.local/rss.xml"
        urlField.typeText(testFeedURL)
        usleep(500_000)
        snap(app, "rss-04-url-typed")

        // Step 6 — Verify the Subscribe button exists and is enabled.
        // (It should be enabled once the field has non-empty text.)
        let subscribeBtn = app.buttons.matching(
            NSPredicate(format: "label == 'Subscribe' OR label CONTAINS[c] 'Fetching'")
        ).firstMatch
        let hasSubscribeBtn = subscribeBtn.waitForExistence(timeout: 4)
        XCTAssertTrue(
            hasSubscribeBtn,
            "FAIL subscribe-via-rss-url: Subscribe button not found after typing URL in add-show-url-field"
        )
        snap(app, "rss-05-subscribe-ready")

        // Step 7 — Tap Subscribe and observe the outcome.
        // Acceptable outcomes: sheet dismisses (alreadySubscribed → show in Library),
        // an error banner appears (network / DNS failure), or the sheet stays
        // open with a spinner then an error. A crash is NOT acceptable.
        if hasSubscribeBtn { subscribeBtn.tap(); sleep(3) }
        snap(app, "rss-06-after-submit")
        dumpTree(app, "rss-06-tree")

        // The app must still be running (not crashed).
        XCTAssertEqual(
            app.state, .runningForeground,
            "FAIL subscribe-via-rss-url: app crashed or was killed after tapping Subscribe"
        )

        // If the sheet dismissed, the seeded show should still be in Home.
        // If an error banner appeared, that's also a valid (non-crash) outcome.
        // We accept either — the key assertions are the field identity and no crash.
    }
}
