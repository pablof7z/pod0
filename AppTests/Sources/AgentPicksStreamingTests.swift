import XCTest
@testable import Podcastr

/// Coverage for `AgentPicksStreamingParser`. The parser drives the
/// progressive-render UX for agent picks — each test simulates the way
/// `AgentLLMClient.streamCompletion`'s `onPartialContent` callback delivers
/// a *cumulative* buffer (not a delta) by feeding successive prefixes of
/// the same payload to the parser and asserting on the new events each
/// `feed` call returns.
final class AgentPicksStreamingTests: XCTestCase {

    // MARK: - Fixtures

    private let heroID = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
    private let secondaryAID = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
    private let secondaryBID = UUID(uuidString: "33333333-3333-3333-3333-333333333333")!

    private func known() -> Set<UUID> {
        [heroID, secondaryAID, secondaryBID]
    }

    private func fullPayload(includeBSpoken: Bool = false) -> String {
        // Hero-first ordering matches the system prompt's "emit hero first"
        // rule the streaming logic depends on for the shimmer slot.
        """
        {
          "hero": {
            "episode_id": "\(heroID.uuidString)",
            "reason": "Top of the field on Lp(a). Two new biomarker studies plus the speaker your memory flagged last week.",
            "spoken_reason": "This one ties straight to Lp(a) you've been tracking and to the speaker your memory flagged last week."
          },
          "secondaries": [
            { "episode_id": "\(secondaryAID.uuidString)", "reason": "A short B-roll dive on sleep apnea." },
            { "episode_id": "\(secondaryBID.uuidString)", "reason": "Cross-show riff on creatine that you may have missed."\(includeBSpoken ? ", \"spoken_reason\": \"Quick listen on creatine.\"" : "") }
          ]
        }
        """
    }

    // MARK: - Tests

    /// Smoke test: feeding the entire payload in one shot yields all
    /// three picks in hero-first ordering, each tagged with the correct slot.
    func testParsesFullPayloadInOneFeed() {
        let parser = AgentPicksStreamingParser()
        let events = parser.feed(fullPayload(), knownEpisodeIDs: known())
        XCTAssertEqual(events.count, 3)
        XCTAssertEqual(events[0].slot, .hero)
        XCTAssertEqual(events[0].episodeID, heroID)
        XCTAssertFalse(events[0].spokenReason.isEmpty,
                       "Hero must surface a spoken_reason for the Read-aloud affordance.")
        XCTAssertEqual(events[1].slot, .secondary)
        XCTAssertEqual(events[1].episodeID, secondaryAID)
        XCTAssertEqual(events[2].slot, .secondary)
        XCTAssertEqual(events[2].episodeID, secondaryBID)
    }

    /// Incremental emission: walking a prefix one character at a time the
    /// parser should emit events *as soon as* each inner object's closing
    /// brace lands, and never more than once per inner object.
    func testEmitsPicksIncrementallyAsTheBufferGrows() {
        let parser = AgentPicksStreamingParser()
        let payload = fullPayload()
        var emitted: [AgentPicksStreamEvent] = []
        // Stride at 32 chars so we test that re-scanning the buffer doesn't
        // emit duplicates — the parser must keep its own cursor.
        var idx = 0
        while idx < payload.count {
            idx = min(idx + 32, payload.count)
            let prefix = String(payload.prefix(idx))
            let chunk = parser.feed(prefix, knownEpisodeIDs: known())
            emitted.append(contentsOf: chunk)
        }
        XCTAssertEqual(emitted.count, 3,
                       "Parser must emit each pick exactly once across the stream.")
        XCTAssertEqual(emitted.map(\.episodeID), [heroID, secondaryAID, secondaryBID])
    }

    /// Hero arrives first: after feeding the buffer up through the hero's
    /// closing brace but before the secondaries are written, only the
    /// hero event should have been emitted.
    func testHeroLandsBeforeSecondaries() {
        let parser = AgentPicksStreamingParser()
        let payload = fullPayload()

        // Find the prefix that ends right after the hero object closes.
        // We locate the *second* `}` (first one closes hero — the outer
        // object stays open) and feed everything up to that point.
        let chars = Array(payload)
        var depth = 0
        var inString = false
        var escape = false
        var heroEnd: Int?
        for (i, ch) in chars.enumerated() {
            if inString {
                if escape { escape = false }
                else if ch == "\\" { escape = true }
                else if ch == "\"" { inString = false }
                continue
            }
            switch ch {
            case "\"": inString = true
            case "{":  depth += 1
            case "}":
                if depth == 2 {
                    heroEnd = i
                }
                depth -= 1
            default:   break
            }
            if heroEnd != nil { break }
        }
        XCTAssertNotNil(heroEnd, "Test fixture must contain a hero object.")
        let prefix = String(chars[0...heroEnd!])

        let events = parser.feed(prefix, knownEpisodeIDs: known())
        XCTAssertEqual(events.count, 1, "Only hero should have streamed in.")
        XCTAssertEqual(events.first?.slot, .hero)
    }

    /// Markdown-fence tolerance: the parser locks onto the first `{` and
    /// ignores any preamble (a fence opener, "Here are…" prose, etc.).
    func testStripsLeadingMarkdownFencePreamble() {
        let parser = AgentPicksStreamingParser()
        let wrapped = """
        Sure thing! Here you go:
        ```json
        \(fullPayload())
        ```
        """
        let events = parser.feed(wrapped, knownEpisodeIDs: known())
        XCTAssertEqual(events.count, 3)
        XCTAssertEqual(events[0].slot, .hero)
    }

    /// Unknown episode IDs (the model hallucinating beyond the candidate
    /// list) are dropped silently. The streaming UX never gets to render
    /// a card the agent can't justify.
    func testDropsUnknownEpisodeIDs() {
        let parser = AgentPicksStreamingParser()
        let stranger = UUID()
        let payload = """
        {
          "hero": { "episode_id": "\(stranger.uuidString)", "reason": "made up" },
          "secondaries": [
            { "episode_id": "\(secondaryAID.uuidString)", "reason": "real" }
          ]
        }
        """
        let events = parser.feed(payload, knownEpisodeIDs: known())
        // Only the secondary survives — the hallucinated hero is dropped.
        XCTAssertEqual(events.count, 1)
        XCTAssertEqual(events.first?.episodeID, secondaryAID)
    }

    /// Idempotency: re-feeding the exact same buffer must not emit duplicate
    /// picks. The view layer relies on event-once semantics so it can
    /// `append` straight into the bundle without an after-the-fact dedupe.
    func testReFeedingDoesNotDuplicateEvents() {
        let parser = AgentPicksStreamingParser()
        let payload = fullPayload()
        let first = parser.feed(payload, knownEpisodeIDs: known())
        let second = parser.feed(payload, knownEpisodeIDs: known())
        XCTAssertEqual(first.count, 3)
        XCTAssertTrue(second.isEmpty, "No new events on a no-op re-feed.")
    }
}
