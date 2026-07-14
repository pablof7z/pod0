import Foundation
import SwiftUI

/// Filter criteria applied on top of `AgentRunLogger.runs` in
/// `AgentRunListView`. Selection semantics:
///   • Empty set for `sources` / `outcomes` means "no filter" — every
///     run matches that category. The user opts *in* to a subset by
///     tapping chips.
///   • `toolNameQuery` is a case-insensitive substring match against
///     every `AgentToolDispatch.toolName` across every turn. Empty =
///     no filter.
///   • Categories compose with AND. Inside a category, multi-select
///     is OR.
struct AgentRunFilter: Equatable {
    var sources: Set<AgentRunSource>
    var outcomes: Set<AgentRunOutcome>
    var toolNameQuery: String

    static let empty = AgentRunFilter(sources: [], outcomes: [], toolNameQuery: "")

    var isEmpty: Bool {
        sources.isEmpty && outcomes.isEmpty && toolNameQuery.trimmingCharacters(in: .whitespaces).isEmpty
    }

    func matches(_ run: AgentRun) -> Bool {
        if !sources.isEmpty, !sources.contains(run.source) { return false }
        if !outcomes.isEmpty, !outcomes.contains(run.finalOutcome) { return false }
        let q = toolNameQuery.trimmingCharacters(in: .whitespaces)
        if !q.isEmpty {
            let needle = q.lowercased()
            let hit = run.turns.contains { turn in
                turn.toolDispatches.contains { $0.toolName.lowercased().contains(needle) }
            }
            if !hit { return false }
        }
        return true
    }
}

/// `@AppStorage`-backed binding for `AgentRunFilter`. `@AppStorage`
/// can't hold a `Set` directly, so sources/outcomes serialise as
/// comma-joined `rawValue` strings. An empty string = empty set =
/// no filter applied. Tool query stores as a plain `String`.
@MainActor
@Observable
final class AgentRunFilterStore {

    private static let sourcesKey = "agentRunFilter.sources"
    private static let outcomesKey = "agentRunFilter.outcomes"
    private static let toolQueryKey = "agentRunFilter.toolQuery"

    var filter: AgentRunFilter {
        didSet { persist() }
    }

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        let sourcesRaw = defaults.string(forKey: Self.sourcesKey) ?? ""
        let outcomesRaw = defaults.string(forKey: Self.outcomesKey) ?? ""
        let query = defaults.string(forKey: Self.toolQueryKey) ?? ""
        self.filter = AgentRunFilter(
            sources: Self.decodeSources(sourcesRaw),
            outcomes: Self.decodeOutcomes(outcomesRaw),
            toolNameQuery: query
        )
    }

    private let defaults: UserDefaults

    private func persist() {
        defaults.set(Self.encode(filter.sources.map(\.rawValue)), forKey: Self.sourcesKey)
        defaults.set(Self.encode(filter.outcomes.map(\.rawValue)), forKey: Self.outcomesKey)
        defaults.set(filter.toolNameQuery, forKey: Self.toolQueryKey)
    }

    private static func encode(_ values: [String]) -> String {
        values.sorted().joined(separator: ",")
    }

    private static func decodeSources(_ raw: String) -> Set<AgentRunSource> {
        Set(raw.split(separator: ",").compactMap { AgentRunSource(rawValue: String($0)) })
    }

    private static func decodeOutcomes(_ raw: String) -> Set<AgentRunOutcome> {
        Set(raw.split(separator: ",").compactMap { AgentRunOutcome(rawValue: String($0)) })
    }
}
