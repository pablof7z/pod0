import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class ProductSignalInstrumentationTests: XCTestCase {
    func testStoreEmitsOnlyFirstSubscriptionAndUserAuthoredArtifacts() async throws {
        let sink = RecordingProductSignalSink()
        let firstURL = "https://example.com/first.xml"
        let secondURL = "https://example.com/second.xml"
        let host = QueuedCoreFeedHost([
            feedResponse(url: firstURL, guid: "first"),
            feedResponse(url: secondURL, guid: "second")
        ])
        let made = AppStateTestSupport.makeIsolatedStore(
            productSignals: sink,
            sharedFeedHost: host
        )
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let service = SubscriptionService(store: made.store)

        let first = try await service.addSubscription(feedURLString: firstURL)
        _ = try await service.addSubscription(feedURLString: secondURL)
        _ = made.store.addNote(text: "private user note")
        _ = made.store.addNote(text: "agent note", author: .agent)
        let episode = try XCTUnwrap(
            made.store.state.episodes.first { $0.podcastID == first.id }
        )
        made.store.addClip(Clip(
            episodeID: episode.id,
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

    func testMeaningfulListeningConsumesRustCommittedThresholdOnce() async throws {
        let sink = RecordingProductSignalSink()
        let fixture = makePlaybackFixture(position: 0, sink: sink)
        defer { fixture.persistence.reset() }

        fixture.playback.seek(to: 299)
        fixture.playback.seek(to: 300)
        fixture.playback.seek(to: 301)

        let captured = await waitForCount(1, sink: sink)
        XCTAssertEqual(captured.filter { $0.name == .meaningfulListening }.count, 1)
        XCTAssertNotNil(captured.first?.domainRevision)
    }

    func testPlayStartedWaitsForRustCommittedPlayingState() async throws {
        let sink = RecordingProductSignalSink()
        let fixture = makePlaybackFixture(position: 0, sink: sink)
        defer { fixture.persistence.reset() }

        fixture.playback.play()

        let captured = await waitForCount(1, sink: sink)
        XCTAssertEqual(captured.filter { $0.name == .playStarted }.count, 1)
        XCTAssertEqual(captured.first?.outcome, .succeeded)
    }

    func testTranscriptReadyObservesRustProjectionExactlyOnce() async throws {
        let sink = RecordingProductSignalSink()
        let made = AppStateTestSupport.makeIsolatedStore(productSignals: sink)
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let episode = try await made.store.upsertExternalEpisodeAndWait(
            podcastID: Podcast.unknownID,
            feedURL: nil,
            podcastTitle: "Unknown Podcast",
            audioURL: URL(string: "https://signals.example/transcript.mp3")!,
            title: "Transcript signal",
            imageURL: nil,
            duration: 60
        )
        let client = try XCTUnwrap(made.store.sharedLibrary)
        let transcript = Transcript(
            episodeID: episode.id,
            language: "en-US",
            source: .publisher,
            segments: [.init(start: 0, end: 1, text: "Rust is authoritative.")]
        )
        let context = TranscriptObservationContext(
            podcastID: episode.podcastID,
            sourceRevision: DesiredStatePlanner.audioVersion(episode),
            sourcePayloadDigest: String(repeating: "ab", count: 32),
            provider: "product-signal-test"
        )

        _ = try client.submitTranscriptObservation(transcript, context: context)
        let first = await waitForCount(1, sink: sink)
        XCTAssertEqual(first.filter { $0.name == .transcriptReady }.count, 1)

        let envelope = client.facade.snapshot(request: ProjectionRequest(
            scope: .library,
            offset: 0,
            maxItems: 200
        ))
        guard case .library(let page) = envelope.projection else {
            return XCTFail("Expected library projection")
        }
        made.store.applySharedLibrary(SharedLibrarySnapshot(
            podcasts: page.podcasts,
            subscriptions: page.subscriptions,
            episodes: page.episodes,
            operations: page.operations
        ))
        try await Task.sleep(for: .milliseconds(20))
        let replayed = await sink.captured()
        XCTAssertEqual(replayed.filter { $0.name == .transcriptReady }.count, 1)
    }

    func testPlaybackResumeAndTypedFailureAreObserved() async throws {
        let sink = RecordingProductSignalSink()
        let fixture = makePlaybackFixture(position: 42, sink: sink)
        defer { fixture.persistence.reset() }

        fixture.engine.setState(.failed(EngineError(
            failure: ProductFailure(code: .offline)
        )))

        let captured = await waitForCount(2, sink: sink)
        XCTAssertEqual(
            captured.first { $0.name == .resumeAttempt }?.outcome,
            .succeeded
        )
        XCTAssertEqual(
            captured.first { $0.name == .playbackError }?.errorClass,
            .offline
        )
    }

    private func makePlaybackFixture(
        position: TimeInterval,
        sink: RecordingProductSignalSink
    ) -> (
        persistence: Persistence,
        store: AppStateStore,
        engine: AudioEngine,
        playback: PlaybackState
    ) {
        let persistence = Persistence(fileURL: AppStateTestSupport.uniqueTempFileURL())
        let podcast = Podcast(
            feedURL: URL(string: "https://signals.example/feed.xml")!,
            title: "Signal Show",
            discoveredAt: Date(timeIntervalSince1970: 1_700_000_000)
        )
        let episode = Episode(
            podcastID: podcast.id,
            guid: "signals",
            title: "Signal Episode",
            pubDate: Date(timeIntervalSince1970: 1_700_000_100),
            duration: 1_800,
            enclosureURL: URL(string: "https://signals.example/episode.mp3")!,
            playbackPosition: position
        )
        var legacy = AppState()
        legacy.podcasts = [podcast]
        legacy.subscriptions = [PodcastSubscription(
            podcastID: podcast.id,
            subscribedAt: Date(timeIntervalSince1970: 1_700_000_050)
        )]
        legacy.episodes = [episode]
        legacy.lastPlayedEpisodeID = episode.id
        XCTAssertTrue(persistence.write(legacy, revision: 1))

        let store = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        let engine = AudioEngine()
        let playback = PlaybackState(engine: engine, productSignals: sink)
        store.sharedLibrary?.attachPlayback(playback, store: store)
        return (persistence, store, engine, playback)
    }

    private func feedResponse(url: String, guid: String) -> HostObservation {
        .feedBytesFetched(
            bytes: Data(Self.feed(guid: guid).utf8),
            entityTag: nil,
            lastModified: nil,
            responseUrl: url,
            httpStatus: 200
        )
    }

    private static func feed(guid: String) -> String {
        #"""
        <?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0"><channel><title>Signal Show</title><item>
        <title>Episode</title><guid>\#(guid)</guid>
        <enclosure url="https://example.com/\#(guid).mp3" type="audio/mpeg" />
        </item></channel></rss>
        """#
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
