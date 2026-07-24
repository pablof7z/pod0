import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class CoreChapterModelHostTests: XCTestCase {
    private let episodeID = EpisodeId(high: 41, low: 42)
    private let submissionFenceID = ChapterModelSubmissionFenceId(high: 43, low: 44)

    override func setUp() {
        super.setUp()
        ChapterProviderStubProtocol.reset()
    }

    override func tearDown() {
        ChapterProviderStubProtocol.reset()
        super.tearDown()
    }

    func testExecuteForwardsTypedRequestAndReturnsCanonicalProviderEvidence() async {
        let execution = modelExecution()
        let opaqueCompletion = "opaque provider output\nnot chapter JSON"
        let transport = RecordingCoreChapterModelTransport(result: .success(
            ChapterModelTransportResponse(
                completion: opaqueCompletion,
                provider: "openrouter",
                model: "provider/canonical-model-v2",
                usage: ChapterModelUsage(
                    promptTokens: 21,
                    completionTokens: 13,
                    cachedTokens: 8,
                    reasoningTokens: 5,
                    costUSD: 0.001234
                )
            )
        ))
        let host = CoreChapterModelHost(transport: transport)

        let observation = await host.execute(.executeChapterModel(
            episodeId: episodeID,
            generation: 7,
            submissionFenceId: submissionFenceID,
            execution: execution
        ))

        let recordedRequests = await transport.recordedRequests()
        XCTAssertEqual(recordedRequests, [execution])
        XCTAssertEqual(observation, .chapterModelCompleted(
            episodeId: episodeID,
            generation: 7,
            submissionFenceId: submissionFenceID,
            completion: ChapterModelCompletionObservation(
                completion: opaqueCompletion,
                provider: "openrouter",
                model: "provider/canonical-model-v2",
                promptTokens: 21,
                completionTokens: 13,
                cachedTokens: 8,
                reasoningTokens: 5,
                costMicrousd: 1_234,
                providerOperationId: nil,
                providerStatus: "completed",
                providerGeneratedAt: nil
            )
        ))
    }

    func testFailurePreservesTypedCodeDetailAndRetryAfter() async {
        let transport = RecordingCoreChapterModelTransport(result: .failure(
            ChapterModelTransportFailure(
                code: .transport,
                httpStatus: 429,
                safeDetail: "Provider is rate limited",
                retryAfterMilliseconds: 45_000
            )
        ))
        let host = CoreChapterModelHost(transport: transport)

        let observation = await host.execute(.executeChapterModel(
            episodeId: episodeID,
            generation: 8,
            submissionFenceId: submissionFenceID,
            execution: modelExecution()
        ))

        XCTAssertEqual(observation, .chapterModelFailed(
            episodeId: episodeID,
            generation: 8,
            submissionFenceId: submissionFenceID,
            code: .httpResponse(statusCode: 429),
            safeDetail: "Provider is rate limited",
            retryAfterMilliseconds: 45_000
        ))
    }

    func testLiveCredentialNeverLeaksIntoBodyOrFailureObservation() async throws {
        let credential = "chapter-model-secret-do-not-leak"
        ChapterProviderStubProtocol.responseStatus = 401
        let session = makeSession()
        let host = CoreChapterModelHost(transport: LiveChapterModelTransport(
            session: session,
            openRouterEndpoint: endpoint,
            ollamaEndpoint: endpoint,
            credentialResolver: { _ in credential }
        ))

        let observation = await host.execute(.executeChapterModel(
            episodeId: episodeID,
            generation: 9,
            submissionFenceId: submissionFenceID,
            execution: modelExecution()
        ))

        XCTAssertEqual(observation, .chapterModelFailed(
            episodeId: episodeID,
            generation: 9,
            submissionFenceId: submissionFenceID,
            code: .httpResponse(statusCode: 401),
            safeDetail: "Chapter model HTTP 401",
            retryAfterMilliseconds: nil
        ))
        XCTAssertFalse(String(describing: observation).contains(credential))
        let requestBody = try XCTUnwrap(ChapterProviderStubProtocol.lastRequest?.httpBody)
        XCTAssertFalse(String(decoding: requestBody, as: UTF8.self).contains(credential))
        session.invalidateAndCancel()
    }

    func testRecoverNeverPostsAndReturnsProviderRecoveryUnavailable() async {
        let session = makeSession()
        let host = CoreChapterModelHost(transport: LiveChapterModelTransport(
            session: session,
            openRouterEndpoint: endpoint,
            ollamaEndpoint: endpoint,
            credentialResolver: { _ in "unused-credential" }
        ))

        let observation = await host.execute(.recoverChapterModelOperation(
            episodeId: episodeID,
            generation: 10,
            submissionFenceId: submissionFenceID,
            provider: "openrouter",
            model: "provider/model",
            providerOperationId: "provider-operation-1",
            providerStatus: "running",
            maximumCompletionBytes: 65_536
        ))

        XCTAssertEqual(observation, .chapterModelFailed(
            episodeId: episodeID,
            generation: 10,
            submissionFenceId: submissionFenceID,
            code: .providerRecoveryUnavailable,
            safeDetail: "Provider operation recovery is unavailable",
            retryAfterMilliseconds: nil
        ))
        XCTAssertNil(ChapterProviderStubProtocol.lastRequest)
        session.invalidateAndCancel()
    }

    private var endpoint: URL {
        URL(string: "https://provider.example.test/complete")!
    }

    private func modelExecution() -> ChapterModelExecutionRequest {
        ChapterModelExecutionRequest(
            provider: "openrouter",
            model: "requested/model-v1",
            systemPrompt: "Return one opaque JSON object.",
            userPrompt: "Use only bounded transcript evidence.",
            responseFormat: .jsonObject,
            maximumCompletionBytes: 65_536
        )
    }

    private func makeSession() -> URLSession {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ChapterProviderStubProtocol.self]
        return URLSession(configuration: configuration)
    }
}

private actor RecordingCoreChapterModelTransport: CoreChapterModelTransporting {
    private let result: Result<ChapterModelTransportResponse, ChapterModelTransportFailure>
    private var requests: [ChapterModelExecutionRequest] = []

    init(result: Result<ChapterModelTransportResponse, ChapterModelTransportFailure>) {
        self.result = result
    }

    func execute(
        _ request: ChapterModelExecutionRequest
    ) async -> Result<ChapterModelTransportResponse, ChapterModelTransportFailure> {
        requests.append(request)
        return result
    }

    func recordedRequests() -> [ChapterModelExecutionRequest] {
        requests
    }
}
