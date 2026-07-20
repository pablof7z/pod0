import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class CorePublisherChapterHostTests: XCTestCase {
    override func setUp() {
        super.setUp()
        ChapterProviderStubProtocol.reset()
    }

    override func tearDown() {
        ChapterProviderStubProtocol.reset()
        super.tearDown()
    }

    func testReportsRawHTTPFactsWithoutClassifyingStatusOrParsingBytes() async {
        let raw = Data("not chapter JSON".utf8)
        ChapterProviderStubProtocol.responseStatus = 404
        ChapterProviderStubProtocol.responseBody = raw
        ChapterProviderStubProtocol.responseHeaders = [
            "Content-Type": "application/json+chapters",
            "ETag": "\"publisher-v2\"",
            "Last-Modified": "Mon, 20 Jul 2026 00:00:00 GMT",
        ]
        let session = makeSession()

        let result = await CorePublisherChapterHost(session: session).fetch(
            episodeID: EpisodeId(high: 1, low: 2),
            sourceURL: "https://example.test/chapters.json",
            maximumResponseBytes: 1_024,
            deadline: Date().addingTimeInterval(30)
        )

        guard case let .publisherChaptersFetched(
            episodeId, bytes, contentType, responseUrl, entityTag, lastModified, httpStatus
        ) = result else { return XCTFail("Expected raw publisher response: \(result)") }
        XCTAssertEqual(episodeId, EpisodeId(high: 1, low: 2))
        XCTAssertEqual(bytes, raw)
        XCTAssertEqual(contentType, "application/json+chapters")
        XCTAssertEqual(responseUrl, "https://example.test/chapters.json")
        XCTAssertEqual(entityTag, "\"publisher-v2\"")
        XCTAssertEqual(lastModified, "Mon, 20 Jul 2026 00:00:00 GMT")
        XCTAssertEqual(httpStatus, 404)
        XCTAssertEqual(ChapterProviderStubProtocol.lastRequest?.httpMethod, "GET")
        session.invalidateAndCancel()
    }

    func testEnforcesOnlyTypedRequestAndByteBounds() async {
        let invalid = await CorePublisherChapterHost().fetch(
            episodeID: EpisodeId(high: 1, low: 2),
            sourceURL: "file:///tmp/chapters.json",
            maximumResponseBytes: 1_024,
            deadline: nil
        )
        guard case .failed(code: .invalidResponse, safeDetail: _) = invalid else {
            return XCTFail("Expected invalid request fact")
        }

        ChapterProviderStubProtocol.responseBody = Data(repeating: 7, count: 5)
        let session = makeSession()
        let oversized = await CorePublisherChapterHost(session: session).fetch(
            episodeID: EpisodeId(high: 1, low: 2),
            sourceURL: "https://example.test/chapters.json",
            maximumResponseBytes: 4,
            deadline: nil
        )
        guard case .failed(code: .responseTooLarge, safeDetail: _) = oversized else {
            return XCTFail("Expected response bound fact")
        }
        session.invalidateAndCancel()
    }

    private func makeSession() -> URLSession {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ChapterProviderStubProtocol.self]
        return URLSession(configuration: configuration)
    }
}
