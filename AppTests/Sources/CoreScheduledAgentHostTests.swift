import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreScheduledAgentHostTests: XCTestCase {
    func testCompletionPreservesExactRequestAndUsesRustQualifiedArtifact() async {
        let transport = ScheduledAgentTransportStub(output: "Bounded briefing")
        let host = CoreScheduledAgentHost(transport: transport)
        let request = execution()

        let result = await host.execute(request)

        XCTAssertEqual(transport.modelReference, request.modelReference)
        XCTAssertEqual(transport.context, request.context)
        XCTAssertEqual(transport.prompt, request.prompt)
        XCTAssertEqual(
            result,
            qualifyScheduledAgentCompletion(
                execution: request,
                rawOutput: "Bounded briefing"
            )
        )
    }

    func testMissingCredentialIsRawTypedFailureWithoutSecretMaterial() async {
        let host = CoreScheduledAgentHost(
            transport: ScheduledAgentTransportStub(error: AgentError.missingCredential)
        )
        let request = execution()

        guard case let .failed(occurrenceID, attemptID, code, detail, retryAfter) =
            await host.execute(request)
        else { return XCTFail("Expected typed failure") }

        XCTAssertEqual(occurrenceID, request.occurrenceId)
        XCTAssertEqual(attemptID, request.attemptId)
        XCTAssertEqual(code, .missingCredential)
        XCTAssertEqual(retryAfter, nil)
        XCTAssertLessThanOrEqual(detail?.utf8.count ?? 0, 1_024)
    }

    func testOversizedOutputFailsBeforeCrossingTheQualificationBoundary() async {
        let host = CoreScheduledAgentHost(
            transport: ScheduledAgentTransportStub(output: "12345")
        )
        var request = execution()
        request = ScheduledAgentExecutionRequest(
            occurrenceId: request.occurrenceId,
            attemptId: request.attemptId,
            promptRevision: request.promptRevision,
            prompt: request.prompt,
            modelReference: request.modelReference,
            context: request.context,
            maximumOutputBytes: 4
        )

        guard case let .failed(_, _, code, _, _) = await host.execute(request) else {
            return XCTFail("Expected invalid output")
        }
        XCTAssertEqual(code, .invalidOutput)
    }

    private func execution() -> ScheduledAgentExecutionRequest {
        ScheduledAgentExecutionRequest(
            occurrenceId: ScheduledOccurrenceId(high: 1, low: 2),
            attemptId: ScheduledAttemptId(high: 3, low: 4),
            promptRevision: ContentDigest(word0: 5, word1: 6, word2: 7, word3: 8),
            prompt: "Prepare my briefing",
            modelReference: "openrouter:test/model",
            context: [
                ScheduledAgentContextMessage(role: .system, content: "Use only supplied evidence")
            ],
            maximumOutputBytes: 16_384
        )
    }
}

@MainActor
private final class ScheduledAgentTransportStub: CoreScheduledAgentTransporting {
    private let result: Result<String, Error>
    private(set) var modelReference: String?
    private(set) var context: [ScheduledAgentContextMessage]?
    private(set) var prompt: String?

    init(output: String) {
        result = .success(output)
    }

    init(error: Error) {
        result = .failure(error)
    }

    func complete(
        modelReference: String,
        context: [ScheduledAgentContextMessage],
        prompt: String
    ) async throws -> String {
        self.modelReference = modelReference
        self.context = context
        self.prompt = prompt
        return try result.get()
    }
}
