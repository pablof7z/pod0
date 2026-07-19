import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class RecallShadowParityTests: XCTestCase {
    private let chunkID = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
    private let episodeID = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
    private let podcastID = UUID(uuidString: "33333333-3333-3333-3333-333333333333")!

    func testRecorderReceivesEvidenceWithoutChangingAuthoritativeAnswer() async {
        var shadow: (query: String, evidence: [RecallEvidence], limit: Int)?
        let service = RecallAnswerService(
            rag: RecallRAGStub(hits: [goldenHit], readiness: .ready),
            shadow: { query, evidence, limit in shadow = (query, evidence, limit) },
            metadata: { _ in
                RecallEvidenceMetadata(
                    episodeTitle: "The Habit Loop",
                    podcastTitle: "Practical Minds"
                )
            }
        )

        let answer = await service.answer(query: "  private recall question  ", limit: 3)

        XCTAssertEqual(answer.status, .ready)
        XCTAssertEqual(shadow?.query, "private recall question")
        XCTAssertEqual(shadow?.evidence, answer.evidence)
        XCTAssertEqual(shadow?.limit, 3)
    }

    func testParityPinsEpisodeSpanTimestampsExcerptAndBaseOrder() {
        let first = sharedEvidence(
            span: EvidenceSpanId(high: 1, low: 1),
            start: 47_125,
            end: 60_000,
            excerpt: "Small habits become durable when the cue is obvious."
        )
        let secondLegacy = RecallEvidence(
            chunkID: UUID(), episodeID: episodeID, podcastID: podcastID,
            episodeTitle: "The Habit Loop", podcastTitle: "Practical Minds",
            artifactVersion: "transcript-v3", startMilliseconds: 61_000,
            endMilliseconds: 70_000, excerpt: "The next exact span.", provenance: "publisher"
        )
        let second = sharedEvidence(
            span: EvidenceSpanId(high: 1, low: 2),
            start: 61_000,
            end: 70_000,
            excerpt: "The next exact span."
        )

        XCTAssertTrue(RecallShadowParity.matches(
            legacy: [goldenEvidence, secondLegacy], shared: [first, second]
        ))
        XCTAssertFalse(RecallShadowParity.matches(
            legacy: [goldenEvidence, secondLegacy], shared: [second, first]
        ))
    }

    private var goldenHit: TranscriptHit {
        TranscriptHit(
            chunkID: chunkID.uuidString,
            episodeID: episodeID.uuidString,
            podcastID: podcastID.uuidString,
            artifactVersion: "transcript-v3",
            provenance: "publisher",
            startSeconds: 47.125,
            endSeconds: 60,
            speaker: "Host",
            text: "Small habits become durable when the cue is obvious.",
            score: 0.92
        )
    }

    private var goldenEvidence: RecallEvidence {
        RecallEvidence(
            chunkID: chunkID, episodeID: episodeID, podcastID: podcastID,
            episodeTitle: "The Habit Loop", podcastTitle: "Practical Minds",
            artifactVersion: "transcript-v3", startMilliseconds: 47_125,
            endMilliseconds: 60_000,
            excerpt: "Small habits become durable when the cue is obvious.", provenance: "publisher"
        )
    }

    private func sharedEvidence(
        span: EvidenceSpanId,
        start: UInt64,
        end: UInt64,
        excerpt: String
    ) -> RecallEvidenceProjection {
        RecallEvidenceProjection(
            episodeId: EpisodeId(uuid: episodeID),
            podcastId: PodcastId(uuid: podcastID),
            generationId: EvidenceGenerationId(high: 2, low: 2),
            transcriptVersionId: TranscriptVersionId(high: 3, low: 3),
            transcriptContentDigest: ContentDigest(word0: 1, word1: 2, word2: 3, word3: 4),
            spanId: span,
            firstSegmentId: TranscriptSegmentId(high: 4, low: 4),
            lastSegmentId: TranscriptSegmentId(high: 5, low: 5),
            startSegmentOrdinal: 0,
            endSegmentOrdinalExclusive: 1,
            startMilliseconds: start,
            endMilliseconds: end,
            excerpt: excerpt,
            speakerId: nil,
            provenance: Pod0Core.TranscriptProvenance(
                source: .publisher,
                provider: nil,
                sourcePayloadDigest: ContentDigest(word0: 5, word1: 6, word2: 7, word3: 8)
            ),
            score: RecallScoreProjection(
                vectorRrfUnits: 10,
                lexicalRrfUnits: 10,
                totalRrfUnits: 20,
                baseRank: 1,
                rerankRank: nil
            )
        )
    }
}
