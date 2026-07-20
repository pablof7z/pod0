import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class EpisodeChapterAuthorityDecodingTests: XCTestCase {
    func testAuthoritativeDecodeSkipsMalformedLegacyChapterAndAdPayloads() throws {
        let bytes = Data(
            """
            {
              "id":"11111111-1111-1111-1111-111111111111",
              "podcastID":"22222222-2222-2222-2222-222222222222",
              "guid":"authority",
              "title":"Authority",
              "pubDate":"2026-07-20T00:00:00Z",
              "enclosureURL":"https://example.com/episode.mp3",
              "chapters":[{"id":"not-a-uuid","startTime":"invalid"}],
              "adSegments":[{"start":"invalid","end":20}]
            }
            """.utf8
        )
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        decoder.userInfo[.loadLegacyChapterAdjuncts] = false

        let episode = try decoder.decode(Episode.self, from: bytes)

        XCTAssertNil(episode.chapters)
        XCTAssertNil(episode.adSegments)
    }

    func testMigrationDecodeStillReadsValidLegacyChapterAndExplicitEmptyAds() throws {
        let fixture = SharedTranscriptRecoveryTestSupport.makeFixture()
        defer { SharedTranscriptRecoveryTestSupport.dispose(fixture) }
        try SharedChapterRecoveryTestSupport.injectLegacyChapters(fixture)

        let episode = try XCTUnwrap(fixture.persistence.load().episodes.first)

        XCTAssertEqual(episode.chapters?.first?.title, "Recovered chapter")
        XCTAssertEqual(episode.adSegments, [])
    }
}
