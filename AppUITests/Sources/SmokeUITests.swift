import XCTest

/// Black-box UI smoke tests that drive the already-installed `io.f7z.podcast`
/// build on a physical device. No dependency on the app target — the runner
/// launches the app purely by bundle identifier and queries its accessibility
/// tree. This is the channel-proving harness for the device scenario suite.
final class SmokeUITests: XCTestCase {
    static let bundleID = "io.f7z.podcast"

    override func setUp() {
        super.setUp()
        continueAfterFailure = false
    }

    /// Launches the app and confirms it reaches the foreground with rendered
    /// content (not just a launch screen). Attaches a screenshot for review.
    func testLaunchHomeVisibleAndScreenshot() throws {
        let app = XCUIApplication(bundleIdentifier: Self.bundleID)
        app.launch()

        XCTAssertTrue(
            app.wait(for: .runningForeground, timeout: 10),
            "App did not reach foreground within 10s"
        )

        // Give the reactive snapshot a moment to render real content.
        _ = app.staticTexts.firstMatch.waitForExistence(timeout: 8)

        let staticTextCount = app.staticTexts.count
        let buttonCount = app.buttons.count
        attachScreenshot(app, name: "01-launch-home")

        // A bare launch screen has ~0 interactive elements; rendered Home has many.
        XCTAssertGreaterThan(
            staticTextCount + buttonCount, 2,
            "Home appears to show no rendered content (texts=\(staticTextCount) buttons=\(buttonCount))"
        )
    }

    /// Measures cold-launch time of the installed build. Budget asserted in the
    /// scenario suite; here we just capture the metric.
    func testColdLaunchPerformance() throws {
        let app = XCUIApplication(bundleIdentifier: Self.bundleID)
        let options = XCTMeasureOptions()
        options.invocationOptions = [.manuallyStop]
        measure(metrics: [XCTApplicationLaunchMetric()], options: options) {
            app.launch()
            _ = app.wait(for: .runningForeground, timeout: 10)
            stopMeasuring()
            app.terminate()
        }
    }

    // MARK: - Helpers

    func attachScreenshot(_ app: XCUIApplication, name: String) {
        let shot = app.screenshot()
        let attachment = XCTAttachment(screenshot: shot)
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
