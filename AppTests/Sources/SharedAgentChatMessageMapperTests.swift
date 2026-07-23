import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class SharedAgentChatMessageMapperTests: XCTestCase {
    func testMapsOldestTurnFirstWithStableMessageIdentifiers() {
        let older = turn(
            id: AgentTurnId(high: 1, low: 10),
            stage: .completed,
            messages: [
                AgentMessageProjection(role: .user, content: "First question"),
                AgentMessageProjection(role: .assistant, content: "First answer"),
            ]
        )
        let newer = turn(
            id: AgentTurnId(high: 1, low: 20),
            stage: .completed,
            messages: [AgentMessageProjection(role: .user, content: "Second question")]
        )

        let messages = SharedAgentChatMessageMapper.messages(from: [newer, older])

        XCTAssertEqual(messages.map(\.text), ["First question", "First answer", "Second question"])
        XCTAssertEqual(messages[0].id, older.turnId.messageUUID(at: 0))
        XCTAssertEqual(messages[1].id, older.turnId.messageUUID(at: 1))
    }

    func testToolPayloadIsNotRenderedAndSafeFailureIsVisible() {
        let projection = turn(
            id: AgentTurnId(high: 3, low: 40),
            stage: .failed,
            messages: [AgentMessageProjection(
                role: .tool,
                content: #"{"private":"provider payload"}"#
            )],
            safeFailure: "The action could not be completed."
        )

        let messages = SharedAgentChatMessageMapper.messages(from: [projection])

        XCTAssertEqual(messages.map(\.text), [
            "Agent action completed",
            "The action could not be completed.",
        ])
        XCTAssertEqual(messages[0].role, .toolBatch(batchID: messages[0].id, count: 1))
        XCTAssertEqual(messages[1].role, .error)
    }

    func testTypedRecallEvidenceIsAttachedOnlyToTheFinalAssistantAnswer() throws {
        let episodeID = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
        let projection = turn(
            id: AgentTurnId(high: 5, low: 60),
            stage: .completed,
            messages: [
                AgentMessageProjection(role: .user, content: "What did they say about habits?"),
                AgentMessageProjection(role: .assistant, content: "I’ll check the transcript."),
                AgentMessageProjection(role: .tool, content: #"{"private":"raw tool result"}"#),
                AgentMessageProjection(role: .assistant, content: "The episode recommends obvious cues."),
            ],
            recallEvidence: [evidence(episodeID: episodeID)]
        )

        let messages = SharedAgentChatMessageMapper.messages(from: [projection]) { id in
            guard id == episodeID else { return nil }
            return RecallEvidenceMetadata(
                episodeTitle: "The Habit Loop",
                podcastTitle: "Practical Minds"
            )
        }

        XCTAssertNil(messages[1].recallAnswer)
        XCTAssertEqual(messages[2].text, "Agent action completed")
        let answer = try XCTUnwrap(messages[3].recallAnswer)
        XCTAssertEqual(answer.text, "The episode recommends obvious cues.")
        XCTAssertEqual(answer.evidence.map(\.episodeTitle), ["The Habit Loop"])
        XCTAssertEqual(answer.evidence.map(\.startMilliseconds), [47_125])
    }

    private func turn(
        id: AgentTurnId,
        stage: AgentTurnStage,
        messages: [AgentMessageProjection],
        recallEvidence: [RecallEvidenceProjection] = [],
        safeFailure: String? = nil
    ) -> AgentTurnProjection {
        AgentTurnProjection(
            conversationId: ConversationId(high: 1, low: 2),
            turnId: id,
            revision: StateRevision(value: 1),
            stage: stage,
            messages: messages,
            recallEvidence: recallEvidence,
            proposal: nil,
            executionFenceId: nil,
            commit: nil,
            safeFailure: safeFailure,
            updatedAt: UnixTimestampMilliseconds(value: 1_900_000_000_000)
        )
    }

    private func evidence(episodeID: UUID) -> RecallEvidenceProjection {
        RecallEvidenceProjection(
            episodeId: EpisodeId(uuid: episodeID),
            podcastId: PodcastId(high: 3, low: 4),
            generationId: EvidenceGenerationId(high: 5, low: 6),
            transcriptVersionId: TranscriptVersionId(high: 7, low: 8),
            transcriptContentDigest: ContentDigest(word0: 9, word1: 10, word2: 11, word3: 12),
            spanId: EvidenceSpanId(high: 13, low: 14),
            firstSegmentId: TranscriptSegmentId(high: 15, low: 16),
            lastSegmentId: TranscriptSegmentId(high: 17, low: 18),
            startSegmentOrdinal: 2,
            endSegmentOrdinalExclusive: 4,
            startMilliseconds: 47_125,
            endMilliseconds: 60_000,
            excerpt: "Small habits become durable when the cue is obvious.",
            speakerId: nil,
            provenance: TranscriptProvenance(
                source: .publisher,
                provider: "fixture-provider",
                sourcePayloadDigest: ContentDigest(word0: 19, word1: 20, word2: 21, word3: 22)
            ),
            score: RecallScoreProjection(
                vectorRrfUnits: 10,
                lexicalRrfUnits: 11,
                totalRrfUnits: 21,
                baseRank: 1,
                rerankRank: nil
            )
        )
    }
}
