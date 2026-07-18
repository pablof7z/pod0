import XCTest
@testable import Podcastr

@MainActor
final class RecallAnswerServiceTests: XCTestCase {
    private let chunkID = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
    private let episodeID = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
    private let podcastID = UUID(uuidString: "33333333-3333-3333-3333-333333333333")!

    func testGoldenRecallAnswerPreservesPlayableCitationIdentityAndProvenance() async {
        let rag = RecallRAGStub(hits: [goldenHit], readiness: .ready)
        let service = RecallAnswerService(rag: rag) { episodeID in
            guard episodeID == self.episodeID else { return nil }
            return RecallEvidenceMetadata(
                episodeTitle: "The Habit Loop",
                podcastTitle: "Practical Minds"
            )
        }

        let answer = await service.answer(query: "What did I hear about habits?")

        XCTAssertEqual(answer.status, .ready)
        XCTAssertEqual(answer.text, "Small habits become durable when the cue is obvious.")
        XCTAssertEqual(answer.evidence, [RecallEvidence(
            chunkID: chunkID,
            episodeID: episodeID,
            podcastID: podcastID,
            episodeTitle: "The Habit Loop",
            podcastTitle: "Practical Minds",
            artifactVersion: "transcript-v3",
            startMilliseconds: 47_125,
            endMilliseconds: 60_000,
            excerpt: "Small habits become durable when the cue is obvious.",
            provenance: "publisher"
        )])
    }

    func testEmptyRecallDistinguishesIndexingMissingAndNoEvidence() async {
        for (readiness, status) in [
            (TranscriptCorpusReadiness.indexing, RecallAnswer.Status.indexing),
            (.transcriptMissing, .transcriptMissing),
            (.ready, .noEvidence),
            (.unavailable, .unavailable),
        ] {
            let answer = await RecallAnswerService(
                rag: RecallRAGStub(hits: [], readiness: readiness),
                metadata: { _ in nil }
            ).answer(query: "unknown")
            XCTAssertEqual(answer.status, status)
            XCTAssertTrue(answer.evidence.isEmpty)
        }
    }

    func testIncompleteRetrievalRowCannotBecomeEvidence() async {
        let incomplete = TranscriptHit(
            episodeID: episodeID.uuidString,
            startSeconds: 1,
            endSeconds: 2,
            speaker: nil,
            text: "This row lacks stable evidence identity."
        )
        let answer = await RecallAnswerService(
            rag: RecallRAGStub(hits: [incomplete], readiness: .ready),
            metadata: { _ in RecallEvidenceMetadata(episodeTitle: "Episode", podcastTitle: "Show") }
        ).answer(query: "identity")

        XCTAssertEqual(answer.status, .noEvidence)
        XCTAssertTrue(answer.evidence.isEmpty)
    }

    func testCancellationProducesNoAnswerEvidence() async {
        let rag = RecallRAGStub(hits: [goldenHit], readiness: .ready, delayNanoseconds: 5_000_000_000)
        let task = Task {
            await RecallAnswerService(
                rag: rag,
                metadata: { _ in RecallEvidenceMetadata(episodeTitle: "Episode", podcastTitle: "Show") }
            ).answer(query: "cancel me")
        }
        await Task.yield()
        task.cancel()

        let answer = await task.value
        XCTAssertEqual(answer.status, .cancelled)
        XCTAssertTrue(answer.evidence.isEmpty)
    }

    func testRecallIntentDoesNotCapturePlaybackInventoryPrompt() {
        XCTAssertTrue(RecallIntentClassifier.matches("Where did I hear the idea about habits?"))
        XCTAssertTrue(RecallIntentClassifier.matches("What did the guest say about sleep?"))
        XCTAssertFalse(RecallIntentClassifier.matches("Where did I leave off?"))
        XCTAssertFalse(RecallIntentClassifier.matches("What was I listening to?"))
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

    func testCancelledRecallCannotCommitLateAssistantMessage() async {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let podcast = Podcast(id: podcastID, title: "Practical Minds")
        let episode = Episode(
            id: episodeID,
            podcastID: podcastID,
            guid: "recall-cancel",
            title: "The Habit Loop",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://example.com/episode.mp3")!
        )
        made.store.upsertPodcast(podcast)
        made.store.upsertEpisodes([episode], forPodcast: podcastID)
        let historyURL = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("recall-history-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: historyURL) }
        let rag = RecallRAGStub(
            hits: [goldenHit],
            readiness: .ready,
            delayNanoseconds: 5_000_000_000
        )
        let session = AgentChatSession(
            store: made.store,
            podcastDeps: makeRecallDeps(rag: rag),
            history: ChatHistoryStore(fileURL: historyURL),
            resumeWindow: 0,
            drainPendingContext: false
        )

        session.startRecall("What did I hear about habits?")
        let task = session.sendingTask
        await Task.yield()
        session.cancelSend()
        await task?.value

        XCTAssertFalse(session.messages.contains { $0.role == .assistant })
        XCTAssertEqual(session.phase, .idle)
    }

    func testEvidencePlaybackHandoffSeeksToExactTranscriptMoment() {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let podcast = Podcast(id: podcastID, title: "Practical Minds")
        let episode = Episode(
            id: episodeID,
            podcastID: podcastID,
            guid: "recall-play",
            title: "The Habit Loop",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://example.com/episode.mp3")!
        )
        made.store.upsertPodcast(podcast)
        made.store.upsertEpisodes([episode], forPodcast: podcastID)
        let playback = PlaybackState()
        let evidence = RecallEvidence(
            chunkID: chunkID,
            episodeID: episodeID,
            podcastID: podcastID,
            episodeTitle: episode.title,
            podcastTitle: podcast.title,
            artifactVersion: "transcript-v3",
            startMilliseconds: 47_125,
            endMilliseconds: 60_000,
            excerpt: "Evidence",
            provenance: "publisher"
        )

        XCTAssertTrue(RecallPlaybackHandoff.open(evidence, store: made.store, playback: playback))
        XCTAssertEqual(playback.episode?.id, episodeID)
        XCTAssertEqual(playback.currentTime, 47.125, accuracy: 0.001)
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
            text: "  Small habits become durable\nwhen the cue is obvious.  ",
            score: 0.92
        )
    }
}

private func makeRecallDeps(rag: PodcastAgentRAGSearchProtocol) -> PodcastAgentToolDeps {
    let inventory = MockInventory()
    return PodcastAgentToolDeps(
        rag: rag,
        summarizer: MockSummarizer(),
        fetcher: MockFetcher(),
        playback: MockPlayback(),
        library: MockLibrary(),
        inventory: inventory,
        categories: inventory,
        perplexity: MockPerplexity(),
        ttsPublisher: MockTTSPublisher(),
        directory: MockDirectory(),
        subscribe: MockSubscribe(),
        youtubeIngestion: MockYouTubeIngestion(),
        ownedPodcasts: MockOwnedPodcasts()
    )
}

private actor RecallRAGStub: PodcastAgentRAGSearchProtocol {
    let hits: [TranscriptHit]
    let readiness: TranscriptCorpusReadiness
    let delayNanoseconds: UInt64

    init(
        hits: [TranscriptHit],
        readiness: TranscriptCorpusReadiness,
        delayNanoseconds: UInt64 = 0
    ) {
        self.hits = hits
        self.readiness = readiness
        self.delayNanoseconds = delayNanoseconds
    }

    func searchEpisodes(query: String, scope: PodcastID?, limit: Int) async throws -> [EpisodeHit] { [] }

    func queryTranscripts(query: String, scope: String?, limit: Int) async throws -> [TranscriptHit] {
        if delayNanoseconds > 0 { try await Task.sleep(nanoseconds: delayNanoseconds) }
        return hits
    }

    func transcriptCorpusReadiness() async -> TranscriptCorpusReadiness { readiness }

    func findSimilarEpisodes(seedEpisodeID: EpisodeID, k: Int) async throws -> [EpisodeHit] { [] }
}
