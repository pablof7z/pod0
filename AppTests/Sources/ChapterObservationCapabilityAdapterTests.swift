import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class ChapterObservationCapabilityAdapterTests: XCTestCase {
    func testAllRawKindsMatchDirectRustQualification() async {
        let publisherAdapter = adapter(
            publisher: StubChapterPublisherTransport(result: .success(
                ChapterCapabilityFixtures.publisherResponse()
            ))
        )
        let publisher = await response(
            from: publisherAdapter,
            envelope: ChapterCapabilityFixtures.envelope(
                id: 1,
                request: .publisher(ChapterCapabilityFixtures.publisherRequest())
            )
        )
        assertDirectQualification(publisher)
        guard case let .observed(.publisher(observation), .publisher(evidence), _) = publisher.outcome else {
            return XCTFail("Expected publisher observation")
        }
        XCTAssertEqual(observation.payload, ChapterCapabilityFixtures.publisherPayload)
        XCTAssertEqual(evidence.entityTag, "\"chapters-v1\"")
        XCTAssertEqual(evidence.payloadDigest, observation.payloadDigest)

        let modelAdapter = adapter(
            model: StubChapterModelTransport(result: .success(
                ChapterCapabilityFixtures.modelResponse()
            ))
        )
        let model = await response(
            from: modelAdapter,
            envelope: ChapterCapabilityFixtures.envelope(
                id: 2,
                request: .model(ChapterCapabilityFixtures.modelRequest())
            )
        )
        assertDirectQualification(model)
        guard case let .observed(.model(observation), .model(evidence), _) = model.outcome else {
            return XCTFail("Expected model observation")
        }
        XCTAssertEqual(observation.completion, ChapterCapabilityFixtures.modelCompletion)
        XCTAssertEqual(observation.model, "resolved-model-v1")
        XCTAssertEqual(evidence.usage?.cachedTokens, 5)

        let agentAdapter = adapter()
        let agent = await response(
            from: agentAdapter,
            envelope: ChapterCapabilityFixtures.envelope(
                id: 3,
                request: .agent(ChapterCapabilityFixtures.agentRequest())
            )
        )
        assertDirectQualification(agent)
        guard case let .observed(.agent(observation), .agent(evidence), _) = agent.outcome else {
            return XCTFail("Expected agent observation")
        }
        XCTAssertEqual(observation.items.map(\.title), ["Synthesis", "Source moment"])
        XCTAssertEqual(evidence.orderedItemCount, 2)
    }

    func testUnavailableCoreAndBoundsFailBeforeCapabilityExecution() async {
        let unavailable = ChapterObservationCapabilityAdapter(
            publisherTransport: StubChapterPublisherTransport(
                result: .success(ChapterCapabilityFixtures.publisherResponse())
            ),
            modelTransport: defaultModel,
            qualifier: UnavailableChapterObservationQualifier()
        )
        let missing = await response(
            from: unavailable,
            envelope: ChapterCapabilityFixtures.envelope(
                id: 4,
                request: .publisher(ChapterCapabilityFixtures.publisherRequest())
            )
        )
        XCTAssertEqual(missing.outcome, .failed(.coreUnavailable))

        let oversizedPrompt = String(repeating: "x", count: 262_145)
        let bounded = await response(
            from: adapter(),
            envelope: ChapterCapabilityFixtures.envelope(
                id: 5,
                request: .model(ChapterCapabilityFixtures.modelRequest(
                    systemPrompt: oversizedPrompt,
                    userPrompt: ""
                ))
            )
        )
        assertFailure(bounded, code: .responseTooLarge)
    }

    func testOversizedPayloadAndMalformedMetadataFailWithoutQualification() async {
        let oversized = Data(count: 2_097_153)
        let oversizedResponse = await response(
            from: adapter(publisher: StubChapterPublisherTransport(
                result: .success(ChapterCapabilityFixtures.publisherResponse(bytes: oversized))
            )),
            envelope: ChapterCapabilityFixtures.envelope(
                id: 6,
                request: .publisher(ChapterCapabilityFixtures.publisherRequest())
            )
        )
        assertFailure(oversizedResponse, code: .responseTooLarge)

        let malformedResponse = await response(
            from: adapter(publisher: StubChapterPublisherTransport(
                result: .success(ChapterCapabilityFixtures.publisherResponse(contentType: ""))
            )),
            envelope: ChapterCapabilityFixtures.envelope(
                id: 7,
                request: .publisher(ChapterCapabilityFixtures.publisherRequest())
            )
        )
        assertFailure(malformedResponse, code: .invalidResponseMetadata)

        let oversizedIdentity = await response(
            from: adapter(model: StubChapterModelTransport(result: .success(
                ChapterCapabilityFixtures.modelResponse(
                    provider: String(repeating: "p", count: 129)
                )
            ))),
            envelope: ChapterCapabilityFixtures.envelope(
                id: 10,
                request: .model(ChapterCapabilityFixtures.modelRequest())
            )
        )
        assertFailure(oversizedIdentity, code: .invalidResponseMetadata)
    }

    func testCancellationDuplicateAndLateCompletionDeliverExactlyOnce() async {
        let publisher = SuspendingChapterPublisherTransport()
        let capability = adapter(publisher: publisher)
        let envelope = ChapterCapabilityFixtures.envelope(
            id: 8,
            request: .publisher(ChapterCapabilityFixtures.publisherRequest())
        )
        var delivered: [ChapterCapabilityResponse] = []

        capability.execute(envelope) { delivered.append($0) }
        capability.execute(envelope) { delivered.append($0) }
        await publisher.waitUntilStarted()
        capability.cancel(cancellationID: envelope.cancellationID)
        capability.cancel(cancellationID: envelope.cancellationID)
        await publisher.finish(.success(ChapterCapabilityFixtures.publisherResponse()))
        await Task.yield()

        XCTAssertEqual(delivered.count, 1)
        XCTAssertEqual(delivered[0].outcome, .failed(.cancelled))
    }

    func testShutdownIsIdempotentAndFreshAdapterCanReuseRequestAfterRelaunch() async {
        let publisher = SuspendingChapterPublisherTransport()
        let first = adapter(publisher: publisher)
        let envelope = ChapterCapabilityFixtures.envelope(
            id: 9,
            request: .publisher(ChapterCapabilityFixtures.publisherRequest())
        )
        var firstDelivery: [ChapterCapabilityResponse] = []
        first.execute(envelope) { firstDelivery.append($0) }
        await publisher.waitUntilStarted()

        first.shutdown()
        first.shutdown()
        await publisher.finish(.success(ChapterCapabilityFixtures.publisherResponse()))
        await Task.yield()
        XCTAssertEqual(firstDelivery.map(\.outcome), [.failed(.cancelled)])

        let relaunched = adapter(publisher: StubChapterPublisherTransport(
            result: .success(ChapterCapabilityFixtures.publisherResponse())
        ))
        let second = await response(from: relaunched, envelope: envelope)
        guard case .observed = second.outcome else {
            return XCTFail("Fresh transient adapter should accept the request")
        }
    }

    private var defaultModel: StubChapterModelTransport {
        StubChapterModelTransport(result: .failure(.invalidRequest("unused")))
    }

    private func adapter(
        publisher: any ChapterPublisherTransporting = StubChapterPublisherTransport(
            result: .failure(.invalidRequest("unused"))
        ),
        model: any ChapterModelTransporting = StubChapterModelTransport(
            result: .failure(.invalidRequest("unused"))
        )
    ) -> ChapterObservationCapabilityAdapter {
        ChapterObservationCapabilityAdapter(
            publisherTransport: publisher,
            modelTransport: model,
            qualifier: RustChapterObservationQualifier()
        )
    }

    private func response(
        from adapter: ChapterObservationCapabilityAdapter,
        envelope: ChapterCapabilityRequestEnvelope
    ) async -> ChapterCapabilityResponse {
        await withCheckedContinuation { continuation in
            adapter.execute(envelope) { continuation.resume(returning: $0) }
        }
    }

    private func assertDirectQualification(
        _ response: ChapterCapabilityResponse,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard case let .observed(observation, _, qualification) = response.outcome else {
            return XCTFail("Expected qualified observation", file: file, line: line)
        }
        XCTAssertEqual(
            RustChapterObservationQualifier().qualify(observation),
            qualification,
            file: file,
            line: line
        )
    }

    private func assertFailure(
        _ response: ChapterCapabilityResponse,
        code: ChapterCapabilityFailureCode,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard case .failed(let failure) = response.outcome else {
            return XCTFail("Expected typed failure", file: file, line: line)
        }
        XCTAssertEqual(failure.code, code, file: file, line: line)
    }
}
