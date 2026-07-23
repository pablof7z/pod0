import XCTest
@testable import Podcastr

final class AgentProviderRequestBodyTests: XCTestCase {
    func testOpenRouterOmitsEmptyToolListForFinalAnswer() {
        let body = AgentOpenRouterClient.requestBody(
            messages: [["role": "user", "content": "Answer now"]],
            tools: [],
            model: "test/model"
        )

        XCTAssertNil(body["tools"])
    }

    func testOllamaOmitsEmptyToolListForFinalAnswer() {
        let body = AgentOllamaClient.requestBody(
            messages: [["role": "user", "content": "Answer now"]],
            tools: [],
            model: "test-model"
        )

        XCTAssertNil(body["tools"])
    }

    func testProviderBodiesStillIncludeNonEmptyToolSchemas() {
        let tools = [["type": "function"]]

        XCTAssertNotNil(AgentOpenRouterClient.requestBody(
            messages: [],
            tools: tools,
            model: "test/model"
        )["tools"])
        XCTAssertNotNil(AgentOllamaClient.requestBody(
            messages: [],
            tools: tools,
            model: "test-model"
        )["tools"])
    }
}
