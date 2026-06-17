import Foundation

// MARK: - AgentEmptyStateProjection
//
// Swift owns the rendered strings. Rust owns which suggestion context applies
// to the current library/playback state.

enum AgentEmptyStateSuggestionContext: String, Decodable {
    case resume
    case subscribed
    case onboarding
}

struct AgentEmptyStateProjection: Decodable {
    let suggestionContext: AgentEmptyStateSuggestionContext

    static func load(store: AppStateStore) -> AgentEmptyStateProjection {
        guard let envelope = store.kernel?.agentEmptyStateEnvelope(),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.agentEmptyState.decode(AgentEmptyStateProjection.self, from: data)
        else { return AgentEmptyStateProjection(suggestionContext: .onboarding) }
        return decoded
    }
}

private extension JSONDecoder {
    static let agentEmptyState: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }()
}
