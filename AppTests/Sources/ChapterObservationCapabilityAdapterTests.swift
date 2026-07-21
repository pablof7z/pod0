import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class ChapterObservationCapabilityAdapterTests: XCTestCase {
    func testAgentObservationMatchesDirectRustQualification() async {
        let agentAdapter = ChapterObservationCapabilityAdapter()
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

    func testUnavailableCoreAndBoundsFailBeforeCapabilityExecution() async throws {
        let unavailable = ChapterObservationCapabilityAdapter(
            qualifier: UnavailableChapterObservationQualifier()
        )
        let missing = await response(
            from: unavailable,
            envelope: ChapterCapabilityFixtures.envelope(
                id: 4,
                request: .agent(ChapterCapabilityFixtures.agentRequest())
            )
        )
        XCTAssertEqual(missing.outcome, .failed(.coreUnavailable))

        let itemLimit = Int(try XCTUnwrap(chapterObservationLimits()).agentItems)
        let bounded = await response(
            from: ChapterObservationCapabilityAdapter(),
            envelope: ChapterCapabilityFixtures.envelope(
                id: 5,
                request: .agent(ChapterCapabilityFixtures.agentRequest(itemCount: itemLimit + 1))
            )
        )
        assertFailure(bounded, code: .responseTooLarge)
    }

    func testDuplicateRequestDeliversExactlyOnce() async {
        let capability = ChapterObservationCapabilityAdapter()
        let envelope = ChapterCapabilityFixtures.envelope(
            id: 8,
            request: .agent(ChapterCapabilityFixtures.agentRequest())
        )
        var delivered: [ChapterCapabilityResponse] = []

        capability.execute(envelope) { delivered.append($0) }
        capability.execute(envelope) { delivered.append($0) }
        await Task.yield()

        XCTAssertEqual(delivered.count, 1)
        guard case .observed = delivered.first?.outcome else {
            return XCTFail("Expected the first request to complete")
        }
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
