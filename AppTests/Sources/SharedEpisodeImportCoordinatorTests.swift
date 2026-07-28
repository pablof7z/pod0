import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class SharedEpisodeImportCoordinatorTests: XCTestCase {
    func testDirectAudioShareStartsDownloadAndDrainsTheQueue() async throws {
        let (store, storeFileURL) = AppStateTestSupport.makeIsolatedStore(
            sharedFeedHost: QueuedCoreFeedHost([])
        )
        defer { AppStateTestSupport.disposeIsolatedStore(at: storeFileURL) }
        let requestStore = try makeRequestStore()
        let audioURL = URL(string: "https://media.example.com/direct-episode.mp3")!
        try requestStore.enqueue(sourceURL: audioURL)

        let resolver = SharedEpisodeResolver { url, _, _ in
            throw StubLoaderError.unexpectedRequest(url)
        }
        let coordinator = SharedEpisodeImportCoordinator(resolver: resolver)
        var imported: [UUID] = []

        await coordinator.consumePending(from: requestStore, store: store) { id in
            imported.append(id)
        }

        XCTAssertEqual(imported.count, 1)
        guard case .downloadStarted(let title) = coordinator.phase else {
            return XCTFail("Expected downloadStarted, got \(String(describing: coordinator.phase))")
        }
        XCTAssertEqual(title, "direct episode")
        XCTAssertTrue(try requestStore.pendingRequests().isEmpty)
    }

    func testFeedMatchedShareCarriesTheRealGUIDThroughToStorage() async throws {
        let (store, storeFileURL) = AppStateTestSupport.makeIsolatedStore(
            sharedFeedHost: QueuedCoreFeedHost([])
        )
        defer { AppStateTestSupport.disposeIsolatedStore(at: storeFileURL) }
        let requestStore = try makeRequestStore()
        let pageURL = URL(string: "https://overcast.fm/+episode")!
        let feedURL = URL(string: "https://feeds.example.com/show.xml")!
        let audioURL = URL(string: "https://cdn.example.com/selected.mp3")!
        try requestStore.enqueue(sourceURL: pageURL)

        let stub = StubLoader(documents: [
            pageURL: .html(
                """
                <meta property="og:title" content="Selected Episode — Test Show">
                <meta name="twitter:player:stream" content="\(audioURL.absoluteString)">
                <link rel="alternate" type="application/rss+xml" href="\(feedURL.absoluteString)">
                """,
                url: pageURL
            ),
            feedURL: .xml(
                """
                <rss version="2.0"><channel>
                  <title>Test Show from RSS</title>
                  <item>
                    <title>Selected Episode</title>
                    <guid>stable-rss-guid-42</guid>
                    <pubDate>Sun, 26 Jul 2026 12:00:00 GMT</pubDate>
                    <enclosure url="\(audioURL.absoluteString)" type="audio/mpeg"/>
                  </item>
                </channel></rss>
                """,
                url: feedURL
            )
        ])
        let resolver = SharedEpisodeResolver { url, _, _ in try await stub.document(for: url) }
        let coordinator = SharedEpisodeImportCoordinator(resolver: resolver)

        await coordinator.consumePending(from: requestStore, store: store) { _ in }

        guard case .downloadStarted = coordinator.phase else {
            return XCTFail("Expected downloadStarted, got \(String(describing: coordinator.phase))")
        }
        let podcast = try XCTUnwrap(store.state.podcasts.first { $0.feedURL == feedURL })
        let episodes = store.episodes(forPodcast: podcast.id)
        XCTAssertEqual(episodes.map(\.guid), ["stable-rss-guid-42"])
    }

    func testFailedImportIsRetriedRatherThanDiscardedUntilAttemptsAreExhausted() async throws {
        let (store, storeFileURL) = AppStateTestSupport.makeIsolatedStore(
            sharedFeedHost: QueuedCoreFeedHost([])
        )
        defer { AppStateTestSupport.disposeIsolatedStore(at: storeFileURL) }
        let requestStore = try makeRequestStore()
        let pageURL = URL(string: "https://example.com/flaky-article")!
        try requestStore.enqueue(sourceURL: pageURL)

        let resolver = SharedEpisodeResolver { url, _, _ in
            throw StubLoaderError.unexpectedRequest(url)
        }
        let coordinator = SharedEpisodeImportCoordinator(resolver: resolver)

        for attempt in 1..<SharedEpisodeImportCoordinator.maxAttempts {
            await coordinator.consumePending(from: requestStore, store: store) { _ in }
            let pending = try requestStore.pendingRequests()
            XCTAssertEqual(pending.count, 1, "attempt \(attempt) should leave the request queued")
            XCTAssertEqual(pending.first?.attemptCount, attempt)
            guard case .failed = coordinator.phase else {
                return XCTFail("Expected failed phase after attempt \(attempt)")
            }
        }

        // One more failure exceeds maxAttempts: the request is finally dropped.
        await coordinator.consumePending(from: requestStore, store: store) { _ in }
        XCTAssertTrue(try requestStore.pendingRequests().isEmpty)
    }

    private func makeRequestStore() throws -> SharedEpisodeImportRequestStore {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "pod0-share-queue-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return SharedEpisodeImportRequestStore(directoryURL: root)
    }
}

private enum StubLoaderError: Error {
    case unexpectedRequest(URL)
}

private actor StubLoader {
    private let documents: [URL: SharedEpisodeHTTPDocument]

    init(documents: [URL: SharedEpisodeHTTPDocument]) {
        self.documents = documents
    }

    func document(for url: URL) throws -> SharedEpisodeHTTPDocument {
        guard let document = documents[url] else {
            throw StubLoaderError.unexpectedRequest(url)
        }
        return document
    }
}

private extension SharedEpisodeHTTPDocument {
    static func html(_ body: String, url: URL) -> Self {
        .init(data: Data(body.utf8), finalURL: url, mimeType: "text/html", statusCode: 200)
    }

    static func xml(_ body: String, url: URL) -> Self {
        .init(data: Data(body.utf8), finalURL: url, mimeType: "application/rss+xml", statusCode: 200)
    }
}
