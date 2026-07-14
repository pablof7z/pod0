import XCTest
@testable import Podcastr

/// Coverage for `DeepLinkHandler.resolve` and `friendInviteURL`. The
/// parser sits behind every quick-action, push-notification tap, and
/// share-sheet target, so a regression in URL parsing means
/// "tapping Open Agent does nothing" or "shared friend invite goes
/// to a blank Add Friend sheet."
@MainActor
final class DeepLinkHandlerTests: XCTestCase {

    // MARK: - Simple hosts

    func testResolvesSettings() {
        let url = URL(string: "podcastr://settings")!
        guard case .settings = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .settings"); return
        }
    }

    func testResolvesFeedback() {
        let url = URL(string: "podcastr://feedback")!
        guard case .feedback = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .feedback"); return
        }
    }

    func testResolvesAgent() {
        let url = URL(string: "podcastr://agent")!
        guard case .agent = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .agent"); return
        }
    }

    // MARK: - Friend

    func testResolvesFriendWithNpubAndName() {
        let url = URL(string: "podcastr://friend/add?npub=npub1abc&name=Alice")!
        guard case .addFriend(let npub, let name) = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .addFriend"); return
        }
        XCTAssertEqual(npub, "npub1abc")
        XCTAssertEqual(name, "Alice")
    }

    func testResolvesFriendWithNpubOnly() {
        let url = URL(string: "podcastr://friend/add?npub=npub1xyz")!
        guard case .addFriend(let npub, let name) = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .addFriend"); return
        }
        XCTAssertEqual(npub, "npub1xyz")
        XCTAssertNil(name)
    }

    func testFriendMissingNpubReturnsNil() {
        let url = URL(string: "podcastr://friend/add?name=Alice")!
        XCTAssertNil(DeepLinkHandler.resolve(url))
    }

    func testFriendEmptyNpubReturnsNil() {
        let url = URL(string: "podcastr://friend/add?npub=&name=Alice")!
        XCTAssertNil(DeepLinkHandler.resolve(url))
    }

    func testFriendWrongPathReturnsNil() {
        let url = URL(string: "podcastr://friend/something-else?npub=npub1abc")!
        XCTAssertNil(DeepLinkHandler.resolve(url))
    }

    // MARK: - Episode

    func testResolvesEpisodeWithValidUUID() {
        let id = UUID()
        let url = URL(string: "podcastr://episode/\(id.uuidString)")!
        guard case .episode(let parsed) = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .episode"); return
        }
        XCTAssertEqual(parsed, id)
    }

    func testEpisodeWithInvalidUUIDReturnsNil() {
        let url = URL(string: "podcastr://episode/not-a-uuid")!
        XCTAssertNil(DeepLinkHandler.resolve(url))
    }

    func testEpisodeWithoutPathComponentReturnsNil() {
        let url = URL(string: "podcastr://episode")!
        XCTAssertNil(DeepLinkHandler.resolve(url))
    }

    // MARK: - Subscription

    func testResolvesSubscriptionWithValidUUID() {
        let id = UUID()
        let url = URL(string: "podcastr://subscription/\(id.uuidString)")!
        guard case .subscription(let parsed) = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .subscription"); return
        }
        XCTAssertEqual(parsed, id)
    }

    func testSubscriptionWithInvalidUUIDReturnsNil() {
        let url = URL(string: "podcastr://subscription/not-a-uuid")!
        XCTAssertNil(DeepLinkHandler.resolve(url))
    }

    // MARK: - Failure surface

    func testRejectsForeignScheme() {
        let url = URL(string: "https://settings")!
        XCTAssertNil(DeepLinkHandler.resolve(url))
    }

    func testRejectsUnknownHost() {
        let url = URL(string: "podcastr://nope")!
        XCTAssertNil(DeepLinkHandler.resolve(url))
    }

    // MARK: - friendInviteURL builder

    func testFriendInviteURLIncludesNameWhenProvided() throws {
        let url = try XCTUnwrap(DeepLinkHandler.friendInviteURL(npub: "npub1abc", name: "Alice"))

        XCTAssertEqual(url.scheme, "podcastr")
        XCTAssertEqual(url.host, "friend")
        XCTAssertEqual(url.path, "/add")
        let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        XCTAssertEqual(items.first(where: { $0.name == "npub" })?.value, "npub1abc")
        XCTAssertEqual(items.first(where: { $0.name == "name" })?.value, "Alice")
    }

    func testFriendInviteURLOmitsEmptyName() throws {
        let url = try XCTUnwrap(DeepLinkHandler.friendInviteURL(npub: "npub1abc", name: ""))
        let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        XCTAssertNil(items.first(where: { $0.name == "name" }),
                     "Empty name should not surface as ?name= in the invite URL")
    }

    func testFriendInviteURLOmitsNilName() throws {
        let url = try XCTUnwrap(DeepLinkHandler.friendInviteURL(npub: "npub1abc", name: nil))
        let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        XCTAssertNil(items.first(where: { $0.name == "name" }))
    }

    // MARK: - Round-trip

    func testInviteURLRoundTripsThroughResolver() throws {
        let url = try XCTUnwrap(DeepLinkHandler.friendInviteURL(npub: "npub1abc", name: "Alice"))
        guard case .addFriend(let npub, let name) = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected the builder's URL to parse back via resolve()"); return
        }
        XCTAssertEqual(npub, "npub1abc")
        XCTAssertEqual(name, "Alice")
    }

    // MARK: - Episode-by-GUID short-link
    //
    // PlayerMoreMenu's "Copy episode link" produces `podcastr://e/<guid>` —
    // historically dropped on the floor by `resolve()`. The transcript
    // share path additionally appends `?t=<seconds>` so deep-linking
    // lands on the referenced timestamp. Both forms must resolve.

    func testResolvesEpisodeByGUID() {
        let url = URL(string: "podcastr://e/podcast-1234")!
        guard case .episodeByGUID(let guid, let t) = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .episodeByGUID"); return
        }
        XCTAssertEqual(guid, "podcast-1234")
        XCTAssertNil(t)
    }

    func testResolvesEpisodeByGUIDWithStartTime() {
        let url = URL(string: "podcastr://e/podcast-1234?t=420")!
        guard case .episodeByGUID(let guid, let t) = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .episodeByGUID"); return
        }
        XCTAssertEqual(guid, "podcast-1234")
        XCTAssertEqual(t, 420)
    }

    func testResolvesEpisodeByGUIDClampsNegativeT() {
        // Defensive: if a corrupted or hand-edited link arrives with a
        // negative timestamp, clamp to zero rather than seeking to a
        // negative offset that the engine would have to defend against.
        let url = URL(string: "podcastr://e/abc?t=-30")!
        guard case .episodeByGUID(_, let t) = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .episodeByGUID"); return
        }
        XCTAssertEqual(t, 0)
    }

    func testResolvesEpisodeByGUIDIgnoresMalformedT() {
        // Non-numeric `t=` shouldn't poison the resolve — we just drop
        // the timestamp and surface the link as a no-time episode jump.
        let url = URL(string: "podcastr://e/abc?t=banana")!
        guard case .episodeByGUID(let guid, let t) = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .episodeByGUID"); return
        }
        XCTAssertEqual(guid, "abc")
        XCTAssertNil(t)
    }

    func testEpisodeByGUIDDecodesPercentEncoding() {
        // Some publisher GUIDs include reserved characters (commonly `:`
        // and `=`). The share path percent-encodes them; the resolver
        // must round-trip cleanly.
        let url = URL(string: "podcastr://e/tag%3Aacme.com%2C2024%3Aep-7")!
        guard case .episodeByGUID(let guid, _) = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .episodeByGUID"); return
        }
        XCTAssertEqual(guid, "tag:acme.com,2024:ep-7")
    }

    func testEpisodeGUIDBuilderEncodesReservedCharacters() throws {
        let url = try XCTUnwrap(DeepLinkHandler.episodeGUIDURL(
            guid: "tag:acme.com,2024:/episode?x=1",
            startTime: 42.8
        ))
        XCTAssertEqual(url.absoluteString, "podcastr://e/tag%3Aacme.com%2C2024%3A%2Fepisode%3Fx%3D1?t=42")
        guard case .episodeByGUID(let guid, let t) = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .episodeByGUID"); return
        }
        XCTAssertEqual(guid, "tag:acme.com,2024:/episode?x=1")
        XCTAssertEqual(t, 42)
    }

    func testRejectsEmptyEpisodeByGUID() {
        // `podcastr://e` with no path is not a valid link.
        XCTAssertNil(DeepLinkHandler.resolve(URL(string: "podcastr://e")!))
    }

    // MARK: - Clip share

    func testResolvesClipByUUID() {
        let id = UUID()
        let url = URL(string: "podcastr://clip/\(id.uuidString)")!
        guard case .clip(let resolved) = DeepLinkHandler.resolve(url) else {
            XCTFail("Expected .clip"); return
        }
        XCTAssertEqual(resolved, id)
    }

    func testRejectsClipWithMalformedUUID() {
        // Anything that isn't a valid UUID should bail rather than
        // surfacing a nonsense link the consumer can't resolve anyway.
        XCTAssertNil(DeepLinkHandler.resolve(URL(string: "podcastr://clip/not-a-uuid")!))
    }

    func testRejectsClipWithEmptyPath() {
        XCTAssertNil(DeepLinkHandler.resolve(URL(string: "podcastr://clip")!))
    }
}
