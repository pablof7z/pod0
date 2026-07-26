import Pod0Core
import XCTest
@testable import Podcastr

final class AgentTurnProductSignalMapperTests: XCTestCase {
    func testSuccessfulConversationTurnRecordsCompletionOnly() {
        let observations = AgentTurnProductSignalMapper.observations(
            for: turn(stage: .completed),
            latencyBucket: nil
        )

        XCTAssertEqual(observations.map(\.name), [.agentTurnCompleted])
        XCTAssertEqual(observations.first?.outcome, .succeeded)
    }

    func testGroundedRecallRecordsAskResultLatencyAndCompletion() {
        let observations = AgentTurnProductSignalMapper.observations(
            for: turn(
                stage: .completed,
                action: transcriptQuery,
                evidence: [recallEvidence]
            ),
            latencyBucket: .milliseconds750To1999
        )

        XCTAssertEqual(
            observations.map(\.name),
            [.recallAsked, .recallGrounded, .agentTurnCompleted]
        )
        XCTAssertEqual(
            observations.first { $0.name == .recallGrounded }?.outcome,
            .grounded
        )
        XCTAssertEqual(
            observations.first { $0.name == .recallGrounded }?.latencyBucket,
            .milliseconds750To1999
        )
    }

    func testFailedRecallRecordsTypedFailureWithoutSuccessfulTurn() {
        let observations = AgentTurnProductSignalMapper.observations(
            for: turn(stage: .failed, action: transcriptQuery),
            latencyBucket: .seconds2To4
        )

        XCTAssertEqual(
            observations.map(\.name),
            [.recallAsked, .recallGrounded]
        )
        XCTAssertEqual(observations.last?.outcome, .failed)
    }

    func testNonTerminalRecallRecordsAskWithoutPrematureResult() {
        let observations = AgentTurnProductSignalMapper.observations(
            for: turn(stage: .approvalRequired, action: transcriptQuery),
            latencyBucket: nil
        )

        XCTAssertEqual(observations.map(\.name), [.recallAsked])
    }

    func testReplayedProjectionProducesTheSameDeduplicationIDs() {
        let projection = turn(
            stage: .completed,
            action: transcriptQuery,
            evidence: [recallEvidence]
        )

        let first = AgentTurnProductSignalMapper.observations(
            for: projection,
            latencyBucket: .under250Milliseconds
        )
        let replayed = AgentTurnProductSignalMapper.observations(
            for: projection,
            latencyBucket: .under250Milliseconds
        )

        XCTAssertEqual(first.map(\.signalID), replayed.map(\.signalID))
    }

    private func turn(
        stage: AgentTurnStage,
        action: AgentToolAction? = nil,
        evidence: [RecallEvidenceProjection] = []
    ) -> AgentTurnProjection {
        AgentTurnProjection(
            conversationId: ConversationId(high: 1, low: 2),
            turnId: AgentTurnId(high: 3, low: 4),
            revision: StateRevision(value: 7),
            stage: stage,
            messages: [],
            recallEvidence: evidence,
            modelUsage: [],
            proposal: action.map {
                AgentProposalProjection(
                    proposalId: AgentProposalId(high: 5, low: 6),
                    proposalDigest: digest,
                    revision: StateRevision(value: 6),
                    action: $0,
                    requiredAuthority: .oneShotApproval
                )
            },
            executionFenceId: nil,
            commit: nil,
            safeFailure: stage == .failed ? "Recall unavailable" : nil,
            updatedAt: UnixTimestampMilliseconds(value: 1_900_000_000_000)
        )
    }

    private var transcriptQuery: AgentToolAction {
        .queryTranscripts(query: "private query", scope: .library, limit: 3)
    }

    private var recallEvidence: RecallEvidenceProjection {
        RecallEvidenceProjection(
            episodeId: EpisodeId(high: 1, low: 2),
            podcastId: PodcastId(high: 3, low: 4),
            generationId: EvidenceGenerationId(high: 5, low: 6),
            transcriptVersionId: TranscriptVersionId(high: 7, low: 8),
            transcriptContentDigest: digest,
            spanId: EvidenceSpanId(high: 9, low: 10),
            firstSegmentId: TranscriptSegmentId(high: 11, low: 12),
            lastSegmentId: TranscriptSegmentId(high: 13, low: 14),
            startSegmentOrdinal: 0,
            endSegmentOrdinalExclusive: 1,
            startMilliseconds: 1_000,
            endMilliseconds: 2_000,
            excerpt: "private transcript evidence",
            speakerId: nil,
            provenance: TranscriptProvenance(
                source: .publisher,
                provider: nil,
                sourcePayloadDigest: digest
            ),
            score: RecallScoreProjection(
                vectorRrfUnits: 1,
                lexicalRrfUnits: 1,
                totalRrfUnits: 2,
                baseRank: 1,
                rerankRank: nil
            )
        )
    }

    private var digest: ContentDigest {
        ContentDigest(word0: 1, word1: 2, word2: 3, word3: 4)
    }
}
