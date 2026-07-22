import Pod0Core
import XCTest

final class TranscriptWorkflowContractBindingTests: XCTestCase {
    func testSwiftBindingPlansUserRequestAndValidatesCapabilities() {
        let episodeID = EpisodeId(high: 0, low: 501)
        let plan = planTranscriptWorkflow(input: TranscriptWorkflowPlanInput(
            episodeId: episodeID,
            sourceRevision: "audio-v1",
            committedTranscript: nil,
            selectedEvidenceInputVersion: nil,
            origin: .user,
            configuredProvider: .assemblyAi,
            configuredModel: "universal-3-pro",
            remoteAudioUrl: "https://example.test/audio.mp3",
            localAudioUrl: nil,
            publisherTranscriptUrl: "https://example.test/transcript.vtt",
            publisherMimeHint: "text/vtt",
            autoPublisherEnabled: true,
            autoProviderEnabled: true,
            credentialAvailable: true,
            embeddingSpaceId: "embedding-space-v1"
        ))

        XCTAssertEqual(plan.generation, .ensure)
        XCTAssertEqual(plan.request?.publisherFirst, false)
        XCTAssertEqual(plan.request?.provider, .assemblyAi)

        XCTAssertEqual(
            validateTranscriptCapabilityRequest(request: .fetchPublisher(
                context: TranscriptCapabilityContext(
                    episodeId: episodeID,
                    podcastId: PodcastId(high: 0, low: 502),
                    sourceRevision: "audio-v1"
                ),
                sourceUrl: "https://example.test/transcript.vtt",
                mimeHint: "text/vtt",
                maximumResponseBytes: 1_024
            )),
            .accepted
        )
        XCTAssertEqual(
            validateTranscriptCapabilityRequest(request: .submitProvider(
                context: TranscriptCapabilityContext(
                    episodeId: episodeID,
                    podcastId: PodcastId(high: 0, low: 502),
                    sourceRevision: "audio-v1"
                ),
                attemptId: TranscriptAttemptId(high: 0, low: 1),
                submissionFenceId: TranscriptSubmissionFenceId(high: 0, low: 2),
                provider: .appleSpeech,
                model: "apple-speech-v1",
                audioUrl: "file:///tmp/audio.m4a",
                maximumResponseBytes: 1_024
            )),
            .rejected(code: .unsupportedProvider)
        )
        XCTAssertEqual(
            validateTranscriptCapabilityObservation(observation: .providerAccepted(
                externalOperationId: "provider-operation-1",
                providerStatus: "queued"
            )),
            .accepted
        )
    }
}
