import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class RecallAnswerServiceTests: XCTestCase {
    private let episodeID = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
    private let podcastID = UUID(uuidString: "33333333-3333-3333-3333-333333333333")!

    func testGoldenRecallPreservesEveryCoreIdentityAndPlayableAnchor() async {
        let projection = goldenProjection()
        let service = makeService(projection: projection)

        let answer = await service.answer(query: "What did I hear about habits?")

        XCTAssertEqual(answer.status, .ready)
        XCTAssertEqual(answer.text, projection.evidence[0].excerpt)
        let evidence = try? XCTUnwrap(answer.evidence.first)
        XCTAssertEqual(evidence?.spanID, projection.evidence[0].spanId.stableString)
        XCTAssertEqual(evidence?.generationID, projection.evidence[0].generationId.stableString)
        XCTAssertEqual(
            evidence?.transcriptContentDigest,
            projection.evidence[0].transcriptContentDigest.stableString
        )
        XCTAssertEqual(evidence?.firstSegmentID, projection.evidence[0].firstSegmentId.stableString)
        XCTAssertEqual(evidence?.lastSegmentID, projection.evidence[0].lastSegmentId.stableString)
        XCTAssertEqual(evidence?.startMilliseconds, 47_125)
        XCTAssertEqual(evidence?.endMilliseconds, 60_000)
        XCTAssertEqual(evidence?.provenance.source, "publisher")
        XCTAssertEqual(evidence?.provenance.provider, "fixture-provider")
        XCTAssertEqual(evidence?.score.baseRank, 1)
    }

    func testGroundedRecallEmitsContentFreeOutcomeSignals() async {
        let sink = RecordingProductSignalSink()
        let service = makeService(projection: goldenProjection(), productSignals: sink)

        _ = await service.answer(query: "private question about habits")
        let captured = await sink.waitForCount(3)

        XCTAssertEqual(captured.count, 3)
        XCTAssertEqual(Set(captured.map(\.name)), [.recallAsked, .recallGrounded, .transcriptUsed])
        XCTAssertEqual(captured.first { $0.name == .recallGrounded }?.outcome, .grounded)
    }

    func testEveryKernelTerminalStageRendersExplicitly() async {
        let cases: [(RecallStage, RecallAnswer.Status)] = [
            (.noEvidence, .noEvidence),
            (.transcriptMissing, .transcriptMissing),
            (.indexMissing, .indexMissing),
            (.indexing, .indexing),
            (.indexUnavailable, .indexUnavailable),
            (.providerUnavailable, .providerUnavailable),
            (.corruptArtifact, .corruptArtifact),
            (.interrupted, .interrupted),
            (.cancelled, .cancelled),
            (.failed, .unavailable),
        ]
        for (stage, expected) in cases {
            let projection = RecallResultProjection(
                queryId: RecallQueryId(high: 1, low: 1),
                stage: stage,
                evidence: [],
                failure: nil,
                operation: nil
            )
            let answer = await makeService(projection: projection).answer(query: "unknown")
            XCTAssertEqual(answer.status, expected, "Unexpected mapping for \(stage.stableName)")
            XCTAssertTrue(answer.evidence.isEmpty)
        }
    }

    func testIncompleteProjectionCannotBecomePresentationEvidence() async {
        let invalid = RecallEvidenceProjection(
            episodeId: EpisodeId(uuid: episodeID),
            podcastId: PodcastId(uuid: podcastID),
            generationId: EvidenceGenerationId(high: 1, low: 2),
            transcriptVersionId: TranscriptVersionId(high: 3, low: 4),
            transcriptContentDigest: ContentDigest(word0: 5, word1: 6, word2: 7, word3: 8),
            spanId: EvidenceSpanId(high: 9, low: 10),
            firstSegmentId: TranscriptSegmentId(high: 11, low: 12),
            lastSegmentId: TranscriptSegmentId(high: 13, low: 14),
            startSegmentOrdinal: 0,
            endSegmentOrdinalExclusive: 1,
            startMilliseconds: 60_000,
            endMilliseconds: 47_125,
            excerpt: "Invalid bounds",
            speakerId: nil,
            provenance: provenance,
            score: score
        )
        let projection = RecallResultProjection(
            queryId: RecallQueryId(high: 1, low: 2),
            stage: .ready,
            evidence: [invalid],
            failure: nil,
            operation: nil
        )

        let answer = await makeService(projection: projection).answer(query: "identity")

        XCTAssertEqual(answer.status, .unavailable)
        XCTAssertTrue(answer.evidence.isEmpty)
    }

    func testCancellationRendersOnlyTheKernelCancellationProjection() async {
        let service = RecallAnswerService(search: { _, _, _ in
            do {
                try await Task.sleep(for: .seconds(5))
                return self.goldenProjection()
            } catch {
                return RecallResultProjection(
                    queryId: RecallQueryId(high: 1, low: 3),
                    stage: .cancelled,
                    evidence: [],
                    failure: nil,
                    operation: nil
                )
            }
        }, metadata: { _ in nil })
        let task = Task { await service.answer(query: "cancel me") }
        await Task.yield()
        task.cancel()

        let answer = await task.value

        XCTAssertEqual(answer.status, .cancelled)
        XCTAssertTrue(answer.evidence.isEmpty)
    }

    func testRecallAnswerSurvivesChatMessagePersistenceRoundTrip() throws {
        let answer = RecallAnswer(text: "Grounded answer", status: .ready)
        let message = ChatMessage(role: .assistant, text: answer.text, recallAnswer: answer)

        let decoded = try JSONDecoder().decode(
            ChatMessage.self,
            from: JSONEncoder().encode(message)
        )

        XCTAssertEqual(decoded, message)
        XCTAssertEqual(decoded.recallAnswer, answer)
    }

    func testEvidencePlaybackHandoffSeeksToExactCoreMoment() async throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let podcast = Podcast(id: podcastID, title: "Practical Minds")
        let episode = try await made.store.upsertExternalEpisodeAndWait(
            podcastID: podcastID,
            feedURL: nil,
            podcastTitle: podcast.title,
            audioURL: URL(string: "https://example.test/episode.mp3")!,
            title: "The Habit Loop",
            publishedAt: Date(timeIntervalSince1970: 1_700_000_000),
            imageURL: nil,
            duration: 300
        )
        let played = expectation(description: "Rust requested recall playback")
        let host = RecordingPlaybackHost(played: played)
        let client = try XCTUnwrap(made.store.sharedLibrary)
        client.deferredPlaybackHost.attach(host)
        client.playbackHostAttached = true
        let playback = PlaybackState()
        client.attachPlayback(playback, store: made.store)
        let evidence = RecallEvidence(
            spanID: "span",
            episodeID: episode.id,
            podcastID: podcastID,
            episodeTitle: episode.title,
            podcastTitle: podcast.title,
            generationID: "generation",
            transcriptVersionID: "transcript",
            transcriptContentDigest: "digest",
            firstSegmentID: "first",
            lastSegmentID: "last",
            startSegmentOrdinal: 1,
            endSegmentOrdinalExclusive: 2,
            startMilliseconds: 47_125,
            endMilliseconds: 60_000,
            excerpt: "Evidence",
            speakerID: nil,
            provenance: RecallEvidenceProvenance(
                source: "publisher",
                provider: nil,
                sourcePayloadDigest: "payload"
            ),
            score: RecallEvidenceScore(
                vectorRRFUnits: 10,
                lexicalRRFUnits: 10,
                totalRRFUnits: 20,
                baseRank: 1,
                rerankRank: nil
            )
        )

        XCTAssertTrue(RecallPlaybackHandoff.open(evidence, store: made.store, playback: playback))
        await fulfillment(of: [played], timeout: 5)
        XCTAssertEqual(host.episodeID?.uuid, episode.id)
        XCTAssertEqual(host.positionMilliseconds, 47_125)
        XCTAssertTrue(host.didPlay)
    }

    private func makeService(
        projection: RecallResultProjection,
        productSignals: any ProductSignalSink = DiscardingProductSignalSink.shared
    ) -> RecallAnswerService {
        RecallAnswerService(
            search: { _, _, _ in projection },
            productSignals: productSignals
        ) { episodeID in
            guard episodeID == self.episodeID else { return nil }
            return RecallEvidenceMetadata(
                episodeTitle: "The Habit Loop",
                podcastTitle: "Practical Minds"
            )
        }
    }

    private func goldenProjection() -> RecallResultProjection {
        RecallResultProjection(
            queryId: RecallQueryId(high: 42, low: 7),
            stage: .ready,
            evidence: [RecallEvidenceProjection(
                episodeId: EpisodeId(uuid: episodeID),
                podcastId: PodcastId(uuid: podcastID),
                generationId: EvidenceGenerationId(high: 1, low: 2),
                transcriptVersionId: TranscriptVersionId(high: 3, low: 4),
                transcriptContentDigest: ContentDigest(word0: 5, word1: 6, word2: 7, word3: 8),
                spanId: EvidenceSpanId(high: 9, low: 10),
                firstSegmentId: TranscriptSegmentId(high: 11, low: 12),
                lastSegmentId: TranscriptSegmentId(high: 13, low: 14),
                startSegmentOrdinal: 2,
                endSegmentOrdinalExclusive: 4,
                startMilliseconds: 47_125,
                endMilliseconds: 60_000,
                excerpt: "Small habits become durable when the cue is obvious.",
                speakerId: SpeakerId(high: 15, low: 16),
                provenance: provenance,
                score: score
            )],
            failure: nil,
            operation: nil
        )
    }

    private var provenance: Pod0Core.TranscriptProvenance {
        Pod0Core.TranscriptProvenance(
            source: .publisher,
            provider: "fixture-provider",
            sourcePayloadDigest: ContentDigest(word0: 17, word1: 18, word2: 19, word3: 20)
        )
    }

    private var score: RecallScoreProjection {
        RecallScoreProjection(
            vectorRrfUnits: 10,
            lexicalRrfUnits: 11,
            totalRrfUnits: 21,
            baseRank: 1,
            rerankRank: nil
        )
    }
}
