import Foundation
import Pod0Core
@testable import Podcastr

enum ChapterCapabilityFixtures {
    static let episodeID = EpisodeId(high: 11, low: 102)
    static let podcastID = PodcastId(high: 12, low: 102)
    static let generatedAt = UnixTimestampMilliseconds(value: 1_721_322_123_456)
    static let sourceDigest = ContentDigest(word0: 5, word1: 6, word2: 7, word3: 8)

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

    static func executionRequest(
        systemPrompt: String = "Return chapter JSON.",
        userPrompt: String = "Use this bounded transcript evidence.",
        provider: String = "openrouter",
        model: String = "fixture-model-v1",
        maximumCompletionBytes: UInt64 = 1_048_576
    ) -> ChapterModelExecutionRequest {
        ChapterModelExecutionRequest(
            provider: provider,
            model: model,
            systemPrompt: systemPrompt,
            userPrompt: userPrompt,
            responseFormat: .jsonObject,
            maximumCompletionBytes: maximumCompletionBytes
        )
    }

    static func agentRequest(itemCount: Int = 2) -> AgentChapterCapabilityRequest {
        let base = [
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
        return AgentChapterCapabilityRequest(
            episodeID: episodeID,
            podcastID: podcastID,
            compositionRevision: "agent-composition-v1",
            policyVersion: 1,
            provider: "elevenlabs",
            model: "eleven-multilingual-v2",
            sourcePayloadDigest: sourceDigest,
            generatedAt: generatedAt,
            durationMilliseconds: 30_000,
            items: (0 ..< itemCount).map { base[$0 % base.count] }
        )
    }
}
