import XCTest
@testable import Podcastr

/// Pin the NIP-73 wire format for `CommentTarget`. The `i` and `k` tag
/// values are what makes our comments interoperable with Fountain and any
/// other NIP-22 reader, so we'd really rather not accidentally rename the
/// scheme prefix in a refactor.
final class EpisodeCommentTargetTests: XCTestCase {

    func testEpisodeIdentifierUsesPodcastingTwoPointZeroScheme() {
        let t = CommentTarget.episode(guid: "abc-123")
        XCTAssertEqual(t.nip73Identifier, "podcast:item:guid:abc-123")
        XCTAssertEqual(t.nip73Kind, "podcast:item:guid")
    }

    func testClipIdentifierIsPodcastrScopedAndLowercased() {
        // UUID string output is uppercase on Apple platforms; the NIP-73
        // identifier must be lowercased so two readers comparing strings
        // for the same clip don't miss each other on case.
        let uuid = UUID(uuidString: "DEADBEEF-DEAD-BEEF-DEAD-BEEFDEADBEEF")!
        let t = CommentTarget.clip(id: uuid)
        XCTAssertEqual(t.nip73Identifier, "podcastr:clip:deadbeef-dead-beef-dead-beefdeadbeef")
        XCTAssertEqual(t.nip73Kind, "podcastr:clip")
    }

    func testAuthorShortKeyTruncatesLongHex() {
        let c = EpisodeComment(
            id: "evt1",
            target: .episode(guid: "g"),
            authorPubkeyHex: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            content: "hi",
            createdAt: Date()
        )
        XCTAssertEqual(c.authorShortKey, "dead…beef")
    }

    func testAuthorShortKeyPassesThroughShortInput() {
        let c = EpisodeComment(
            id: "evt1",
            target: .episode(guid: "g"),
            authorPubkeyHex: "short",
            content: "hi",
            createdAt: Date()
        )
        XCTAssertEqual(c.authorShortKey, "short")
    }
}
