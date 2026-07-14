import Foundation

// MARK: - LiveBriefingComposerAdapter
//
// Adapts the live `BriefingComposer` to the agent-tool's smaller surface:
// scope/length/style parameters at the boundary, a `BriefingResult` handle
// out. Synchronous progress events from the composer are dropped since the
// agent loop has nowhere to render them in-band.
//
// Mapping rules between the agent's freeform strings and the strict enums
// `BriefingComposer` expects live in the static helpers at the bottom — the
// agent picks scopes like `"this_week"`, `"unlistened"`, or a topic phrase.
// Unknown strings are treated as topic prompts, and UUID-like show scopes stay
// narrow so the composer can fail explicitly until show-id scoping is modeled.

struct LiveBriefingComposerAdapter: BriefingComposerProtocol {

    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    func composeBriefing(scope: String, lengthMinutes: Int, style: String?) async throws -> BriefingResult {
        let request = BriefingRequest(
            scope: Self.briefingScope(from: scope),
            length: Self.briefingLength(forMinutes: lengthMinutes),
            style: Self.briefingStyle(from: style),
            freeformQuery: Self.freeformQuery(scope: scope)
        )
        let composer = try await Self.makeComposer()
        let result = try await composer.compose(request: request, progress: { _ in })
        let episodeIDs: [EpisodeID] = Array(
            Set(result.script.segments.flatMap { segment in
                segment.attributions.compactMap { $0.episodeID?.uuidString }
            })
        )
        return BriefingResult(
            briefingID: result.script.id.uuidString,
            title: result.script.title,
            estimatedSeconds: Int(result.script.totalDurationSeconds),
            episodeIDs: episodeIDs,
            scriptPreview: result.script.subtitle
        )
    }

    // MARK: Construction

    @MainActor
    private static func makeComposer() throws -> BriefingComposer {
        let storage = try BriefingStorage()
        let settings = RAGService.shared.appStore?.state.settings ?? Settings()
        let reference = LLMModelReference(storedID: settings.memoryCompilationModel)
        let apiKey = (try? LLMProviderCredentialResolver.apiKey(for: reference.provider)) ?? nil
        return BriefingComposer(storage: storage, apiKey: apiKey, model: reference.storedID)
    }

    // MARK: Mapping

    /// The agent passes free-form scope strings (`"this_week"`, `"unlistened"`,
    /// or a podcast UUID). Map them to the closest `BriefingScope` enum case
    /// without silently widening unknown scoped requests to the whole library.
    static func briefingScope(from raw: String) -> BriefingScope {
        let key = raw.lowercased()
        switch key {
        case "this_week", "thisweek", "week":         return .thisWeek
        case "unlistened":                            return .mySubscriptions
        case "this_show", "thisshow", "show":         return .thisShow
        case "this_topic", "thistopic", "topic":      return .thisTopic
        default:
            return UUID(uuidString: raw) == nil ? .thisTopic : .thisShow
        }
    }

    static func briefingLength(forMinutes minutes: Int) -> BriefingLength {
        switch minutes {
        case ...4:        return .quick
        case 5...10:      return .medium
        case 11...20:     return .extended
        default:          return .deepDive
        }
    }

    static func briefingStyle(from raw: String?) -> BriefingStyle {
        switch raw?.lowercased() {
        case "news":                          return .morning
        case "deep_dive", "deepdive":         return .topicAcrossLibrary
        case "quick_hits", "quickhits":       return .weeklyTLDR
        default:                              return .morning
        }
    }

    /// Surfaces a freeform query when the scope string isn't a recognised enum
    /// keyword and isn't a UUID — the composer treats it as a topic prompt.
    static func freeformQuery(scope: String) -> String? {
        let key = scope.lowercased()
        let knownKeywords: Set<String> = [
            "this_week", "thisweek", "week",
            "unlistened",
            "this_show", "thisshow", "show",
            "this_topic", "thistopic", "topic",
        ]
        if knownKeywords.contains(key) { return nil }
        if UUID(uuidString: scope) != nil { return nil }
        return scope
    }
}
