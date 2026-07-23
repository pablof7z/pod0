import Foundation
import Pod0Core

/// Provider-format adapter for the bounded product-proof tool surface selected
/// by Rust. Swift describes only the transport shape; Rust parses every call,
/// authorizes durable actions, and remains the sole policy owner.
///
/// Issue #138 moves these provider-neutral definitions through the facade and
/// deletes this temporary native adapter.
@MainActor
enum CoreAgentToolSchemas {
    static func schemas(for tools: [AgentToolName]) -> [[String: Any]]? {
        let result = tools.compactMap(schema)
        return result.count == tools.count ? result : nil
    }

    static func wireName(_ tool: AgentToolName) -> String? {
        switch tool {
        case .createNote: "create_note"
        case .listSubscriptions: "list_subscriptions"
        case .listPodcasts: "list_podcasts"
        case .listEpisodes: "list_episodes"
        case .listInProgress: "list_in_progress"
        case .listRecentUnplayed: "list_recent_unplayed"
        case .searchEpisodes: "search_episodes"
        case .pausePlayback: "pause_playback"
        case .setPlaybackRate: "set_playback_rate"
        default: nil
        }
    }

    private static func schema(_ toolName: AgentToolName) -> [String: Any]? {
        switch toolName {
        case .createNote:
            tool(
                name: "create_note",
                description: "Save a note or reflection for the user.",
                properties: [
                    "text": stringProperty("The note content to save."),
                ],
                required: ["text"]
            )
        case .listSubscriptions:
            noArgumentTool(
                name: "list_subscriptions",
                description: "List the podcasts the user currently subscribes to."
            )
        case .listPodcasts:
            noArgumentTool(
                name: "list_podcasts",
                description: "List every podcast currently known to the user's library."
            )
        case .listEpisodes:
            tool(
                name: "list_episodes",
                description: "List episodes for one podcast, newest first.",
                properties: [
                    "podcast_id": stringProperty(
                        "Stable podcast UUID returned by another library tool."
                    ),
                ],
                required: ["podcast_id"]
            )
        case .listInProgress:
            noArgumentTool(
                name: "list_in_progress",
                description: "List episodes the user started but has not finished."
            )
        case .listRecentUnplayed:
            noArgumentTool(
                name: "list_recent_unplayed",
                description: "List recently published episodes the user has not played."
            )
        case .searchEpisodes:
            tool(
                name: "search_episodes",
                description: "Search episode metadata in the user's library for topical or fuzzy recall.",
                properties: [
                    "query": stringProperty("Natural-language search query."),
                    "scope": stringProperty("Optional podcast UUID to constrain the search."),
                    "limit": [
                        "type": "integer",
                        "description": "Maximum results from 1 through 25. Defaults to 10.",
                        "minimum": 1,
                        "maximum": 25,
                    ],
                ],
                required: ["query"]
            )
        case .pausePlayback:
            noArgumentTool(
                name: "pause_playback",
                description: "Pause current podcast playback and persist the playhead."
            )
        case .setPlaybackRate:
            tool(
                name: "set_playback_rate",
                description: "Set the active podcast playback speed.",
                properties: [
                    "rate": [
                        "type": "number",
                        "description": "Playback speed multiplier from 0.5 through 3.0.",
                        "minimum": 0.5,
                        "maximum": 3.0,
                    ],
                ],
                required: ["rate"]
            )
        default:
            nil
        }
    }

    private static func stringProperty(_ description: String) -> [String: Any] {
        ["type": "string", "description": description]
    }

    private static func noArgumentTool(name: String, description: String) -> [String: Any] {
        tool(name: name, description: description, properties: [:], required: [])
    }

    private static func tool(
        name: String,
        description: String,
        properties: [String: Any],
        required: [String]
    ) -> [String: Any] {
        [
            "type": "function",
            "function": [
                "name": name,
                "description": description,
                "parameters": [
                    "type": "object",
                    "properties": properties,
                    "required": required,
                    "additionalProperties": false,
                ] as [String: Any],
            ] as [String: Any],
        ]
    }
}
