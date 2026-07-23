import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreAgentToolSchemasTests: XCTestCase {
    func testEncodesRustOwnedNamesDescriptionsAndOrder() throws {
        let definitions = [
            definition(.createNote, "create_note", "Save a note.", [
                parameter("text", "Note text.", .text, required: true),
            ]),
            definition(.recordMemory, "record_memory", "Remember a preference.", [
                parameter("text", "Memory text.", .text, required: true),
            ]),
        ]

        let schemas = try XCTUnwrap(CoreAgentToolSchemas.schemas(for: definitions))

        XCTAssertEqual(
            schemas.compactMap(Self.functionName),
            ["create_note", "record_memory"]
        )
        XCTAssertEqual(
            (schemas[1]["function"] as? [String: Any])?["description"] as? String,
            "Remember a preference."
        )
    }

    func testEncodesTypedBoundsAndRequiredFieldsWithoutNativePolicy() throws {
        let definition = definition(.searchEpisodes, "search_episodes", "Search.", [
            parameter("query", "Query.", .text, required: true),
            parameter(
                "limit",
                "Limit.",
                .integer(minimum: 1, maximum: 25),
                required: false
            ),
            parameter(
                "rate",
                "Rate.",
                .decimalPermille(minimum: 500, maximum: 3_000),
                required: true
            ),
        ])

        let schema = try XCTUnwrap(CoreAgentToolSchemas.schemas(for: [definition])?.first)
        let parameters = try Self.parameters(in: schema)
        let properties = try XCTUnwrap(parameters["properties"] as? [String: [String: Any]])

        XCTAssertEqual(parameters["required"] as? [String], ["query", "rate"])
        XCTAssertEqual(parameters["additionalProperties"] as? Bool, false)
        XCTAssertEqual(properties["query"]?["type"] as? String, "string")
        XCTAssertEqual(properties["limit"]?["minimum"] as? Int64, 1)
        XCTAssertEqual(properties["limit"]?["maximum"] as? Int64, 25)
        XCTAssertEqual(properties["rate"]?["minimum"] as? Double, 0.5)
        XCTAssertEqual(properties["rate"]?["maximum"] as? Double, 3.0)
    }

    func testMalformedOrDuplicateDefinitionsFailTheWholeRequest() {
        let valid = definition(.createNote, "create_note", "Save.", [])
        let duplicate = definition(.recordMemory, "create_note", "Remember.", [])
        let invalidBounds = definition(.searchEpisodes, "search_episodes", "Search.", [
            parameter(
                "limit",
                "Limit.",
                .integer(minimum: 25, maximum: 1),
                required: false
            ),
        ])

        XCTAssertNil(CoreAgentToolSchemas.schemas(for: [valid, duplicate]))
        XCTAssertNil(CoreAgentToolSchemas.schemas(for: [invalidBounds]))
    }

    private func definition(
        _ tool: AgentToolName,
        _ wireName: String,
        _ description: String,
        _ parameters: [AgentToolParameterDefinition]
    ) -> AgentToolDefinition {
        AgentToolDefinition(
            tool: tool,
            wireName: wireName,
            description: description,
            parameters: parameters
        )
    }

    private func parameter(
        _ name: String,
        _ description: String,
        _ kind: AgentToolParameterKind,
        required: Bool
    ) -> AgentToolParameterDefinition {
        AgentToolParameterDefinition(
            name: name,
            description: description,
            kind: kind,
            required: required
        )
    }

    private static func functionName(_ schema: [String: Any]) -> String? {
        (schema["function"] as? [String: Any])?["name"] as? String
    }

    private static func parameters(in schema: [String: Any]) throws -> [String: Any] {
        let function = try XCTUnwrap(schema["function"] as? [String: Any])
        return try XCTUnwrap(function["parameters"] as? [String: Any])
    }
}

func agentNoteDefinition() -> AgentToolDefinition {
    AgentToolDefinition(
        tool: .createNote,
        wireName: "create_note",
        description: "Save a note.",
        parameters: [AgentToolParameterDefinition(
            name: "text",
            description: "The note text.",
            kind: .text,
            required: true
        )]
    )
}
