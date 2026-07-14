import XCTest
@testable import Podcastr

/// Coverage for `AgentPicksFallback.derive` — the heuristic used when the
/// LLM agent path is unavailable (no API key) or fails. The picks must
/// degrade gracefully without crashing or returning duplicates.
final class AgentPicksFallbackTests: XCTestCase {

    func testEmptyInputsProduceNoPicks() {
        let inputs = AgentPicksInputs(
            unplayed: [],
            inProgress: [],
            subscriptionTitles: [:],
            memorySnippets: [],
            topicNames: []
        )
        XCTAssertTrue(AgentPicksFallback.derive(inputs: inputs).isEmpty)
    }

    func testFirstPickIsHero() {
        let subID = UUID()
        let ep = makeEpisode(subID: subID, pubDate: Date())
        let inputs = AgentPicksInputs(
            unplayed: [ep],
            inProgress: [],
            subscriptionTitles: [subID: "Show"],
            memorySnippets: [],
            topicNames: []
        )
        let picks = AgentPicksFallback.derive(inputs: inputs)
        XCTAssertEqual(picks.count, 1)
        XCTAssertTrue(picks[0].isHero)
        XCTAssertEqual(picks[0].episodeID, ep.id)
    }

    func testPicksAreDedupedByShow() {
        let sub = UUID()
        let ep1 = makeEpisode(subID: sub, pubDate: Date().addingTimeInterval(-3_600))
        let ep2 = makeEpisode(subID: sub, pubDate: Date())
        let inputs = AgentPicksInputs(
            unplayed: [ep1, ep2],
            inProgress: [],
            subscriptionTitles: [sub: "Show"],
            memorySnippets: [],
            topicNames: []
        )
        let picks = AgentPicksFallback.derive(inputs: inputs)
        XCTAssertEqual(picks.count, 1)
        // Newest unplayed wins per show.
        XCTAssertEqual(picks[0].episodeID, ep2.id)
    }

    func testStalestShowsRankFirst() {
        // Sub A's freshest unplayed is older than B's, which is older than C's.
        // The fallback heuristic surfaces "shows you've been ignoring" —
        // stalest first.
        let subA = UUID(); let subB = UUID(); let subC = UUID()
        let now = Date()
        let epA = makeEpisode(subID: subA, pubDate: now.addingTimeInterval(-7 * 86_400))
        let epB = makeEpisode(subID: subB, pubDate: now.addingTimeInterval(-2 * 86_400))
        let epC = makeEpisode(subID: subC, pubDate: now)
        let inputs = AgentPicksInputs(
            unplayed: [epC, epB, epA],
            inProgress: [],
            subscriptionTitles: [subA: "A", subB: "B", subC: "C"],
            memorySnippets: [],
            topicNames: []
        )
        let picks = AgentPicksFallback.derive(inputs: inputs)
        XCTAssertEqual(picks.map(\.episodeID), [epA.id, epB.id, epC.id])
        XCTAssertTrue(picks[0].isHero)
        XCTAssertFalse(picks[1].isHero)
        XCTAssertFalse(picks[2].isHero)
    }

    func testCapsAtThreePicks() {
        let now = Date()
        let subs = (0..<5).map { _ in UUID() }
        let eps = subs.enumerated().map { idx, sub in
            makeEpisode(subID: sub, pubDate: now.addingTimeInterval(Double(-idx) * 86_400))
        }
        let titles = Dictionary(uniqueKeysWithValues: subs.enumerated().map { ($1, "S\($0)") })
        let inputs = AgentPicksInputs(
            unplayed: eps,
            inProgress: [],
            subscriptionTitles: titles,
            memorySnippets: [],
            topicNames: []
        )
        let picks = AgentPicksFallback.derive(inputs: inputs)
        XCTAssertEqual(picks.count, 3)
    }

    // MARK: - Fixtures

    private func makeEpisode(subID: UUID, pubDate: Date) -> Episode {
        Episode(
            podcastID: subID,
            guid: UUID().uuidString,
            title: "T",
            pubDate: pubDate,
            enclosureURL: URL(string: "https://example.com/x.mp3")!
        )
    }
}
