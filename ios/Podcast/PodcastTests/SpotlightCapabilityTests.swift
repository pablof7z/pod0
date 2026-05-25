import CoreSpotlight
import XCTest
@testable import Podcast

// MARK: - SpotlightCapability tests
//
// These tests pin the identifier scheme, the deep-link decoder, and
// the item-builder shape. They deliberately do not assert against the
// real `CSSearchableIndex.default()` — that's a system service whose
// behaviour we don't own and whose side effects don't round-trip in
// a unit-test environment.

@MainActor
final class SpotlightCapabilityTests: XCTestCase {

    // MARK: - Identifier scheme

    func testPodcastIdentifierUsesExpectedPrefix() {
        XCTAssertEqual(
            SpotlightCapability.podcastIdentifier("abc-123"),
            "podcast:abc-123")
    }

    func testEpisodeIdentifierUsesExpectedPrefix() {
        XCTAssertEqual(
            SpotlightCapability.episodeIdentifier("ep-99"),
            "episode:ep-99")
    }

    func testDomainIdentifierMatchesContract() {
        // The task contract pins the single-domain id; assert it so a
        // rename surfaces here instead of as a silent index drift.
        XCTAssertEqual(SpotlightCapability.domainIdentifier, "io.f7z.podcast.library")
    }

    // MARK: - Deep-link decoding

    func testDeepLinkDecodesPodcastIdentifier() {
        let decoded = SpotlightCapability.deepLink(fromIdentifier: "podcast:abc-123")
        XCTAssertEqual(decoded, .podcast("abc-123"))
    }

    func testDeepLinkDecodesEpisodeIdentifier() {
        let decoded = SpotlightCapability.deepLink(fromIdentifier: "episode:ep-99")
        XCTAssertEqual(decoded, .episode("ep-99"))
    }

    func testDeepLinkRejectsUnknownPrefix() {
        XCTAssertNil(SpotlightCapability.deepLink(fromIdentifier: "note:foo"))
        XCTAssertNil(SpotlightCapability.deepLink(fromIdentifier: "abc-123"))
        XCTAssertNil(SpotlightCapability.deepLink(fromIdentifier: ""))
    }

    func testDeepLinkRejectsEmptyId() {
        XCTAssertNil(SpotlightCapability.deepLink(fromIdentifier: "podcast:"))
        XCTAssertNil(SpotlightCapability.deepLink(fromIdentifier: "episode:"))
    }

    func testDeepLinkFromActivityRoundTripsThroughUserInfo() {
        let activity = NSUserActivity(activityType: CSSearchableItemActionType)
        activity.userInfo = [
            CSSearchableItemActivityIdentifier: "episode:my-ep-id"
        ]
        XCTAssertEqual(
            SpotlightCapability.deepLink(fromActivity: activity),
            .episode("my-ep-id"))
    }

    func testDeepLinkFromActivityRejectsForeignActivityType() {
        let activity = NSUserActivity(activityType: "io.f7z.podcast.playing")
        activity.userInfo = [
            CSSearchableItemActivityIdentifier: "podcast:abc-123"
        ]
        XCTAssertNil(SpotlightCapability.deepLink(fromActivity: activity))
    }

    func testDeepLinkFromActivityRejectsMissingUserInfo() {
        let activity = NSUserActivity(activityType: CSSearchableItemActionType)
        XCTAssertNil(SpotlightCapability.deepLink(fromActivity: activity))
    }

    // MARK: - Item building

    func testBuildItemsEmitsOnePerPodcastAndPerEpisode() {
        let capability = SpotlightCapability()
        let library: [PodcastSummary] = [
            makePodcast(id: "p1", title: "Show One", episodeIds: ["e1", "e2"]),
            makePodcast(id: "p2", title: "Show Two", episodeIds: ["e3"]),
        ]
        let items = capability.buildItems(for: library)

        // 2 podcasts + 3 episodes = 5 searchable items.
        XCTAssertEqual(items.count, 5)

        let ids = Set(items.map(\.uniqueIdentifier))
        XCTAssertEqual(ids, [
            "podcast:p1", "podcast:p2",
            "episode:e1", "episode:e2", "episode:e3",
        ])
    }

    func testBuildItemsTagsAllItemsWithSharedDomain() {
        let capability = SpotlightCapability()
        let library = [makePodcast(id: "p1", title: "Show", episodeIds: ["e1"])]
        let items = capability.buildItems(for: library)
        for item in items {
            XCTAssertEqual(item.domainIdentifier, SpotlightCapability.domainIdentifier)
        }
    }

    func testBuildItemsPodcastAttributesCarryTitleAuthorAndArtwork() throws {
        let capability = SpotlightCapability()
        let podcast = PodcastSummary(
            id: "p1",
            title: "Cool Show",
            episodeCount: 12,
            unplayedCount: 3,
            artworkUrl: "https://example.com/art.png",
            feedUrl: nil,
            author: "Jane Host",
            episodes: [])
        let items = capability.buildItems(for: [podcast])
        let item = try XCTUnwrap(items.first)
        XCTAssertEqual(item.uniqueIdentifier, "podcast:p1")
        XCTAssertEqual(item.attributeSet.title, "Cool Show")
        XCTAssertEqual(item.attributeSet.artist, "Jane Host")
        XCTAssertEqual(item.attributeSet.thumbnailURL,
                       URL(string: "https://example.com/art.png"))
        // Episode count is folded into the description for context.
        let description = try XCTUnwrap(item.attributeSet.contentDescription)
        XCTAssertTrue(description.contains("12"))
        XCTAssertTrue(description.contains("Jane Host"))
    }

    func testBuildItemsEpisodeAttributesIncludeParentShow() throws {
        let capability = SpotlightCapability()
        let podcast = makePodcast(id: "p1", title: "Daily News", episodeIds: ["e1"])
        let items = capability.buildItems(for: [podcast])
        let episodeItem = try XCTUnwrap(items.first(where: { $0.uniqueIdentifier == "episode:e1" }))
        XCTAssertEqual(episodeItem.attributeSet.album, "Daily News")
        XCTAssertEqual(episodeItem.attributeSet.artist, "Daily News")
        let description = try XCTUnwrap(episodeItem.attributeSet.contentDescription)
        XCTAssertTrue(description.contains("Daily News"))
    }

    // MARK: - Smoke tests

    func testIndexLibraryHandlesEmptyArrayWithoutCrash() {
        let capability = SpotlightCapability()
        // First call with empty library should not throw; the
        // delete-then-rebuild handshake is allowed to skip the index
        // submission entirely.
        capability.indexLibrary([])
    }

    func testIndexLibraryIsIdempotentForRepeatedSnapshots() {
        // Two identical calls — the second should short-circuit on
        // the cached comparison. We can't directly observe the system
        // index call count, but this exercise pins that the equality
        // check doesn't blow up on `PodcastSummary` Equatable.
        let capability = SpotlightCapability()
        let library = [makePodcast(id: "p1", title: "S", episodeIds: ["e1"])]
        capability.indexLibrary(library)
        capability.indexLibrary(library)
    }

    func testDeindexClearsCachedLibraryRow() {
        let capability = SpotlightCapability()
        let library = [
            makePodcast(id: "p1", title: "S1", episodeIds: ["e1"]),
            makePodcast(id: "p2", title: "S2", episodeIds: ["e2"]),
        ]
        capability.indexLibrary(library)
        capability.deindex(podcastId: "p1")
        // Re-feeding the *original* library should now succeed (not
        // short-circuit) because `deindex` evicted p1 from the cache.
        // We re-call to confirm no crash; equality vs cache differs.
        capability.indexLibrary(library)
    }

    // MARK: - Helpers

    private func makePodcast(id: String, title: String, episodeIds: [String]) -> PodcastSummary {
        PodcastSummary(
            id: id,
            title: title,
            episodeCount: episodeIds.count,
            unplayedCount: 0,
            artworkUrl: nil,
            feedUrl: nil,
            author: nil,
            episodes: episodeIds.map { eid in
                EpisodeSummary(
                    id: eid,
                    title: "Episode \(eid)",
                    podcastId: id,
                    podcastTitle: title,
                    durationSecs: 600,
                    artworkUrl: nil,
                    publishedAt: 1_700_000_000,
                    downloadPath: nil)
            })
    }
}

// MARK: - SpotlightDeepLinkRouter tests

@MainActor
final class SpotlightDeepLinkRouterTests: XCTestCase {

    func testRouterStartsEmpty() {
        let router = SpotlightDeepLinkRouter()
        XCTAssertNil(router.pendingDeepLink)
    }

    func testHandleSpotlightActivityStashesDeepLink() {
        let router = SpotlightDeepLinkRouter()
        let activity = NSUserActivity(activityType: CSSearchableItemActionType)
        activity.userInfo = [
            CSSearchableItemActivityIdentifier: "podcast:abc"
        ]
        XCTAssertTrue(router.handle(activity))
        XCTAssertEqual(router.pendingDeepLink, .podcast("abc"))
    }

    func testHandleForeignActivityReturnsFalseAndLeavesSlotEmpty() {
        let router = SpotlightDeepLinkRouter()
        let activity = NSUserActivity(activityType: "io.f7z.podcast.playing")
        XCTAssertFalse(router.handle(activity))
        XCTAssertNil(router.pendingDeepLink)
    }

    func testConsumeClearsPendingSlot() {
        let router = SpotlightDeepLinkRouter()
        router.requestNavigation(to: .episode("e1"))
        XCTAssertEqual(router.pendingDeepLink, .episode("e1"))
        router.consume()
        XCTAssertNil(router.pendingDeepLink)
    }

    func testRequestNavigationReplacesUnconsumedPendingLink() {
        let router = SpotlightDeepLinkRouter()
        router.requestNavigation(to: .podcast("a"))
        router.requestNavigation(to: .episode("b"))
        XCTAssertEqual(router.pendingDeepLink, .episode("b"))
    }
}
