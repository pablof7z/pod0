import XCTest
@testable import Podcastr

/// Covers `EpisodeDetailView.readyTranscript(for:store:)` — the bridge from
/// the in-memory `Episode.transcriptState` to the on-disk `TranscriptStore`.
///
/// Each test spins up an isolated `TranscriptStore` rooted in a unique temp
/// directory so they don't trample the app's real Application Support state
/// or each other.
@MainActor
final class EpisodeDetailTranscriptTests: XCTestCase {

    // MARK: - Fixtures

    private var tempDir: URL!
    private var store: TranscriptStore!

    override func setUpWithError() throws {
        try super.setUpWithError()
        tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("readyTranscriptTests-\(UUID().uuidString)", isDirectory: true)
        store = try TranscriptStore(rootDirectory: tempDir)
    }

    override func tearDownWithError() throws {
        if let tempDir {
            try? FileManager.default.removeItem(at: tempDir)
        }
        store = nil
        tempDir = nil
        try super.tearDownWithError()
    }

    // MARK: - Tests

    /// Happy path: state is `.ready` and the transcript exists on disk —
    /// `readyTranscript` returns the persisted transcript verbatim.
    func testReadyTranscriptReturnsPersistedTranscriptWhenStateIsReady() throws {
        let episode = makeEpisode(state: .ready(source: .publisher))
        let saved = makeTranscript(episodeID: episode.id)
        try store.save(saved)

        let resolved = EpisodeDetailView.readyTranscript(for: episode, store: store)

        XCTAssertNotNil(resolved, "Expected the persisted transcript to load when state is .ready")
        XCTAssertEqual(resolved?.id, saved.id)
        XCTAssertEqual(resolved?.episodeID, episode.id)
        XCTAssertEqual(resolved?.segments.count, saved.segments.count)
        XCTAssertEqual(resolved?.segments.first?.text, saved.segments.first?.text)
    }

    /// Defensive path: state claims `.ready` but the on-disk file is missing
    /// (user wiped Application Support, store init failed, etc.) —
    /// `readyTranscript` must return `nil` so the UI falls back to the
    /// in-progress / empty surface rather than crashing or rendering empty
    /// chrome.
    func testReadyTranscriptReturnsNilWhenStateIsReadyButFileMissing() {
        let episode = makeEpisode(state: .ready(source: .scribe))

        let resolved = EpisodeDetailView.readyTranscript(for: episode, store: store)

        XCTAssertNil(resolved)
    }

    /// State-gate: anything other than `.ready` yields `nil` even if a
    /// transcript happens to be on disk (e.g. an interrupted Scribe job that
    /// reset state to `.failed`).
    func testReadyTranscriptReturnsNilForNonReadyStates() throws {
        let nonReady: [TranscriptState] = [
            .none,
            .queued,
            .fetchingPublisher,
            .transcribing(progress: 0.5),
            .failed(message: "boom")
        ]
        for state in nonReady {
            let episode = makeEpisode(state: state)
            let saved = makeTranscript(episodeID: episode.id)
            try store.save(saved)

            let resolved = EpisodeDetailView.readyTranscript(for: episode, store: store)

            XCTAssertNil(
                resolved,
                "Expected nil for state \(state) — only .ready should resolve a transcript"
            )
        }
    }

    // MARK: - Helpers

    private func makeEpisode(state: TranscriptState) -> Episode {
        Episode(
            podcastID: UUID(),
            guid: "tx-test-\(UUID().uuidString)",
            title: "Episode Under Test",
            pubDate: Date(timeIntervalSince1970: 1_700_000_000),
            enclosureURL: URL(string: "https://example.com/audio.mp3")!,
            transcriptState: state
        )
    }

    private func makeTranscript(episodeID: UUID) -> Transcript {
        let speaker = Speaker(label: "host", displayName: "Host")
        return Transcript(
            episodeID: episodeID,
            language: "en-US",
            source: .publisher,
            segments: [
                Segment(start: 0, end: 4, speakerID: speaker.id, text: "Hello and welcome."),
                Segment(start: 4, end: 9, speakerID: speaker.id, text: "Today we're talking about transcripts.")
            ],
            speakers: [speaker]
        )
    }
}
