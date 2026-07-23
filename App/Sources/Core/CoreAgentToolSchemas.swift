import Foundation
import Pod0Core

/// Encodes Rust-owned, provider-neutral tool definitions into the JSON shape
/// required by OpenRouter and Ollama. Swift does not choose tools, descriptions,
/// argument names, required fields, or semantic bounds.
@MainActor
enum CoreAgentToolSchemas {
    static func schemas(for definitions: [AgentToolDefinition]) -> [[String: Any]]? {
        var tools = Set<AgentToolName>()
        var wireNames = Set<String>()
        var result: [[String: Any]] = []
        result.reserveCapacity(definitions.count)
        for definition in definitions {
            guard tools.insert(definition.tool).inserted,
                  wireNames.insert(definition.wireName).inserted,
                  let schema = schema(definition)
            else {
                return nil
            }
            result.append(schema)
        }
        return result
    }

    private static func schema(_ definition: AgentToolDefinition) -> [String: Any]? {
        guard !definition.wireName.isBlank,
              !definition.description.isBlank
        else {
            return nil
        }
        var properties: [String: Any] = [:]
        var required: [String] = []
        for parameter in definition.parameters {
            guard properties[parameter.name] == nil,
                  let property = property(parameter)
            else {
                return nil
            }
            properties[parameter.name] = property
            if parameter.required {
                required.append(parameter.name)
            }
        }
        return [
            "type": "function",
            "function": [
                "name": definition.wireName,
                "description": definition.description,
                "parameters": [
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false,
                ] as [String: Any],
            ] as [String: Any],
        ]
    }

    private static func property(
        _ parameter: AgentToolParameterDefinition
    ) -> [String: Any]? {
        guard !parameter.name.isBlank,
              !parameter.description.isBlank
        else {
            return nil
        }
        switch parameter.kind {
        case .text:
            return [
                "type": "string",
                "description": parameter.description,
            ]
        case .integer(let minimum, let maximum):
            guard minimum <= maximum else { return nil }
            return [
                "type": "integer",
                "description": parameter.description,
                "minimum": minimum,
                "maximum": maximum,
            ]
        case .decimalPermille(let minimum, let maximum):
            guard minimum <= maximum else { return nil }
            return [
                "type": "number",
                "description": parameter.description,
                "minimum": Double(minimum) / 1_000,
                "maximum": Double(maximum) / 1_000,
            ]
        }
    }
}
