import Foundation
import Pod0Core
@testable import Podcastr

enum ChapterCapabilityFixtures {
    static let episodeID = EpisodeId(high: 11, low: 102)
    static let podcastID = PodcastId(high: 12, low: 102)
    static let generatedAt = UnixTimestampMilliseconds(value: 1_721_322_123_456)
    static let transcriptID = TranscriptVersionId(high: 13, low: 102)
    static let transcriptDigest = ContentDigest(word0: 1, word1: 2, word2: 3, word3: 4)
    static let sourceDigest = ContentDigest(word0: 5, word1: 6, word2: 7, word3: 8)
    static let modelCompletion = #"{"chapters":[{"start":0,"title":"Generated"},{"start":50,"title":"Source"}],"ads":[]}"#

    static func envelope(
        id: UInt64,
        request: ChapterCapabilityRequest
    ) -> ChapterCapabilityRequestEnvelope {
        ChapterCapabilityRequestEnvelope(
            requestID: HostRequestId(high: 102, low: id),
            cancellationID: CancellationId(high: 102, low: id),
            request: request
        )
    }

    static func modelRequest(
        systemPrompt: String = "Return chapter JSON.",
        userPrompt: String = "Use this bounded transcript evidence.",
        provider: String = "openrouter",
        model: String = "fixture-model-v1",
        maximumCompletionBytes: UInt64 = 1_048_576
    ) -> ModelChapterCapabilityRequest {
        ModelChapterCapabilityRequest(
            planned: PlannedChapterModelRequest(
                sourceVersion: "fixture-source-v1",
                episodeId: episodeID,
                podcastId: podcastID,
                formatVersion: 1,
                requestedTranscriptVersionId: transcriptID,
                requestedTranscriptContentDigest: transcriptDigest,
                selectedTranscriptVersionId: transcriptID,
                selectedTranscriptContentDigest: transcriptDigest,
                policyVersion: 1,
                provider: provider,
                model: model,
                systemPrompt: systemPrompt,
                userPrompt: userPrompt,
                responseFormat: .jsonObject,
                maximumCompletionBytes: maximumCompletionBytes,
                durationMilliseconds: 100_000,
                mode: .generate,
                expectedArtifactSource: .generated,
                expectedChapterSelectionRevision: StateRevision(value: 0)
            ),
            generatedAt: generatedAt
        )
    }

    static func agentRequest() -> AgentChapterCapabilityRequest {
        AgentChapterCapabilityRequest(
            episodeID: episodeID,
            podcastID: podcastID,
            compositionRevision: "agent-composition-v1",
            policyVersion: 1,
            provider: "elevenlabs",
            model: "eleven-multilingual-v2",
            sourcePayloadDigest: sourceDigest,
            generatedAt: generatedAt,
            durationMilliseconds: 30_000,
            items: [
                AgentComposedChapterItem(
                    startSeconds: 0,
                    endSeconds: 10,
                    title: "Synthesis",
                    summary: nil,
                    imageUrl: nil,
                    linkUrl: nil,
                    includeInTableOfContents: true,
                    sourceEpisodeId: nil
                ),
                AgentComposedChapterItem(
                    startSeconds: 10,
                    endSeconds: 30,
                    title: "Source moment",
                    summary: nil,
                    imageUrl: nil,
                    linkUrl: "https://example.test/source?t=42",
                    includeInTableOfContents: true,
                    sourceEpisodeId: EpisodeId(high: 99, low: 7)
                ),
            ]
        )
    }

    static func modelResponse(
        completion: String = modelCompletion,
        provider: String = "openrouter",
        model: String = "resolved-model-v1"
    ) -> ChapterModelTransportResponse {
        ChapterModelTransportResponse(
            completion: completion,
            provider: provider,
            model: model,
            usage: ChapterModelUsage(
                promptTokens: 20,
                completionTokens: 10,
                cachedTokens: 5,
                reasoningTokens: 2,
                costUSD: 0.001
            )
        )
    }
}

struct StubChapterModelTransport: ChapterModelTransporting {
    let result: Result<ChapterModelTransportResponse, ChapterCapabilityFailure>

    func execute(
        _: ModelChapterCapabilityRequest
    ) async -> Result<ChapterModelTransportResponse, ChapterCapabilityFailure> {
        result
    }
}

actor SuspendingChapterModelTransport: ChapterModelTransporting {
    private var continuation: CheckedContinuation<
        Result<ChapterModelTransportResponse, ChapterCapabilityFailure>, Never
    >?
    private var started = false

    func execute(
        _: ModelChapterCapabilityRequest
    ) async -> Result<ChapterModelTransportResponse, ChapterCapabilityFailure> {
        started = true
        return await withCheckedContinuation { continuation = $0 }
    }

    func waitUntilStarted() async {
        while !started { await Task.yield() }
    }

    func finish(
        _ result: Result<ChapterModelTransportResponse, ChapterCapabilityFailure>
    ) {
        continuation?.resume(returning: result)
        continuation = nil
    }
}
