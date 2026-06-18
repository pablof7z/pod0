import XCTest

/// Black-box UI tests for the agent chat surface.
///
/// `testAgentRepliesToAMessageOnSimulator` — runs on the simulator without
/// network by injecting a deterministic stub via `--UITestAgentStub`. This is
/// the authoritative simulator-verifiable proof that the agent turn-loop, LLM
/// client, and transcript rendering all work together: it FAILS if no reply
/// bubble is rendered.
///
/// `testAgentRepliesToAMessage` — device-only (real LLM, ~2.6 GB model). Kept
/// for manual device validation; skipped in CI/simulator runs via an env-gate.
final class AgentChatUITest: XCTestCase {

    override func setUp() { continueAfterFailure = false }

    // MARK: - Simulator path (stub provider, no network required)

    /// Verifies that the full agent reply path works on the simulator.
    ///
    /// Launches the app with `--UITestAgentStub` so `AgentLLMClient` returns a
    /// deterministic canned reply instead of calling the Rust FFI (which requires
    /// a live LLM provider). The Swift turn-loop in `AgentChatSession+Turns`
    /// still executes authentically: it calls `streamCompletion`, receives the
    /// stub reply, appends an `.assistant` `ChatMessage`, transitions phase to
    /// `.idle`, and renders the bubble. The test asserts the canned reply text
    /// is visible in the transcript — a hard failure if the transcript is empty.
    func testAgentRepliesToAMessageOnSimulator() {
        // PodcastrUITests is a detached runner (no host-app dependency, no
        // TEST_TARGET_APPLICATION_BUNDLE_ID), so XCUIApplication() with no
        // arguments fails with "No target application path specified". Use the
        // same bundle-ID pattern as SmokeUITests so the runner finds the app
        // that the Podcastr scheme already installed on the simulator.
        let app = XCUIApplication(bundleIdentifier: "io.f7z.podcast")
        // --UITestSeed writes the seeded library before the kernel starts.
        // --UITestAgentStub activates the deterministic stub inside AgentLLMClient.
        app.launchArguments = ["--UITestSeed", "--UITestAgentStub"]
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 20),
                      "App failed to reach foreground")

        // 1) Open the Agent surface.
        let openAgent = app.buttons["agent.open"].firstMatch
        XCTAssertTrue(openAgent.waitForExistence(timeout: 30),
                      "UITEST-SIM: 'Open Agent' button never appeared")
        openAgent.tap()

        // 2) Locate the composer input.
        let field: XCUIElement = {
            let tv = app.textViews["agent.input"].firstMatch
            if tv.waitForExistence(timeout: 10) { return tv }
            return app.textFields["agent.input"].firstMatch
        }()
        XCTAssertTrue(field.waitForExistence(timeout: 15),
                      "UITEST-SIM: agent input field never appeared")
        field.tap()
        field.typeText("Hello, agent.")

        // 3) Send.
        let send = app.buttons["Send message"].firstMatch
        XCTAssertTrue(send.waitForExistence(timeout: 10),
                      "UITEST-SIM: Send button missing")
        send.tap()

        // 4) Wait for the stub reply to appear in the transcript.
        //    The stub bypasses network and returns synchronously, so 30 s is
        //    generous even under simulator load. The predicate looks for the
        //    unique canned-reply prefix anywhere in the visible text.
        let replyPredicate = NSPredicate(
            format: "label CONTAINS %@", "UITestStubReply"
        )
        let replyElement = app.staticTexts.matching(replyPredicate).firstMatch
        XCTAssertTrue(
            replyElement.waitForExistence(timeout: 30),
            "UITEST-SIM: agent stub reply never appeared in transcript — " +
            "turn-loop or rendering failed. Visible texts: " +
            app.staticTexts.allElementsBoundByIndex
                .prefix(40)
                .map { $0.label }
                .filter { !$0.isEmpty }
                .joined(separator: " | ")
        )

        // 5) Confirm no error banner was shown alongside the reply.
        let errorBanner = app.staticTexts["agent.error"].firstMatch
        XCTAssertFalse(errorBanner.exists,
                       "UITEST-SIM: error banner present: \"\(errorBanner.label)\"")
    }

    // MARK: - Device path (real LLM provider, opt-in)

    func testAgentRepliesToAMessage() throws {
        // Device-only, slow (first-use loads a ~2.6 GB model), and requires the
        // model to be downloaded — opt in explicitly so CI/simulator runs skip.
        try XCTSkipUnless(
            ProcessInfo.processInfo.environment["RUN_DEVICE_UI_TESTS"] == "1",
            "Set RUN_DEVICE_UI_TESTS=1 to drive the agent on a real device")

        let app = XCUIApplication(bundleIdentifier: "io.f7z.podcast")
        app.launch()

        // 1) Open the Agent (sparkles toolbar button).
        let openAgent = app.buttons["agent.open"].firstMatch
        XCTAssertTrue(openAgent.waitForExistence(timeout: 40),
                      "UITEST: 'Open Agent' button never appeared")
        openAgent.tap()

        // 2) Find the composer (axis:.vertical TextField shows up as a textView).
        let field: XCUIElement = {
            let tv = app.textViews["agent.input"].firstMatch
            if tv.waitForExistence(timeout: 10) { return tv }
            return app.textFields["agent.input"].firstMatch
        }()
        XCTAssertTrue(field.waitForExistence(timeout: 15),
                      "UITEST: agent input field never appeared")
        field.tap()
        field.typeText("Say hello in one short sentence.")

        // 3) Send.
        let send = app.buttons["Send message"].firstMatch
        XCTAssertTrue(send.waitForExistence(timeout: 10), "UITEST: Send button missing")
        send.tap()
        print("UITEST: message sent — waiting for reply or error…")

        // 4) Wait for an outcome. First use loads the ~2.6 GB model (slow), so
        //    give it generous time. Report whichever lands first.
        let errorBanner = app.staticTexts["agent.error"].firstMatch
        let deadline = Date().addingTimeInterval(180)
        var sawError = false
        while Date() < deadline {
            if errorBanner.exists {
                sawError = true
                print("UITEST: ❌ AGENT ERROR BANNER = \"\(errorBanner.label)\"")
                break
            }
            Thread.sleep(forTimeInterval: 2)
        }

        // 5) Dump what's on screen so the actual reply/state is captured.
        print("UITEST: ----- visible text on screen -----")
        for t in app.staticTexts.allElementsBoundByIndex.prefix(60) where !t.label.isEmpty {
            print("UITEST:   \(t.label)")
        }
        print("UITEST: -----------------------------------")

        XCTAssertFalse(sawError,
                       "UITEST: the agent showed an error banner: \"\(errorBanner.label)\"")
    }
}
