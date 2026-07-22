import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class CoreTranscriptHostTests: XCTestCase {
    private let episodeUUID = UUID(uuidString: "11111111-2222-3333-4444-555555555555")!
    private let podcastUUID = UUID(uuidString: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")!

    func testCompletedObservationUsesExactContextAndStableSpeakerIdentity() async throws {
        let speaker = Speaker(label: "speaker-0", displayName: "Ada")
        let transcript = Transcript(
            id: UUID(uuidString: "99999999-8888-7777-6666-555555555555")!,
            episodeID: episodeUUID,
            language: "en-US",
            source: .assemblyAI,
            segments: [Segment(
                start: 0,
                end: 1,
                speakerID: speaker.id,
                text: "Bounded evidence"
            )],
            speakers: [speaker],
            generatedAt: Date(timeIntervalSince1970: 1_700_000_000)
        )
        let host = CoreTranscriptHost(transport: StubTranscriptTransport { _ in
            .completed(
                transcript: transcript,
                externalOperationID: "operation-1",
                status: "completed"
            )
        })

        let observation = await host.execute(.executeTranscriptCapability(
            capability: submitRequest(maximumResponseBytes: 1_000_000)
        ))

        guard case .transcriptCapabilityObserved(.completed(
            let externalID,
            let status,
            let artifact
        )) = observation else {
            return XCTFail("Expected completed transcript capability")
        }
        XCTAssertEqual(externalID, "operation-1")
        XCTAssertEqual(status, "completed")
        XCTAssertEqual(artifact.episodeId.uuid, episodeUUID)
        XCTAssertEqual(artifact.podcastId.uuid, podcastUUID)
        XCTAssertEqual(artifact.sourceRevision, "audio-v1")
        XCTAssertEqual(artifact.segments[0].speakerId, artifact.speakers[0].speakerId)
        XCTAssertEqual(
            artifact.speakers[0].speakerId,
            transcriptSpeakerId(
                episodeId: EpisodeId(uuid: episodeUUID),
                sourceRevision: "audio-v1",
                label: "speaker-0"
            )
        )
    }

    func testRecoveryFailurePreservesAcceptedSubmissionPhase() async {
        let host = CoreTranscriptHost(transport: StubTranscriptTransport { request in
            guard case .recoverProvider = request else {
                throw CoreTranscriptTransportError.invalidRequest
            }
            throw CoreTranscriptTransportError.timedOut
        })

        let observation = await host.execute(.executeTranscriptCapability(
            capability: recoverRequest()
        ))

        XCTAssertEqual(observation, .transcriptCapabilityObserved(observation: .failed(
            evidence: .timedOut(submissionAuthorized: true, providerAccepted: true),
            safeDetail: "Transcript provider request timed out",
            retryAfterMilliseconds: nil
        )))
    }

    func testSubmitMissingCredentialDoesNotClaimProviderAcceptance() async {
        let host = CoreTranscriptHost(transport: StubTranscriptTransport { _ in
            throw CoreTranscriptTransportError.missingCredential
        })
        let observation = await host.execute(.executeTranscriptCapability(
            capability: submitRequest(maximumResponseBytes: 1_000_000)
        ))
        XCTAssertEqual(observation, .transcriptCapabilityObserved(observation: .failed(
            evidence: .missingCredential,
            safeDetail: "Transcript credential is unavailable",
            retryAfterMilliseconds: nil
        )))
    }

    func testOversizedAndMismatchedCompletionFailClosed() async {
        let valid = transcript(episodeID: episodeUUID, text: "Too large")
        let oversized = CoreTranscriptHost(transport: StubTranscriptTransport { _ in
            .completed(transcript: valid, externalOperationID: nil, status: nil)
        })
        let oversizedObservation = await oversized.execute(.executeTranscriptCapability(
            capability: submitRequest(maximumResponseBytes: 1)
        ))
        XCTAssertEqual(
            oversizedObservation,
            failed(.responseTooLarge, "Transcript response exceeds the core limit")
        )

        let wrongEpisode = transcript(episodeID: UUID(), text: "Wrong episode")
        let mismatched = CoreTranscriptHost(transport: StubTranscriptTransport { _ in
            .completed(transcript: wrongEpisode, externalOperationID: nil, status: nil)
        })
        let mismatchedObservation = await mismatched.execute(.executeTranscriptCapability(
            capability: submitRequest(maximumResponseBytes: 1_000_000)
        ))
        XCTAssertEqual(
            mismatchedObservation,
            failed(.invalidResponse, "Transcript provider returned an invalid response")
        )
    }

    func testCancellationReturnsTypedCapabilityCancellation() async {
        let host = CoreTranscriptHost(transport: StubTranscriptTransport { _ in
            throw CancellationError()
        })
        let observation = await host.execute(.executeTranscriptCapability(
            capability: submitRequest(maximumResponseBytes: 1_000_000)
        ))
        XCTAssertEqual(
            observation,
            .transcriptCapabilityObserved(observation: .cancelled)
        )
    }

    func testInvalidRequestIsRejectedBeforeNativeTransportOrCredentialAccess() async {
        let transport = InvalidRequestTranscriptTransport()
        let host = CoreTranscriptHost(transport: transport)
        let observation = await host.execute(.executeTranscriptCapability(
            capability: .fetchPublisher(
                context: context,
                sourceUrl: "https://example.test/transcript.vtt",
                mimeHint: "text/vtt",
                maximumResponseBytes: 0
            )
        ))

        XCTAssertEqual(observation, failed(.invalidRequest, "Invalid transcript capability"))
        let calls = await transport.callCount()
        XCTAssertEqual(calls, 0)
    }

    private func submitRequest(maximumResponseBytes: UInt64) -> TranscriptCapabilityRequest {
        .submitProvider(
            context: context,
            attemptId: TranscriptAttemptId(high: 1, low: 2),
            submissionFenceId: TranscriptSubmissionFenceId(high: 3, low: 4),
            provider: .assemblyAi,
            model: "universal-3-pro",
            audioUrl: "https://example.test/audio.mp3",
            maximumResponseBytes: maximumResponseBytes
        )
    }

    private func recoverRequest() -> TranscriptCapabilityRequest {
        .recoverProvider(
            context: context,
            attemptId: TranscriptAttemptId(high: 1, low: 2),
            submissionFenceId: TranscriptSubmissionFenceId(high: 3, low: 4),
            provider: .assemblyAi,
            model: "universal-3-pro",
            externalOperationId: "operation-1",
            providerStatus: "processing",
            maximumResponseBytes: 1_000_000
        )
    }

    private var context: TranscriptCapabilityContext {
        TranscriptCapabilityContext(
            episodeId: EpisodeId(uuid: episodeUUID),
            podcastId: PodcastId(uuid: podcastUUID),
            sourceRevision: "audio-v1"
        )
    }

    private func transcript(episodeID: UUID, text: String) -> Transcript {
        Transcript(
            episodeID: episodeID,
            language: "en",
            source: .assemblyAI,
            segments: [Segment(start: 0, end: 1, text: text)],
            generatedAt: Date(timeIntervalSince1970: 1_700_000_000)
        )
    }

    private func failed(
        _ evidence: TranscriptFailureEvidence,
        _ detail: String
    ) -> HostObservation {
        .transcriptCapabilityObserved(observation: .failed(
            evidence: evidence,
            safeDetail: detail,
            retryAfterMilliseconds: nil
        ))
    }
}

private struct StubTranscriptTransport: CoreTranscriptTransporting {
    let body: @Sendable (
        TranscriptCapabilityRequest
    ) async throws -> CoreTranscriptTransportObservation

    init(
        body: @escaping @Sendable (
            TranscriptCapabilityRequest
        ) async throws -> CoreTranscriptTransportObservation
    ) {
        self.body = body
    }

    func execute(
        _ request: TranscriptCapabilityRequest
    ) async throws -> CoreTranscriptTransportObservation {
        try await body(request)
    }
}

private actor InvalidRequestTranscriptTransport: CoreTranscriptTransporting {
    private var calls = 0

    func execute(
        _: TranscriptCapabilityRequest
    ) async throws -> CoreTranscriptTransportObservation {
        calls += 1
        throw CoreTranscriptTransportError.missingCredential
    }

    func callCount() -> Int { calls }
}
