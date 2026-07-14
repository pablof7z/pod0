import XCTest
@testable import Podcastr

/// Coverage for `AgentRunToolFormatter` — the per-tool render of the
/// agent Run Logs detail surface. The formatter previously rendered raw
/// UUIDs and unformatted seconds, making the trace barely readable for
/// podcast-domain calls (`play_episode`, `search_episodes`, …).
/// The new `ValueResolver` lets the view pluck friendly strings from
/// `AppStateStore` without coupling the formatter to live state.
final class AgentRunToolFormatterTests: XCTestCase {

    // MARK: - Without resolver (legacy behaviour)

    func testGenericRenderForArgs() {
        let f = AgentRunToolFormatter.format(
            toolName: "play_episode",
            arguments: ["episode_id": .string("abc"), "start_seconds": .int(420)]
        )
        XCTAssertEqual(f.title, "Play Episode")
        XCTAssertEqual(f.detail, "episode_id: \u{201C}abc\u{201D}, start_seconds: 420")
    }

    func testHumanizesUnderscoredToolName() {
        let f = AgentRunToolFormatter.format(toolName: "list_in_progress", arguments: [:])
        XCTAssertEqual(f.title, "List In Progress")
        XCTAssertNil(f.detail)
    }

    // MARK: - Resolver injection

    func testResolverOverridesScalarRender() {
        let resolver: AgentRunToolFormatter.ValueResolver = { key, _ in
            key == "episode_id" ? "\u{201C}How to Think About Keto\u{201D}" : nil
        }
        let f = AgentRunToolFormatter.format(
            toolName: "play_episode",
            arguments: ["episode_id": .string("0123-uuid"), "start_seconds": .int(420)],
            resolveValue: resolver
        )
        // episode_id replaced; start_seconds falls through to scalar render.
        XCTAssertEqual(
            f.detail,
            "episode_id: \u{201C}How to Think About Keto\u{201D}, start_seconds: 420"
        )
    }

    func testResolverNilReturnFallsThroughToScalar() {
        // Resolver returning nil means "I have no opinion on this value."
        // The formatter must not silently swallow the field.
        let resolver: AgentRunToolFormatter.ValueResolver = { _, _ in nil }
        let f = AgentRunToolFormatter.format(
            toolName: "noop",
            arguments: ["x": .int(5)],
            resolveValue: resolver
        )
        XCTAssertEqual(f.detail, "x: 5")
    }

    func testResolverReceivesEachArg() {
        var seenKeys: [String] = []
        let resolver: AgentRunToolFormatter.ValueResolver = { key, _ in
            seenKeys.append(key); return nil
        }
        _ = AgentRunToolFormatter.format(
            toolName: "x",
            arguments: ["a": .int(1), "b": .int(2), "c": .int(3)],
            resolveValue: resolver
        )
        XCTAssertEqual(Set(seenKeys), ["a", "b", "c"])
    }
}
