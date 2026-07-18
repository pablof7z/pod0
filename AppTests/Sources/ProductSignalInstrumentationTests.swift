import XCTest
@testable import Podcastr

@MainActor
final class ProductSignalInstrumentationTests: XCTestCase {
    func testStoreEmitsOnlyFirstSubscriptionAndUserAuthoredArtifacts() async {
        let sink = RecordingProductSignalSink()
        let made = AppStateTestSupport.makeIsolatedStore(productSignals: sink)
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let first = Podcast(feedURL: URL(string: "https://example.com/first.xml")!, title: "First")
        let second = Podcast(feedURL: URL(string: "https://example.com/second.xml")!, title: "Second")
        made.store.upsertPodcast(first)
        made.store.upsertPodcast(second)

        XCTAssertTrue(made.store.addSubscription(podcastID: first.id))
        XCTAssertTrue(made.store.addSubscription(podcastID: second.id))
        _ = made.store.addNote(text: "private user note")
        _ = made.store.addNote(text: "agent note", author: .agent)
        made.store.addClip(Clip(
            episodeID: UUID(),
            subscriptionID: first.id,
            startMs: 1_000,
            endMs: 2_000,
            transcriptText: "private transcript text"
        ))

        let captured = await waitForCount(3, sink: sink)
        XCTAssertEqual(captured.filter { $0.name == .firstSubscription }.count, 1)
        XCTAssertEqual(captured.filter { $0.name == .noteCreated }.count, 1)
        XCTAssertEqual(captured.filter { $0.name == .clipCreated }.count, 1)
    }

    func testMeaningfulListeningEmitsOnceWhenCrossingFiveMinutes() async {
        let sink = RecordingProductSignalSink()
        let made = AppStateTestSupport.makeIsolatedStore(productSignals: sink)
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let podcast = Podcast(title: "Show")
        let episode = Episode(
            podcastID: podcast.id,
            guid: "meaningful-listening",
            title: "Episode",
            pubDate: Date(),
            duration: 1_800,
            enclosureURL: URL(string: "https://example.com/episode.mp3")!
        )
        made.store.upsertPodcast(podcast)
        made.store.upsertEpisodes([episode], forPodcast: podcast.id)

        made.store.setEpisodePlaybackPosition(episode.id, position: 299)
        made.store.setEpisodePlaybackPosition(episode.id, position: 300)
        made.store.setEpisodePlaybackPosition(episode.id, position: 301)

        let captured = await waitForCount(1, sink: sink)
        XCTAssertEqual(captured.filter { $0.name == .meaningfulListening }.count, 1)
        XCTAssertEqual(captured.first?.outcome, .succeeded)
    }

    func testPlaybackResumeAndTypedFailureAreObserved() async {
        let sink = RecordingProductSignalSink()
        let engine = AudioEngine()
        let playback = PlaybackState(engine: engine, productSignals: sink)
        var episode = Episode(
            podcastID: UUID(),
            guid: "resume-observation",
            title: "Episode",
            pubDate: Date(),
            duration: 600,
            enclosureURL: URL(string: "https://example.com/episode.mp3")!
        )
        episode.playbackPosition = 42

        playback.setEpisode(episode)
        engine.setState(.failed(EngineError(failure: ProductFailure(code: .offline))))

        let captured = await waitForCount(2, sink: sink)
        XCTAssertEqual(captured.first { $0.name == .resumeAttempt }?.outcome, .succeeded)
        XCTAssertEqual(captured.first { $0.name == .playbackError }?.errorClass, .offline)
    }

    private func waitForCount(
        _ count: Int,
        sink: RecordingProductSignalSink
    ) async -> [ProductSignalObservation] {
        let arrived = await ProductSignalTestSupport.eventually {
            await sink.captured().count >= count
        }
        XCTAssertTrue(arrived, "Timed out waiting for product signals")
        return await sink.captured()
    }
}
