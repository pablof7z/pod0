import XCTest

/// Regression coverage for FND-001. On first launch the onboarding cover must
/// be the only reachable surface; the mounted app shell behind it must not leak
/// toolbar or tab controls into the accessibility tree.
final class OnboardingIsolationUITests: XCTestCase {
    override func setUp() {
        super.setUp()
        continueAfterFailure = false
    }

    override func tearDown() {
        XCUIApplication(bundleIdentifier: App.bundleID).terminate()
        super.tearDown()
    }

    func testFirstLaunchOnboardingHidesMainShellControls() {
        let app = XCUIApplication(bundleIdentifier: App.bundleID)
        app.launchArguments = ["--UITestSeed", "--UITestSeedOnboardingRequired"]

        XCTAssertTrue(launchApp(app), "app should reach foreground")
        XCTAssertTrue(app.buttons["Get Started"].waitForExistence(timeout: 12), "onboarding welcome should be visible")
        snap(app, "fnd001-onboarding-welcome")

        for label in [
            "Open sidebar",
            "Settings",
            "Search",
            "Open Agent",
            "Add Show",
            "Browse categories",
            "Conversations",
            "Home",
            "Library"
        ] {
            XCTAssertFalse(app.buttons[label].exists, "onboarding leaked hidden main-shell button: \(label)")
        }
        for label in ["Your shows live here"] {
            XCTAssertFalse(app.staticTexts[label].exists, "onboarding leaked hidden main-shell text: \(label)")
        }
        for identifier in ["tab-home", "tab-library", "home-search-button", "agent.open"] {
            XCTAssertFalse(
                app.buttons.matching(NSPredicate(format: "identifier == %@", identifier)).firstMatch.exists,
                "onboarding leaked hidden main-shell identifier: \(identifier)"
            )
        }
    }
}
