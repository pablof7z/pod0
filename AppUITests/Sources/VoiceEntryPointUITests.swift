import XCTest

final class VoiceEntryPointUITests: XCTestCase {
    override func setUp() {
        super.setUp()
        continueAfterFailure = false
    }

    override func tearDown() {
        XCUIApplication(bundleIdentifier: App.bundleID).terminate()
        super.tearDown()
    }

    func testToolbarVoiceButtonPresentsVoiceView() {
        let app = App.make()
        XCTAssertTrue(launchApp(app), "App did not reach foreground")

        let voiceButton = app.buttons["voice.open"]
        XCTAssertTrue(voiceButton.waitForExistence(timeout: 12), "Voice toolbar button was not exposed")
        XCTAssertTrue(robustTap(voiceButton), "Voice toolbar button could not be tapped")

        XCTAssertTrue(
            app.otherElements["voice.view"].waitForExistence(timeout: 8),
            "Voice toolbar button did not present VoiceView"
        )
    }
}
