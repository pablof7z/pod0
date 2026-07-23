import Foundation
import Pod0Core

/// Temporary provider-schema adapter. Rust selects the exact bounded tool set
/// and parses raw calls; issue #138 deletes these Swift schema declarations
/// after the provider-neutral definitions cross the facade.
@MainActor
enum CoreAgentToolSchemas {
    static func schemas(for tools: [AgentToolName]) -> [[String: Any]]? {
        let requestedNames = tools.compactMap(wireName)
        guard requestedNames.count == tools.count else { return nil }
        let available = AgentTools.schema + AgentTools.podcastSchema
        let entries: [(String, [String: Any])] = available.compactMap { schema in
            guard let function = schema["function"] as? [String: Any],
                  let name = function["name"] as? String else { return nil }
            return (name, schema)
        }
        let schemasByName = Dictionary(uniqueKeysWithValues: entries)
        let result = requestedNames.compactMap { schemasByName[$0] }
        return result.count == requestedNames.count ? result : nil
    }

    static func wireName(_ tool: AgentToolName) -> String? {
        switch tool {
        case .createNote: AgentTools.Names.createNote
        case .listSubscriptions: AgentTools.PodcastNames.listSubscriptions
        case .listPodcasts: AgentTools.PodcastNames.listPodcasts
        case .listEpisodes: AgentTools.PodcastNames.listEpisodes
        case .listInProgress: AgentTools.PodcastNames.listInProgress
        case .listRecentUnplayed: AgentTools.PodcastNames.listRecentUnplayed
        case .searchEpisodes: AgentTools.PodcastNames.searchEpisodes
        case .pausePlayback: AgentTools.PodcastNames.pausePlayback
        case .setPlaybackRate: AgentTools.PodcastNames.setPlaybackRate
        default: nil
        }
    }
}
