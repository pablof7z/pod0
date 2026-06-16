import XCTest

/// Shared support for the device scenario suite. Everything is black-box: tests
/// drive the installed `io.f7z.podcast` build by bundle id and assert on the
/// visible accessibility tree. Helpers favour resilience (wait, then tap by
/// label with a coordinate fallback) because the app does not yet expose
/// stable accessibility identifiers on every control.
enum App {
    static let bundleID = "io.f7z.podcast"
    /// Returns an XCUIApplication pre-configured with --UITestSeed so the app
    /// writes a minimal This American Life library before the kernel starts.
    /// Avoids relying on the external seed_pod0_state.py script, which targets
    /// a specific container UUID and becomes stale after each xcodebuild install.
    static func make() -> XCUIApplication {
        let app = XCUIApplication(bundleIdentifier: bundleID)
        app.launchArguments = ["--UITestSeed"]
        return app
    }
}

extension XCTestCase {
    /// Launch (or relaunch) the app and wait for foreground.
    @discardableResult
    func launchApp(_ app: XCUIApplication, timeout: TimeInterval = 15) -> Bool {
        app.launch()
        return app.wait(for: .runningForeground, timeout: timeout)
    }

    /// Attach the current screen as a kept screenshot under a step name.
    /// No-ops when the app is not in the foreground — avoids "lost connection"
    /// errors from XCTest's screenshot API when the app has crashed or been
    /// killed during a background/foreground lifecycle test.
    func snap(_ app: XCUIApplication, _ name: String) {
        guard app.state == .runningForeground else { return }
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }

    /// Wait for any element matching the predicate over the given query.
    @discardableResult
    func waitFor(_ element: XCUIElement, _ timeout: TimeInterval = 10) -> Bool {
        element.waitForExistence(timeout: timeout)
    }

    /// Tap an element if it exists & is hittable; otherwise tap its frame centre
    /// via a normalized coordinate (works for cells SwiftUI marks non-hittable).
    @discardableResult
    func robustTap(_ element: XCUIElement, _ timeout: TimeInterval = 8) -> Bool {
        guard element.waitForExistence(timeout: timeout) else { return false }
        if element.isHittable {
            element.tap()
            return true
        }
        // Fallback: SwiftUI rows are sometimes reported non-hittable.
        let c = element.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        c.tap()
        return true
    }

    /// Expands the mini-player into the full player.
    ///
    /// In long UI-test suite runs SwiftUI can expose `mini-player-bar` with an
    /// invalid activation point even though the bar is visibly present. Wait on
    /// the identifier for synchronization, then tap through an app-origin
    /// coordinate so XCTest does not need to derive a hit point from the element.
    @discardableResult
    func openFullPlayerFromMiniPlayer(_ app: XCUIApplication, timeout: TimeInterval = 10) -> Bool {
        let miniBar = app.otherElements["mini-player-bar"]
        let miniBarButton = app.buttons.matching(
            NSPredicate(format: "identifier == 'mini-player-bar'")
        ).firstMatch

        guard miniBar.waitForExistence(timeout: timeout)
                || miniBarButton.waitForExistence(timeout: 2) else {
            return false
        }

        if tapValidFrameCenter(miniBar, in: app)
            || tapValidFrameCenter(miniBarButton, in: app) {
            return waitForFullPlayer(app)
        }

        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.88)).tap()
        return waitForFullPlayer(app)
    }

    private func tapValidFrameCenter(_ element: XCUIElement, in app: XCUIApplication) -> Bool {
        guard element.exists else { return false }
        let frame = element.frame
        guard frame.width > 1,
              frame.height > 1,
              frame.midX.isFinite,
              frame.midY.isFinite else {
            return false
        }
        app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
            .withOffset(CGVector(dx: frame.midX, dy: frame.midY))
            .tap()
        return true
    }

    private func waitForFullPlayer(_ app: XCUIApplication) -> Bool {
        app.buttons["More options"].waitForExistence(timeout: 4)
            || app.sliders["Playback scrubber"].waitForExistence(timeout: 2)
    }

    /// First static text whose label contains `substring` (case-insensitive).
    func staticTextContaining(_ app: XCUIApplication, _ substring: String) -> XCUIElement {
        let p = NSPredicate(format: "label CONTAINS[c] %@", substring)
        return app.staticTexts.containing(p).firstMatch
    }

    /// Any element (button or static text) whose label matches, preferring buttons.
    func control(_ app: XCUIApplication, labeled label: String) -> XCUIElement {
        let btn = app.buttons[label]
        return btn.exists ? btn : app.staticTexts[label]
    }

    /// Opens the seeded/visible subscribed podcast from Home.
    ///
    /// SwiftUI exposes the row as a combined `library-podcast-row` button, but
    /// tapping that combined accessibility element can land on the row's trailing
    /// metadata and not activate the `NavigationLink`. Prefer the visible title,
    /// then fall back to the row identifier and a frame-based title-area tap.
    @discardableResult
    func openFirstPodcastFromHome(_ app: XCUIApplication) -> Bool {
        let homeTab = app.buttons["tab-home"]
        if homeTab.waitForExistence(timeout: 2) {
            homeTab.tap()
        }

        let seededTitle = staticTextContaining(app, "This American Life")
        _ = seededTitle.waitForExistence(timeout: 20)

        let visibleHomeTitle = app.staticTexts.allElementsBoundByIndex.first { element in
            element.label.localizedCaseInsensitiveContains("This American Life")
                && element.frame.minY > 100
                && element.frame.minY < 320
        }
        if let visibleHomeTitle {
            robustTap(visibleHomeTitle)
            if waitForShowDetail(app) { return true }
        }

        let row = app.buttons.matching(
                NSPredicate(format: "identifier == 'library-podcast-row'")
        ).firstMatch
        if row.waitForExistence(timeout: 12) {
            let origin = app.coordinate(withNormalizedOffset: CGVector(dx: 0, dy: 0))
            origin.withOffset(CGVector(dx: row.frame.minX + 72, dy: row.frame.midY)).tap()
            if waitForShowDetail(app) { return true }
        }

        return false
    }

    /// Opens the first episode detail from a show detail screen.
    @discardableResult
    func openFirstEpisodeFromShow(_ app: XCUIApplication) -> Bool {
        let episodeRow = app.buttons.matching(
            NSPredicate(format: "identifier == 'home-episode-row'")
        ).firstMatch
        if episodeRow.waitForExistence(timeout: 8) {
            episodeRow.tap()
        } else {
            let cells = app.cells
            guard cells.count > 2 else { return false }
            robustTap(cells.element(boundBy: 2))
        }

        return app.buttons["Play"].waitForExistence(timeout: 8)
            || app.buttons["Queue"].waitForExistence(timeout: 4)
            || app.buttons.matching(NSPredicate(format: "label CONTAINS[c] 'resume'")).firstMatch.waitForExistence(timeout: 2)
    }

    @discardableResult
    func waitForShowDetail(_ app: XCUIApplication) -> Bool {
        if staticTextContaining(app, "Episodes").waitForExistence(timeout: 4) { return true }
        let episodeRow = app.buttons.matching(
            NSPredicate(format: "identifier == 'home-episode-row'")
        ).firstMatch
        return episodeRow.waitForExistence(timeout: 2) || app.cells.count > 2
    }

    /// Attach the full accessibility tree as a kept string attachment.
    func dumpTree(_ app: XCUIApplication, _ name: String) {
        guard app.state == .runningForeground else { return }
        let t = XCTAttachment(string: app.debugDescription)
        t.name = name; t.lifetime = .keepAlways; add(t)
    }
}
