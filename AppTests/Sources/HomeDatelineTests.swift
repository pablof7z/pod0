import XCTest
@testable import Podcastr

/// Coverage for the dateline composition rule. Pinned to a fixed clock
/// + en_US calendar so the test is timezone- and locale-stable.
final class HomeDatelineTests: XCTestCase {

    private let utc = TimeZone(identifier: "UTC")!
    private let locale = Locale(identifier: "en_US_POSIX")

    private var calendar: Calendar {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = utc
        cal.locale = locale
        return cal
    }

    private func make(year: Int, month: Int, day: Int) -> Date {
        calendar.date(from: DateComponents(
            timeZone: utc,
            year: year, month: month, day: day, hour: 12, minute: 0
        ))!
    }

    func testRenderedSkipsZeroCounts() {
        let now = make(year: 2026, month: 5, day: 5) // Tuesday May 5
        let components = HomeDateline.components(
            episodes: [],
            topics: [],
            now: now,
            calendar: calendar,
            locale: locale
        )
        XCTAssertEqual(components.weekday, "TUESDAY")
        XCTAssertEqual(components.monthDay, "MAY 5")
        XCTAssertEqual(components.newCount, 0)
        XCTAssertEqual(components.contradictionCount, 0)
        // No `4 NEW` or `1 CONTRADICTION` segments when both are zero.
        XCTAssertEqual(components.rendered, "TUESDAY · MAY 5")
    }

    func testCountsUnplayedEpisodesInTrailing24Hours() {
        let now = make(year: 2026, month: 5, day: 5)
        let twoHoursAgo = now.addingTimeInterval(-2 * 3_600)
        let twentyFiveHoursAgo = now.addingTimeInterval(-25 * 3_600)

        let recentUnplayed = makeEpisode(pubDate: twoHoursAgo, played: false)
        let recentPlayed = makeEpisode(pubDate: twoHoursAgo, played: true)
        let stale = makeEpisode(pubDate: twentyFiveHoursAgo, played: false)
        let future = makeEpisode(pubDate: now.addingTimeInterval(3_600), played: false)

        let components = HomeDateline.components(
            episodes: [recentUnplayed, recentPlayed, stale, future],
            topics: [],
            now: now,
            calendar: calendar,
            locale: locale
        )
        XCTAssertEqual(components.newCount, 1)
        XCTAssertEqual(components.rendered, "TUESDAY · MAY 5 · 1 NEW")
    }

    func testCountsTopicsWithContradictions() {
        let now = make(year: 2026, month: 5, day: 5)
        let topicWith = makeTopic(slug: "keto", contradictions: 2)
        let topicWith2 = makeTopic(slug: "carnivore", contradictions: 1)
        let topicWithout = makeTopic(slug: "ai-safety", contradictions: 0)

        let components = HomeDateline.components(
            episodes: [],
            topics: [topicWith, topicWith2, topicWithout],
            now: now,
            calendar: calendar,
            locale: locale
        )
        XCTAssertEqual(components.contradictionCount, 2)
        XCTAssertEqual(components.rendered, "TUESDAY · MAY 5 · 2 CONTRADICTIONS")
    }

    func testFullDatelineMatchesBriefExample() {
        // Reproduces the literal example in the brief — Tuesday May 5,
        // 4 unplayed in 24h, 1 topic with contradictions.
        let now = make(year: 2026, month: 5, day: 5)
        let recents = (0..<4).map { i -> Episode in
            makeEpisode(pubDate: now.addingTimeInterval(Double(-i) * 3_600), played: false)
        }
        let topic = makeTopic(slug: "x", contradictions: 1)

        let components = HomeDateline.components(
            episodes: recents,
            topics: [topic],
            now: now,
            calendar: calendar,
            locale: locale
        )
        XCTAssertEqual(components.rendered, "TUESDAY · MAY 5 · 4 NEW · 1 CONTRADICTION")
    }

    // MARK: - Fixtures

    private func makeEpisode(pubDate: Date, played: Bool) -> Episode {
        var ep = Episode(
            podcastID: UUID(),
            guid: UUID().uuidString,
            title: "ep",
            pubDate: pubDate,
            enclosureURL: URL(string: "https://example.com/x.mp3")!
        )
        ep.played = played
        return ep
    }

    private func makeTopic(slug: String, contradictions: Int) -> ThreadingTopic {
        ThreadingTopic(
            slug: slug,
            displayName: slug,
            episodeMentionCount: 3,
            contradictionCount: contradictions
        )
    }
}
