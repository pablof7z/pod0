import XCTest
@testable import Podcastr

/// Coverage for the magazine-mode category-scoping derivations: resume
/// rail filtering, dateline prefix + per-category counts, and the
/// `ThreadingInferenceService.topActiveTopics(subscriptionFilter:)`
/// scope. Each derivation is pure (no SwiftUI environment, no live
/// store outside the threading test) so the tests are fast and stable.
final class HomeCategoryScopeTests: XCTestCase {

    private let utc = TimeZone(identifier: "UTC")!
    private let locale = Locale(identifier: "en_US_POSIX")

    private var calendar: Calendar {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = utc
        cal.locale = locale
        return cal
    }

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

    // MARK: - Dateline prefix + scoped counts

    func testDatelinePrefixPrependsCategoryName() {
        let now = make(year: 2026, month: 5, day: 5)
        let components = HomeDateline.components(
            episodes: [],
            topics: [],
            now: now,
            calendar: calendar,
            locale: locale,
            categoryName: "Learning"
        )
        XCTAssertEqual(components.categoryPrefix, "LEARNING")
        XCTAssertEqual(components.rendered, "LEARNING · TUESDAY · MAY 5")
    }

    func testDatelineWithoutCategoryHasEmptyPrefix() {
        let now = make(year: 2026, month: 5, day: 5)
        let components = HomeDateline.components(
            episodes: [],
            topics: [],
            now: now,
            calendar: calendar,
            locale: locale
        )
        XCTAssertEqual(components.categoryPrefix, "")
        XCTAssertEqual(components.rendered, "TUESDAY · MAY 5")
    }

    func testDatelineNewCountScopedToAllowedSubscriptions() {
        let now = make(year: 2026, month: 5, day: 5)
        let inCategory = UUID()
        let outOfCategory = UUID()

        // Two unplayed episodes inside the category, one outside.
        let inOne = makeEpisode(
            podcastID: inCategory,
            pubDate: now.addingTimeInterval(-3_600),
            played: false
        )
        let inTwo = makeEpisode(
            podcastID: inCategory,
            pubDate: now.addingTimeInterval(-7_200),
            played: false
        )
        let out = makeEpisode(
            podcastID: outOfCategory,
            pubDate: now.addingTimeInterval(-3_600),
            played: false
        )

        let scoped = HomeDateline.components(
            episodes: [inOne, inTwo, out],
            topics: [],
            now: now,
            calendar: calendar,
            locale: locale,
            categoryName: "Learning",
            allowedSubscriptionIDs: [inCategory]
        )
        XCTAssertEqual(scoped.newCount, 2)
        XCTAssertEqual(scoped.rendered, "LEARNING · TUESDAY · MAY 5 · 2 NEW")

        // Sanity check: same input, no scope = 3 NEW.
        let unscoped = HomeDateline.components(
            episodes: [inOne, inTwo, out],
            topics: [],
            now: now,
            calendar: calendar,
            locale: locale
        )
        XCTAssertEqual(unscoped.newCount, 3)
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

    func testDatelineContradictionCountScopedToCategory() {
        let now = make(year: 2026, month: 5, day: 5)
        let inCategorySub = UUID()
        let outOfCategorySub = UUID()
        let inCategoryEpisode = makeEpisode(podcastID: inCategorySub)
        let outOfCategoryEpisode = makeEpisode(podcastID: outOfCategorySub)

        let inCategoryTopic = ThreadingTopic(
            slug: "in",
            displayName: "In",
            episodeMentionCount: 1,
            contradictionCount: 2
        )
        let outOfCategoryTopic = ThreadingTopic(
            slug: "out",
            displayName: "Out",
            episodeMentionCount: 1,
            contradictionCount: 5
        )
        let mentions = [
            ThreadingMention(
                topicID: inCategoryTopic.id,
                episodeID: inCategoryEpisode.id,
                startMS: 0, endMS: 1000,
                snippet: "in", confidence: 0.9, isContradictory: true
            ),
            ThreadingMention(
                topicID: outOfCategoryTopic.id,
                episodeID: outOfCategoryEpisode.id,
                startMS: 0, endMS: 1000,
                snippet: "out", confidence: 0.9, isContradictory: true
            )
        ]
        let scopedTopics = HomeCategoryScope.topicsInCategory(
            topics: [inCategoryTopic, outOfCategoryTopic],
            mentions: mentions,
            episodes: [inCategoryEpisode, outOfCategoryEpisode],
            allowedSubscriptionIDs: [inCategorySub]
        )

        let components = HomeDateline.components(
            episodes: [inCategoryEpisode, outOfCategoryEpisode],
            topics: scopedTopics,
            now: now,
            calendar: calendar,
            locale: locale,
            categoryName: "Learning",
            allowedSubscriptionIDs: [inCategorySub]
        )
        // Only the in-category topic with `contradictionCount > 0` is
        // visible, so the dateline reads "1 CONTRADICTION", not "2".
        XCTAssertEqual(components.contradictionCount, 1)
        XCTAssertEqual(
            components.rendered,
            "LEARNING · TUESDAY · MAY 5 · 1 CONTRADICTION"
        )
    }

    // MARK: - topActiveTopics with subscription filter

    @MainActor
    func testTopActiveTopicsFiltersBySubscription() async throws {
        // Build an isolated store so the threading service has somewhere
        // to read mentions from. Two subscriptions: shows in category A
        // and shows outside it. Three unplayed episodes in subA mention
        // the topic; two more from subB do too. With subscriptionFilter
        // = [subA] only the three subA mentions should count, which
        // satisfies the threshold-of-three. Filtering to [subB] should
        // drop the topic entirely.
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let store = made.store

        let subA = Podcast(
            feedURL: URL(string: "https://a.example.com/feed.xml")!,
            title: "Show A"
        )
        let subB = Podcast(
            feedURL: URL(string: "https://b.example.com/feed.xml")!,
            title: "Show B"
        )
        store.upsertPodcast(subA)
        store.upsertPodcast(subB)
        store.addSubscription(podcastID: subA.id)
        store.addSubscription(podcastID: subB.id)

        let aEpisodes = (0..<3).map { i -> Episode in
            makeEpisode(podcastID: subA.id, guid: "a-\(i)")
        }
        let bEpisodes = (0..<2).map { i -> Episode in
            makeEpisode(podcastID: subB.id, guid: "b-\(i)")
        }
        store.upsertEpisodes(aEpisodes, forPodcast: subA.id)
        store.upsertEpisodes(bEpisodes, forPodcast: subB.id)

        let topic = ThreadingTopic(
            slug: "category-scope-topic",
            displayName: "Category Scope Topic",
            episodeMentionCount: aEpisodes.count + bEpisodes.count,
            contradictionCount: 0,
            lastMentionedAt: Date()
        )
        let stored = store.upsertThreadingTopic(topic)

        var mentions: [ThreadingMention] = []
        for ep in aEpisodes + bEpisodes {
            mentions.append(ThreadingMention(
                topicID: stored.id,
                episodeID: ep.id,
                startMS: 1_000,
                endMS: 2_000,
                snippet: "mention",
                confidence: 0.9,
                isContradictory: false
            ))
        }
        store.replaceThreadingMentions(forTopic: stored.id, with: mentions)

        let service = ThreadingInferenceService()
        service.attach(store: store)

        // No filter — global behaviour. 5 unplayed episodes, threshold met.
        let global = service.topActiveTopics(limit: 1, subscriptionFilter: nil)
        XCTAssertEqual(global.count, 1)
        XCTAssertEqual(global.first?.unplayedEpisodeCount, 5)

        // Scoped to subA — 3 mentions, still meets threshold of 3.
        let scopedA = service.topActiveTopics(limit: 1, subscriptionFilter: [subA.id])
        XCTAssertEqual(scopedA.count, 1)
        XCTAssertEqual(scopedA.first?.unplayedEpisodeCount, 3)

        // Scoped to subB — only 2 mentions, drops below threshold.
        let scopedB = service.topActiveTopics(limit: 1, subscriptionFilter: [subB.id])
        XCTAssertTrue(scopedB.isEmpty, "topic with 2 unplayed mentions shouldn't qualify")
    }

    // MARK: - Category framing

    func testCategoryFramingDetectsLearningArchetype() {
        let category = PodcastCategory(
            name: "Learning Deep Dives",
            slug: "learning",
            description: "Long-form educational shows."
        )
        let framing = AgentPicksPrompt.CategoryFraming.make(from: category)
        XCTAssertEqual(framing?.headerLabel, "LEARNING DEEP DIVES")
        XCTAssertTrue(framing?.guidance.contains("LEARNING mode") == true)
    }

    func testCategoryFramingDetectsNewsArchetype() {
        let category = PodcastCategory(
            name: "Daily News",
            slug: "news",
            description: ""
        )
        let framing = AgentPicksPrompt.CategoryFraming.make(from: category)
        XCTAssertTrue(framing?.guidance.contains("NEWS mode") == true)
    }

    func testCategoryFramingFallsBackToDescriptionForCustomNames() {
        let category = PodcastCategory(
            name: "Cosy Nightcaps",
            slug: "cosy-nightcaps",
            description: "Wind-down audio with gentle hosts."
        )
        let framing = AgentPicksPrompt.CategoryFraming.make(from: category)
        XCTAssertEqual(framing?.headerLabel, "COSY NIGHTCAPS")
        XCTAssertTrue(framing?.guidance.contains("Wind-down audio") == true,
                      "custom-named category must surface description verbatim")
    }

    func testCategoryFramingNilForEmptyCategory() {
        let category = PodcastCategory(
            name: "",
            slug: "",
            description: ""
        )
        XCTAssertNil(AgentPicksPrompt.CategoryFraming.make(from: category))
    }

    func testSystemInstructionWithoutFramingMatchesBase() {
        XCTAssertEqual(
            AgentPicksPrompt.systemInstruction(for: nil),
            AgentPicksPrompt.baseSystemInstruction
        )
    }

    func testSystemInstructionAppendsEditorialFraming() {
        let category = PodcastCategory(
            name: "Learning",
            slug: "learning",
            description: "How to learn."
        )
        let framing = AgentPicksPrompt.CategoryFraming.make(from: category)
        let instruction = AgentPicksPrompt.systemInstruction(for: framing)
        XCTAssertTrue(instruction.contains("EDITORIAL FRAMING"))
        XCTAssertTrue(instruction.contains("LEARNING"))
        XCTAssertTrue(instruction.hasPrefix(AgentPicksPrompt.baseSystemInstruction))
    }

    // MARK: - Fixtures

    private func make(year: Int, month: Int, day: Int) -> Date {
        calendar.date(from: DateComponents(
            timeZone: utc,
            year: year, month: month, day: day, hour: 12, minute: 0
        ))!
    }

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
