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
    func snap(_ app: XCUIApplication, _ name: String) {
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

    /// Attach the full accessibility tree as a kept string attachment.
    func dumpTree(_ app: XCUIApplication, _ name: String) {
        let t = XCTAttachment(string: app.debugDescription)
        t.name = name; t.lifetime = .keepAlways; add(t)
    }
}
