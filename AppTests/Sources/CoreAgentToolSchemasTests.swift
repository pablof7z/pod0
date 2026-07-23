import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreAgentToolSchemasTests: XCTestCase {
    func testProductProofSchemasHaveCanonicalNamesAndOrder() throws {
        let tools: [AgentToolName] = [
            .createNote,
            .listSubscriptions,
            .listPodcasts,
            .listEpisodes,
            .listInProgress,
            .listRecentUnplayed,
            .searchEpisodes,
            .queryTranscripts,
            .pausePlayback,
            .setPlaybackRate,
            .generateTtsEpisode,
        ]

        let schemas = try XCTUnwrap(CoreAgentToolSchemas.schemas(for: tools))

        XCTAssertEqual(
            schemas.compactMap(Self.functionName),
            [
                "create_note",
                "list_subscriptions",
                "list_podcasts",
                "list_episodes",
                "list_in_progress",
                "list_recent_unplayed",
                "search_episodes",
                "query_transcripts",
                "pause_playback",
                "set_playback_rate",
                "generate_tts_episode",
            ]
        )
    }

    func testSchemasMatchTheRustParserContract() throws {
        let schemas = try XCTUnwrap(CoreAgentToolSchemas.schemas(for: [
            .createNote,
            .listEpisodes,
            .searchEpisodes,
            .queryTranscripts,
            .setPlaybackRate,
            .generateTtsEpisode,
        ]))
        let schemasByName = Dictionary(uniqueKeysWithValues: schemas.compactMap { schema in
            Self.functionName(schema).map { ($0, schema) }
        })

        XCTAssertEqual(try Self.required(in: XCTUnwrap(schemasByName["create_note"])), ["text"])
        XCTAssertEqual(
            try Self.required(in: XCTUnwrap(schemasByName["list_episodes"])),
            ["podcast_id"]
        )
        XCTAssertEqual(try Self.required(in: XCTUnwrap(schemasByName["search_episodes"])), ["query"])
        XCTAssertEqual(
            try Self.required(in: XCTUnwrap(schemasByName["query_transcripts"])),
            ["query"]
        )
        XCTAssertEqual(
            try Self.required(in: XCTUnwrap(schemasByName["set_playback_rate"])),
            ["rate"]
        )
        XCTAssertEqual(
            try Self.required(in: XCTUnwrap(schemasByName["generate_tts_episode"])),
            ["title", "script"]
        )
        for schema in schemas {
            XCTAssertEqual(try Self.parameters(in: schema)["additionalProperties"] as? Bool, false)
        }
    }

    func testUnsupportedToolFailsTheWholeSchemaRequest() {
        XCTAssertNil(CoreAgentToolSchemas.schemas(for: [.createNote, .findSimilarEpisodes]))
        XCTAssertNil(CoreAgentToolSchemas.wireName(.findSimilarEpisodes))
    }

    private static func functionName(_ schema: [String: Any]) -> String? {
        (schema["function"] as? [String: Any])?["name"] as? String
    }

    private static func parameters(in schema: [String: Any]) throws -> [String: Any] {
        let function = try XCTUnwrap(schema["function"] as? [String: Any])
        return try XCTUnwrap(function["parameters"] as? [String: Any])
    }

    private static func required(in schema: [String: Any]) throws -> [String] {
        try XCTUnwrap(parameters(in: schema)["required"] as? [String])
    }
}
