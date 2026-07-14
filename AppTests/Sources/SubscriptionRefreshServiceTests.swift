import XCTest
@testable import Podcastr

@MainActor
final class SubscriptionRefreshServiceTests: XCTestCase {
    private var storeFileURL: URL!
    private var store: AppStateStore!

    override func setUp() async throws {
        try await super.setUp()
        let made = await AppStateTestSupport.makeIsolatedStore()
        storeFileURL = made.fileURL
        store = made.store
        FeedRefreshStubProtocol.reset()
    }

    override func tearDown() async throws {
        if let storeFileURL {
            AppStateTestSupport.disposeIsolatedStore(at: storeFileURL)
        }
        store = nil
        storeFileURL = nil
        FeedRefreshStubProtocol.reset()
        try await super.tearDown()
    }

    func testSubscriptionServiceRefreshUsesSharedRefreshSemantics() async throws {
        var subscription = Podcast(
            feedURL: URL(string: "https://feeds.example.com/show.xml")!,
            title: "Old Title"
        )
        store.upsertPodcast(subscription)
        XCTAssertTrue(store.addSubscription(podcastID: subscription.id))

        FeedRefreshStubProtocol.responseHeaders = ["ETag": "\"v2\""]
        FeedRefreshStubProtocol.responseBody = Self.feedXML(
            title: "Fresh Title",
            guid: "episode-1"
        ).data(using: .utf8)!

        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [FeedRefreshStubProtocol.self] + (config.protocolClasses ?? [])
        let session = URLSession(configuration: config)
        let service = SubscriptionService(
            store: store,
            client: FeedClient(session: session)
        )

        await service.refresh(subscription)

        subscription = try XCTUnwrap(store.podcast(id: subscription.id))
        XCTAssertEqual(subscription.title, "Fresh Title")
        XCTAssertEqual(subscription.etag, "\"v2\"")
        XCTAssertEqual(store.episodes(forPodcast: subscription.id).map(\.guid), ["episode-1"])
    }

    func testRefreshServiceNotModifiedBumpsLastRefreshedWithoutEpisodes() async throws {
        let originalDate = Date(timeIntervalSince1970: 1_000)
        let subscription = Podcast(
            feedURL: URL(string: "https://feeds.example.com/show.xml")!,
            title: "Show",
            lastRefreshedAt: originalDate
        )
        store.upsertPodcast(subscription)
        XCTAssertTrue(store.addSubscription(podcastID: subscription.id))

        FeedRefreshStubProtocol.responseStatus = 304
        FeedRefreshStubProtocol.responseBody = Data()

        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [FeedRefreshStubProtocol.self] + (config.protocolClasses ?? [])
        let session = URLSession(configuration: config)
        let refresh = SubscriptionRefreshService(client: FeedClient(session: session))

        try await refresh.refresh(subscription.id, store: store)

        let updated = try XCTUnwrap(store.podcast(id: subscription.id))
        XCTAssertGreaterThan(updated.lastRefreshedAt ?? originalDate, originalDate)
        XCTAssertTrue(store.episodes(forPodcast: subscription.id).isEmpty)
    }

    private static func feedXML(title: String, guid: String) -> String {
        #"""
        <?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>\#(title)</title>
            <item>
              <title>Episode</title>
              <pubDate>Mon, 04 May 2026 09:00:00 GMT</pubDate>
              <guid>\#(guid)</guid>
              <enclosure url="https://cdn.example.com/episode.mp3" type="audio/mpeg"/>
            </item>
          </channel>
        </rss>
        """#
    }
}

private final class FeedRefreshStubProtocol: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var responseStatus: Int = 200
    nonisolated(unsafe) static var responseHeaders: [String: String] = [:]
    nonisolated(unsafe) static var responseBody: Data = Data()

    static func reset() {
        responseStatus = 200
        responseHeaders = [:]
        responseBody = Data()
    }

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        let response = HTTPURLResponse(
            url: request.url ?? URL(string: "https://stub.test/")!,
            statusCode: Self.responseStatus,
            httpVersion: "HTTP/1.1",
            headerFields: Self.responseHeaders
        )!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Self.responseBody)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}
