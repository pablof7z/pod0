import XCTest
@testable import Podcastr

/// Coverage for the JSON parser that ingests the LLM picks response.
/// Tolerance to markdown fences + extra prose is the failure mode we're
/// guarding against — models love wrapping JSON in ``` fences and adding
/// "Here you go!" preambles.
final class AgentPicksPromptParseTests: XCTestCase {

    func testParsesPlainJSON() {
        let hero = UUID()
        let sec = UUID()
        let raw = """
        {
          "hero": { "episode_id": "\(hero.uuidString)", "reason": "Two sentences." },
          "secondaries": [
            { "episode_id": "\(sec.uuidString)", "reason": "One sentence." }
          ]
        }
        """
        let picks = AgentPicksPrompt.parse(raw, knownEpisodeIDs: [hero, sec])
        XCTAssertEqual(picks.count, 2)
        XCTAssertTrue(picks[0].isHero)
        XCTAssertEqual(picks[0].episodeID, hero)
        XCTAssertFalse(picks[1].isHero)
        XCTAssertEqual(picks[1].episodeID, sec)
    }

    func testStripsMarkdownFences() {
        let hero = UUID()
        let raw = """
        Sure! Here are your picks:

        ```json
        {
          "hero": { "episode_id": "\(hero.uuidString)", "reason": "Yes." },
          "secondaries": []
        }
        ```
        """
        let picks = AgentPicksPrompt.parse(raw, knownEpisodeIDs: [hero])
        XCTAssertEqual(picks.count, 1)
        XCTAssertEqual(picks[0].episodeID, hero)
    }

    func testDropsUnknownEpisodeIDs() {
        let hero = UUID()
        let unknown = UUID()
        let raw = """
        { "hero": { "episode_id": "\(unknown.uuidString)", "reason": "x" }, "secondaries": [] }
        """
        // Only `hero` is in the candidate set — the unknown ID must NOT
        // produce a pick (the agent hallucinated it).
        let picks = AgentPicksPrompt.parse(raw, knownEpisodeIDs: [hero])
        XCTAssertTrue(picks.isEmpty)
    }

    func testCapsSecondariesAtTwo() {
        let hero = UUID()
        let s1 = UUID(); let s2 = UUID(); let s3 = UUID()
        let raw = """
        {
          "hero": { "episode_id": "\(hero.uuidString)", "reason": "x" },
          "secondaries": [
            { "episode_id": "\(s1.uuidString)", "reason": "a" },
            { "episode_id": "\(s2.uuidString)", "reason": "b" },
            { "episode_id": "\(s3.uuidString)", "reason": "c" }
          ]
        }
        """
        let picks = AgentPicksPrompt.parse(raw, knownEpisodeIDs: [hero, s1, s2, s3])
        // hero + at most 2 secondaries.
        XCTAssertEqual(picks.count, 3)
        XCTAssertEqual(picks.filter { !$0.isHero }.count, 2)
    }

    func testReturnsEmptyForMalformedJSON() {
        let picks = AgentPicksPrompt.parse("not json at all", knownEpisodeIDs: [])
        XCTAssertTrue(picks.isEmpty)
    }
}
