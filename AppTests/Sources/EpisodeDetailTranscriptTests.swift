import XCTest
@testable import Podcastr

@MainActor
final class EpisodeDetailTranscriptTests: XCTestCase {
    func testReadyTranscriptReturnsSharedProjectionWhenStateIsReady() {
        let episode = makeEpisode(state: .ready(source: .publisher))
        let transcript = makeTranscript(episodeID: episode.id)
        let reader = StubTranscriptReader(values: [episode.id: transcript])

        let resolved = EpisodeDetailView.readyTranscript(for: episode, store: reader)

        XCTAssertEqual(resolved?.id, transcript.id)
        XCTAssertEqual(resolved?.segments.first?.text, "Hello and welcome.")
    }

    func testReadyTranscriptReturnsNilWhenProjectionIsUnavailable() {
        let episode = makeEpisode(state: .ready(source: .scribe))
        XCTAssertNil(EpisodeDetailView.readyTranscript(
            for: episode,
            store: StubTranscriptReader(values: [:])
        ))
    }

    func testTranscriptProjectionIsNotReadUntilCoreReportsReady() {
        let episode = makeEpisode(state: .none)
        let transcript = makeTranscript(episodeID: episode.id)
        XCTAssertNil(EpisodeDetailView.readyTranscript(
            for: episode,
            store: StubTranscriptReader(values: [episode.id: transcript])
        ))
    }

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
                Segment(start: 4, end: 9, speakerID: speaker.id, text: "Shared projections win."),
            ],
            speakers: [speaker]
        )
    }
}

private struct StubTranscriptReader: TranscriptReading {
    let values: [UUID: Transcript]

    func load(episodeID: UUID) -> Transcript? {
        values[episodeID]
    }
}
