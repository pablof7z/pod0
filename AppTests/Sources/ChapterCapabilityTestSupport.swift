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
    static let publisherPayload = Data(
        #"{"version":"1.2.0","chapters":[{"startTime":0,"title":"Opening"},{"startTime":50,"title":"Source"}]}"#.utf8
    )
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

    static func publisherRequest(
        sourceURL: String = "https://example.test/chapters.json"
    ) -> PublisherChapterCapabilityRequest {
        PublisherChapterCapabilityRequest(
            episodeID: episodeID,
            podcastID: podcastID,
            sourceURL: sourceURL,
            generatedAt: generatedAt,
            durationMilliseconds: 100_000,
            deadlineAt: nil
        )
    }

    static func modelRequest(
        systemPrompt: String = "Return chapter JSON.",
        userPrompt: String = "Use this bounded transcript evidence."
    ) -> ModelChapterCapabilityRequest {
        ModelChapterCapabilityRequest(
            episodeID: episodeID,
            podcastID: podcastID,
            formatVersion: 1,
            requestedTranscriptVersionID: transcriptID,
            requestedTranscriptContentDigest: transcriptDigest,
            selectedTranscriptVersionID: transcriptID,
            selectedTranscriptContentDigest: transcriptDigest,
            policyVersion: 1,
            provider: "openrouter",
            model: "fixture-model-v1",
            systemPrompt: systemPrompt,
            userPrompt: userPrompt,
            generatedAt: generatedAt,
            durationMilliseconds: 100_000,
            mode: .generate
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

    static func publisherResponse(
        bytes: Data = publisherPayload,
        contentType: String = "application/json"
    ) -> ChapterPublisherTransportResponse {
        ChapterPublisherTransportResponse(
            bytes: bytes,
            responseURL: "https://cdn.example.test/chapters.json",
            contentType: contentType,
            entityTag: "\"chapters-v1\"",
            lastModified: "Mon, 20 Jul 2026 00:00:00 GMT",
            httpStatus: 200
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

struct StubChapterPublisherTransport: ChapterPublisherTransporting {
    let result: Result<ChapterPublisherTransportResponse, ChapterCapabilityFailure>

    func fetch(
        _: PublisherChapterCapabilityRequest,
        maximumResponseBytes _: UInt64
    ) async -> Result<ChapterPublisherTransportResponse, ChapterCapabilityFailure> {
        result
    }
}

struct StubChapterModelTransport: ChapterModelTransporting {
    let result: Result<ChapterModelTransportResponse, ChapterCapabilityFailure>

    func execute(
        _: ModelChapterCapabilityRequest,
        maximumCompletionBytes _: UInt64
    ) async -> Result<ChapterModelTransportResponse, ChapterCapabilityFailure> {
        result
    }
}

actor SuspendingChapterPublisherTransport: ChapterPublisherTransporting {
    private var continuation: CheckedContinuation<
        Result<ChapterPublisherTransportResponse, ChapterCapabilityFailure>, Never
    >?
    private var started = false

    func fetch(
        _: PublisherChapterCapabilityRequest,
        maximumResponseBytes _: UInt64
    ) async -> Result<ChapterPublisherTransportResponse, ChapterCapabilityFailure> {
        started = true
        return await withCheckedContinuation { continuation = $0 }
    }

    func waitUntilStarted() async {
        while !started { await Task.yield() }
    }

    func finish(
        _ result: Result<ChapterPublisherTransportResponse, ChapterCapabilityFailure>
    ) {
        continuation?.resume(returning: result)
        continuation = nil
    }
}
