import Foundation
import XCTest
@testable import Podcastr

final class ChapterProviderTransportTests: XCTestCase {
    override func setUp() {
        super.setUp()
        ChapterProviderStubProtocol.reset()
    }

    override func tearDown() {
        ChapterProviderStubProtocol.reset()
        super.tearDown()
    }

    func testOpenRouterReturnsExactCompletionIdentityAndSnakeCaseUsage() async throws {
        let completion = #"{"chapters":[{"start":0,"title":"Exact"}],"ads":[]}"#
        ChapterProviderStubProtocol.responseBody = try JSONSerialization.data(withJSONObject: [
            "model": "resolved/openrouter-model",
            "choices": [["message": ["content": completion]]],
            "usage": [
                "prompt_tokens": 20,
                "completion_tokens": 10,
                "cost": 0.001,
                "prompt_tokens_details": ["cached_tokens": 5],
                "completion_tokens_details": ["reasoning_tokens": 2],
            ],
        ])
        let session = makeSession()
        let transport = modelTransport(session: session)

        let result = await transport.execute(ChapterCapabilityFixtures.modelRequest(
            maximumCompletionBytes: 1_024
        ))

        guard case .success(let response) = result else {
            return XCTFail("Expected raw model response, got \(result)")
        }
        XCTAssertEqual(response.completion, completion)
        XCTAssertEqual(response.provider, "openrouter")
        XCTAssertEqual(response.model, "resolved/openrouter-model")
        XCTAssertEqual(response.usage?.promptTokens, 20)
        XCTAssertEqual(response.usage?.completionTokens, 10)
        XCTAssertEqual(response.usage?.cachedTokens, 5)
        XCTAssertEqual(response.usage?.reasoningTokens, 2)
        XCTAssertEqual(
            ChapterProviderStubProtocol.lastRequest?.value(forHTTPHeaderField: "Authorization"),
            "Bearer test-key"
        )
        let body = try XCTUnwrap(ChapterProviderStubProtocol.lastRequest?.httpBody)
        let requestJSON = try XCTUnwrap(
            JSONSerialization.jsonObject(with: body) as? [String: Any]
        )
        XCTAssertEqual(requestJSON["model"] as? String, "fixture-model-v1")
        XCTAssertEqual((requestJSON["response_format"] as? [String: String])?["type"], "json_object")
        session.invalidateAndCancel()
    }

    func testOllamaEnvelopeIsMechanicalAndDoesNotChooseFallback() async throws {
        ChapterProviderStubProtocol.responseBody = try JSONSerialization.data(withJSONObject: [
            "model": "resolved-ollama-model",
            "message": ["content": "opaque completion"],
            "prompt_eval_count": 7,
            "eval_count": 3,
        ])
        let session = makeSession()
        let transport = modelTransport(session: session)
        let request = ChapterCapabilityFixtures.modelRequest(
            provider: "ollama",
            model: "requested-ollama-model",
            maximumCompletionBytes: 1_024
        )
        let result = await transport.execute(request)

        guard case .success(let response) = result else {
            return XCTFail("Expected Ollama response, got \(result)")
        }
        XCTAssertEqual(response.completion, "opaque completion")
        XCTAssertEqual(response.provider, "ollama")
        XCTAssertEqual(response.model, "resolved-ollama-model")
        XCTAssertEqual(response.usage?.promptTokens, 7)
        XCTAssertEqual(response.usage?.completionTokens, 3)
        session.invalidateAndCancel()
    }

    func testModelFailsTypedForMissingCredentialOversizeAndMalformedEnvelope() async throws {
        let session = makeSession()
        let missing = LiveChapterModelTransport(
            session: session,
            credentialResolver: { _ in nil }
        )
        let unauthorized = await missing.execute(ChapterCapabilityFixtures.modelRequest(
            maximumCompletionBytes: 100
        ))
        assertFailure(unauthorized, code: .authentication)

        ChapterProviderStubProtocol.responseBody = try JSONSerialization.data(withJSONObject: [
            "model": "resolved-model",
            "choices": [["message": ["content": "five!"]]],
        ])
        let oversized = await modelTransport(session: session).execute(
            ChapterCapabilityFixtures.modelRequest(maximumCompletionBytes: 4)
        )
        assertFailure(oversized, code: .responseTooLarge)

        ChapterProviderStubProtocol.responseBody = Data(#"{"choices":[]}"#.utf8)
        let malformed = await modelTransport(session: session).execute(
            ChapterCapabilityFixtures.modelRequest(maximumCompletionBytes: 100)
        )
        assertFailure(malformed, code: .invalidResponseMetadata)
        session.invalidateAndCancel()
    }

    func testProviderRetryAfterIsBoundedTypedEvidence() async {
        ChapterProviderStubProtocol.responseStatus = 429
        ChapterProviderStubProtocol.responseHeaders["Retry-After"] = "30"
        let session = makeSession()

        let result = await modelTransport(session: session).execute(
            ChapterCapabilityFixtures.modelRequest(maximumCompletionBytes: 100)
        )

        assertFailure(
            result,
            code: .transport,
            status: 429,
            retryAfterMilliseconds: 30_000
        )
        session.invalidateAndCancel()
    }

    private func modelTransport(session: URLSession) -> LiveChapterModelTransport {
        let endpoint = URL(string: "https://provider.example.test/complete")!
        return LiveChapterModelTransport(
            session: session,
            openRouterEndpoint: endpoint,
            ollamaEndpoint: endpoint,
            credentialResolver: { _ in "test-key" }
        )
    }

    private func makeSession() -> URLSession {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ChapterProviderStubProtocol.self]
        return URLSession(configuration: configuration)
    }

    private func assertFailure<T>(
        _ result: Result<T, ChapterCapabilityFailure>,
        code: ChapterCapabilityFailureCode,
        status: UInt16? = nil,
        retryAfterMilliseconds: UInt64? = nil,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard case .failure(let failure) = result else {
            return XCTFail("Expected typed failure", file: file, line: line)
        }
        XCTAssertEqual(failure.code, code, file: file, line: line)
        XCTAssertEqual(failure.httpStatus, status, file: file, line: line)
        XCTAssertEqual(
            failure.retryAfterMilliseconds,
            retryAfterMilliseconds,
            file: file,
            line: line
        )
    }
}
