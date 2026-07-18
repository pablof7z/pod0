import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class CoreFeedHostTests: XCTestCase {
    override func setUp() {
        super.setUp()
        CoreFeedStubProtocol.reset()
    }

    override func tearDown() {
        CoreFeedStubProtocol.reset()
        super.tearDown()
    }

    func testFetchReturnsRawBoundedBytesAndCacheEvidenceWithoutParsing() async throws {
        let body = Data("not-even-xml".utf8)
        CoreFeedStubProtocol.responseBody = body
        CoreFeedStubProtocol.responseHeaders = [
            "ETag": "\"v2\"",
            "Last-Modified": "Sat, 18 Jul 2026 20:00:00 GMT",
        ]
        let session = makeSession()

        let result = await CoreFeedHost(session: session).fetch(
            feedURL: "https://feeds.example.test/show.xml",
            entityTag: "\"v1\"",
            lastModified: "Fri, 17 Jul 2026 20:00:00 GMT",
            maximumResponseBytes: 1_024,
            deadline: Date().addingTimeInterval(10)
        )

        guard case .feedBytesFetched(
            let bytes,
            let entityTag,
            let lastModified,
            let responseURL,
            let status
        ) = result else {
            return XCTFail("Expected raw feed bytes, got \(result)")
        }
        XCTAssertEqual(bytes, body)
        XCTAssertEqual(entityTag, "\"v2\"")
        XCTAssertEqual(lastModified, "Sat, 18 Jul 2026 20:00:00 GMT")
        XCTAssertEqual(responseURL, "https://feeds.example.test/show.xml")
        XCTAssertEqual(status, 200)
        XCTAssertEqual(CoreFeedStubProtocol.lastRequest?.value(forHTTPHeaderField: "If-None-Match"), "\"v1\"")
        XCTAssertEqual(
            CoreFeedStubProtocol.lastRequest?.value(forHTTPHeaderField: "If-Modified-Since"),
            "Fri, 17 Jul 2026 20:00:00 GMT"
        )
        session.invalidateAndCancel()
    }

    func testNotModifiedPreservesValidators() async {
        CoreFeedStubProtocol.responseStatus = 304
        let session = makeSession()

        let result = await CoreFeedHost(session: session).fetch(
            feedURL: "https://feeds.example.test/show.xml",
            entityTag: "\"v1\"",
            lastModified: "yesterday",
            maximumResponseBytes: 1_024,
            deadline: nil
        )

        XCTAssertEqual(
            result,
            .feedNotModified(
                entityTag: "\"v1\"",
                lastModified: "yesterday",
                responseUrl: "https://feeds.example.test/show.xml"
            )
        )
        session.invalidateAndCancel()
    }

    func testOversizedAndOfflineResponsesReturnStableFailureCodes() async {
        CoreFeedStubProtocol.responseBody = Data(repeating: 7, count: 5)
        var session = makeSession()
        let oversized = await CoreFeedHost(session: session).fetch(
            feedURL: "https://feeds.example.test/show.xml",
            entityTag: nil,
            lastModified: nil,
            maximumResponseBytes: 4,
            deadline: nil
        )
        guard case .failed(code: .responseTooLarge, safeDetail: _) = oversized else {
            return XCTFail("Expected bounded response failure")
        }
        session.invalidateAndCancel()

        CoreFeedStubProtocol.error = URLError(.notConnectedToInternet)
        session = makeSession()
        let offline = await CoreFeedHost(session: session).fetch(
            feedURL: "https://feeds.example.test/show.xml",
            entityTag: nil,
            lastModified: nil,
            maximumResponseBytes: 4,
            deadline: nil
        )
        guard case .failed(code: .offline, safeDetail: _) = offline else {
            return XCTFail("Expected stable offline failure")
        }
        session.invalidateAndCancel()
    }

    private func makeSession() -> URLSession {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [CoreFeedStubProtocol.self]
        return URLSession(configuration: configuration)
    }
}

private final class CoreFeedStubProtocol: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var responseStatus = 200
    nonisolated(unsafe) static var responseHeaders: [String: String] = [:]
    nonisolated(unsafe) static var responseBody = Data()
    nonisolated(unsafe) static var error: Error?
    nonisolated(unsafe) static var lastRequest: URLRequest?

    static func reset() {
        responseStatus = 200
        responseHeaders = [:]
        responseBody = Data()
        error = nil
        lastRequest = nil
    }

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        Self.lastRequest = request
        if let error = Self.error {
            client?.urlProtocol(self, didFailWithError: error)
            return
        }
        let response = HTTPURLResponse(
            url: request.url!,
            statusCode: Self.responseStatus,
            httpVersion: "HTTP/1.1",
            headerFields: Self.responseHeaders
        )!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Self.responseBody)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() { }
}
