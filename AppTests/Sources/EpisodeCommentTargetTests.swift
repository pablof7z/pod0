import XCTest
@testable import Podcastr

final class EpisodeCommentTargetTests: XCTestCase {
    func testEpisodeTargetRoundTripsWithoutOwningWireEncoding() throws {
        let target = CommentTarget.episode(guid: "abc-123")
        let encoded = try JSONEncoder().encode(target)

        XCTAssertEqual(try JSONDecoder().decode(CommentTarget.self, from: encoded), target)
        XCTAssertFalse(String(decoding: encoded, as: UTF8.self).contains("podcast:item:guid"))
    }

    func testAuthorShortKeyTruncatesLongHex() {
        let comment = EpisodeComment(
            id: "evt1",
            target: .episode(guid: "g"),
            authorPubkeyHex: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            content: "hi",
            createdAt: Date()
        )
        XCTAssertEqual(comment.authorShortKey, "dead…beef")
    }

    func testAuthorShortKeyPassesThroughShortInput() {
        let comment = EpisodeComment(
            id: "evt1",
            target: .episode(guid: "g"),
            authorPubkeyHex: "short",
            content: "hi",
            createdAt: Date()
        )
        XCTAssertEqual(comment.authorShortKey, "short")
    }
}
