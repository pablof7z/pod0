import XCTest
@testable import Podcastr

/// Coverage for `TranscriptIngestService.evaluateAutoIngest(newEpisodeIDs:)`
/// — the feed-refresh-triggered batch path that finally wires
/// `Settings.autoIngestPublisherTranscripts` to actual ingestion. Without
/// this hook, the dormant-toggle bug meant publisher transcripts only
/// landed for episodes the user manually opened in detail view.
///
/// We can't exercise the real fetch (network + Kingfisher + sqlite-vec
/// indexing) in a unit test, so the assertions focus on the gating + filter
/// logic via the empty-input fast paths the helper uses. The service's
/// `inFlight` set + `attach` requirement prevent any actual network call
/// from firing here.
@MainActor
final class TranscriptAutoIngestTests: XCTestCase {

    // MARK: - Empty-input fast paths

    func testNoOpForEmptyIDList() {
        // `evaluateAutoIngest` should bail before any state lookup when
        // there are no IDs to consider.
        let service = TranscriptIngestService()
        // No throw, no crash, no async side effects: the call is sync.
        service.evaluateAutoIngest(newEpisodeIDs: [])
    }

    func testNoOpWhenStoreUnattached() {
        // The shared RAGService starts unattached in test contexts. The
        // helper logs a warning and returns rather than crashing.
        let service = TranscriptIngestService()
        service.evaluateAutoIngest(newEpisodeIDs: [UUID(), UUID()])
    }

    // MARK: - Settings default

    func testAutoIngestDefaultsOn() {
        // The agent layer (RAG, wiki, briefings) only works once
        // transcripts exist. Defaulting off would leave most users with no
        // transcripts despite the feeds shipping them — the very bug this
        // helper exists to fix. Lock the default in.
        let s = Settings()
        XCTAssertTrue(s.autoIngestPublisherTranscripts)
        // Scribe-fallback default is also load-bearing for the cross-episode
        // RAG marquee story — most indie podcasts don't ship publisher
        // transcripts, so without auto-Scribe the agent's library-wide chat
        // comes up dark for those subscriptions.
        XCTAssertTrue(s.autoFallbackToScribe)
    }

    // MARK: - Candidate selection branching

    func testCandidatesIncludesPublisherEpisodesWhenSettingOn() {
        let pubEp = Self.makeEpisode(hasPublisherURL: true)
        let bareEp = Self.makeEpisode(hasPublisherURL: false)
        var settings = Settings()
        settings.autoIngestPublisherTranscripts = true
        settings.autoFallbackToScribe = false   // Scribe-disabled; only publisher counts

        let ids = TranscriptIngestService.autoIngestCandidates(
            among: [pubEp, bareEp],
            settings: settings,
            elevenLabsKey: "anything"   // ignored when autoFallbackToScribe == false
        )
        XCTAssertEqual(ids, [pubEp.id])
    }

    func testCandidatesIncludesNonPublisherEpisodesWhenScribeConfigured() {
        // The unlock for cross-episode RAG: shows that don't ship a
        // <podcast:transcript> element (most indie podcasts) used to be
        // skipped by `evaluateAutoIngest` even with Scribe configured + on.
        let pubEp = Self.makeEpisode(hasPublisherURL: true)
        let bareEp = Self.makeEpisode(hasPublisherURL: false)
        var settings = Settings()
        settings.autoIngestPublisherTranscripts = true
        settings.autoFallbackToScribe = true

        let ids = TranscriptIngestService.autoIngestCandidates(
            among: [pubEp, bareEp],
            settings: settings,
            elevenLabsKey: "sk-test-key"
        )
        XCTAssertEqual(Set(ids), Set([pubEp.id, bareEp.id]))
    }

    func testCandidatesExcludesNonPublisherEpisodesWhenNoElevenLabsKey() {
        // `autoFallbackToScribe` on but no key means we'd just bounce off
        // ElevenLabs — don't waste work queueing those.
        let pubEp = Self.makeEpisode(hasPublisherURL: true)
        let bareEp = Self.makeEpisode(hasPublisherURL: false)
        var settings = Settings()
        settings.autoIngestPublisherTranscripts = true
        settings.autoFallbackToScribe = true

        let ids = TranscriptIngestService.autoIngestCandidates(
            among: [pubEp, bareEp],
            settings: settings,
            elevenLabsKey: nil
        )
        XCTAssertEqual(ids, [pubEp.id])

        let idsEmptyKey = TranscriptIngestService.autoIngestCandidates(
            among: [pubEp, bareEp],
            settings: settings,
            elevenLabsKey: ""   // empty string treated same as missing
        )
        XCTAssertEqual(idsEmptyKey, [pubEp.id])
    }

    func testCandidatesIncludesNonPublisherEpisodesWhenAssemblyAIConfigured() {
        let bareEp = Self.makeEpisode(hasPublisherURL: false)
        var settings = Settings()
        settings.sttProvider = .assemblyAI
        settings.autoIngestPublisherTranscripts = false
        settings.autoFallbackToScribe = true

        let ids = TranscriptIngestService.autoIngestCandidates(
            among: [bareEp],
            settings: settings,
            elevenLabsKey: nil,
            assemblyAIKey: "assemblyai-test-key"
        )

        XCTAssertEqual(ids, [bareEp.id])
    }

    func testCandidatesEmptyWhenBothPathsDisabled() {
        let pubEp = Self.makeEpisode(hasPublisherURL: true)
        let bareEp = Self.makeEpisode(hasPublisherURL: false)
        var settings = Settings()
        settings.autoIngestPublisherTranscripts = false
        settings.autoFallbackToScribe = false

        let ids = TranscriptIngestService.autoIngestCandidates(
            among: [pubEp, bareEp],
            settings: settings,
            elevenLabsKey: "sk-test-key"
        )
        XCTAssertTrue(ids.isEmpty)
    }

    func testCandidatesSkipsAlreadyReadyEpisodes() {
        let readyEp = Self.makeEpisode(hasPublisherURL: true, transcriptState: .ready(source: .publisher))
        let pendingEp = Self.makeEpisode(hasPublisherURL: true)
        var settings = Settings()
        settings.autoIngestPublisherTranscripts = true

        let ids = TranscriptIngestService.autoIngestCandidates(
            among: [readyEp, pendingEp],
            settings: settings,
            elevenLabsKey: nil
        )
        XCTAssertEqual(ids, [pendingEp.id])
    }

    // MARK: - Helpers

    private static func makeEpisode(
        hasPublisherURL: Bool,
        transcriptState: TranscriptState = .none
    ) -> Episode {
        Episode(
            podcastID: UUID(),
            guid: "guid-\(UUID().uuidString)",
            title: "Test",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/audio.mp3")!,
            publisherTranscriptURL: hasPublisherURL ? URL(string: "https://example.com/t.json") : nil,
            publisherTranscriptType: hasPublisherURL ? .json : nil,
            transcriptState: transcriptState
        )
    }
}
