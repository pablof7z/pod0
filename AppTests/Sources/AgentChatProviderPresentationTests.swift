import XCTest
@testable import Podcastr

final class AgentChatProviderPresentationTests: XCTestCase {
    func testConnectedProviderEnablesTheAgentComposer() {
        let presentation = AgentChatProviderPresentation.resolve(
            providerName: "OpenRouter",
            hasCredential: true
        )

        XCTAssertTrue(presentation.canCompose)
        XCTAssertEqual(presentation.title, "What do you want to know?")
        XCTAssertNil(presentation.actionLabel)
    }

    func testMissingProviderExplainsTheDisabledAgentAndOffersSetup() {
        let presentation = AgentChatProviderPresentation.resolve(
            providerName: "Ollama Cloud",
            hasCredential: false
        )

        XCTAssertFalse(presentation.canCompose)
        XCTAssertEqual(presentation.title, "Connect Ollama Cloud")
        XCTAssertEqual(
            presentation.detail,
            "The Agent needs Ollama Cloud before it can answer."
        )
        XCTAssertEqual(presentation.actionLabel, "Set up Ollama Cloud")
    }
}
