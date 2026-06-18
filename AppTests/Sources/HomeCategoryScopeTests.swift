import XCTest
@testable import Podcastr

/// Coverage for the magazine-mode category-scoping derivations: resume
/// rail filtering and the `HomeCategoryScope` pure-Swift helpers.
///
/// Dateline (`HomeDateline`) and threading-topic tests that depended on
/// types removed in the autosnip migration have been deleted; the
/// `HomeDateline` type was removed and threading projection is now Rust-owned
/// (exercised by `cargo test -p nmp-app-podcast threading`).
final class HomeCategoryScopeTests: XCTestCase {

    // MARK: - episodesInCategory

    func testEpisodesInCategoryNilFilterReturnsInputUntouched() {
        let subA = UUID(); let subB = UUID()
        let episodes = [
            makeEpisode(podcastID: subA),
            makeEpisode(podcastID: subB)
        ]

        let scoped = HomeCategoryScope.episodesInCategory(
            episodes,
            allowedSubscriptionIDs: nil
        )
        XCTAssertEqual(scoped.count, episodes.count)
        XCTAssertEqual(scoped.map(\.podcastID), [subA, subB])
    }

    func testEpisodesInCategoryFiltersToAllowedSubscriptions() {
        let subA = UUID(); let subB = UUID(); let subC = UUID()
        let epA = makeEpisode(podcastID: subA)
        let epB = makeEpisode(podcastID: subB)
        let epC = makeEpisode(podcastID: subC)

        let scoped = HomeCategoryScope.episodesInCategory(
            [epA, epB, epC],
            allowedSubscriptionIDs: [subA, subC]
        )
        XCTAssertEqual(Set(scoped.map(\.podcastID)), [subA, subC])
    }

    func testEpisodesInCategoryEmptyAllowedSetReturnsEmpty() {
        let subA = UUID()
        let episodes = [makeEpisode(podcastID: subA)]

        let scoped = HomeCategoryScope.episodesInCategory(
            episodes,
            allowedSubscriptionIDs: []
        )
        XCTAssertTrue(scoped.isEmpty)
    }

    // MARK: - topicsInCategory (drives dateline contradiction count)

    func testTopicsInCategoryNilFilterReturnsTopicsUntouched() {
        let topic = ThreadingTopic(
            slug: "x",
            displayName: "X",
            episodeMentionCount: 0,
            contradictionCount: 1
        )
        let scoped = HomeCategoryScope.topicsInCategory(
            topics: [topic],
            mentions: [],
            episodes: [],
            allowedSubscriptionIDs: nil
        )
        XCTAssertEqual(scoped.count, 1)
    }

    func testTopicsInCategoryDropsTopicWhoseMentionsAreOutsideTheCategory() {
        // Topic with one mention in an out-of-category episode. Active
        // category covers a different subscription. The topic should NOT
        // surface — fixes the brief-bug where switching to "Learning"
        // still counted contradictions in topics that lived elsewhere.
        let inCategorySub = UUID()
        let outOfCategorySub = UUID()
        let outOfCategoryEpisode = makeEpisode(podcastID: outOfCategorySub)
        let topic = ThreadingTopic(
            slug: "keto",
            displayName: "Keto",
            episodeMentionCount: 1,
            contradictionCount: 1
        )
        let mention = ThreadingMention(
            topicID: topic.id,
            episodeID: outOfCategoryEpisode.id,
            startMS: 0,
            endMS: 1000,
            snippet: "out-of-cat",
            confidence: 0.9,
            isContradictory: true
        )

        let scoped = HomeCategoryScope.topicsInCategory(
            topics: [topic],
            mentions: [mention],
            episodes: [outOfCategoryEpisode],
            allowedSubscriptionIDs: [inCategorySub]
        )
        XCTAssertTrue(scoped.isEmpty)
    }

    func testTopicsInCategoryKeepsTopicWithMentionInCategory() {
        let inCategorySub = UUID()
        let inCategoryEpisode = makeEpisode(podcastID: inCategorySub)
        let topic = ThreadingTopic(
            slug: "keto",
            displayName: "Keto",
            episodeMentionCount: 1,
            contradictionCount: 1
        )
        let mention = ThreadingMention(
            topicID: topic.id,
            episodeID: inCategoryEpisode.id,
            startMS: 0,
            endMS: 1000,
            snippet: "in-cat",
            confidence: 0.9,
            isContradictory: true
        )

        let scoped = HomeCategoryScope.topicsInCategory(
            topics: [topic],
            mentions: [mention],
            episodes: [inCategoryEpisode],
            allowedSubscriptionIDs: [inCategorySub]
        )
        XCTAssertEqual(scoped.count, 1)
    }

    // MARK: - Fixtures

    private func makeEpisode(
        podcastID: UUID,
        guid: String = UUID().uuidString,
        pubDate: Date = Date(),
        played: Bool = false
    ) -> Episode {
        var ep = Episode(
            podcastID: podcastID,
            guid: guid,
            title: "ep \(guid)",
            pubDate: pubDate,
            enclosureURL: URL(string: "https://example.com/\(guid).mp3")!
        )
        ep.played = played
        return ep
    }
}
