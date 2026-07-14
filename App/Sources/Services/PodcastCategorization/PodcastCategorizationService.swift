import Foundation
import Observation
import os.log

// MARK: - Errors

enum CategorizationError: LocalizedError {
    case noAPIKey(provider: String)
    case noSubscriptions
    case noModelSelected
    case invalidResponse
    case httpError(status: Int, body: String)

    var errorDescription: String? {
        switch self {
        case .noAPIKey(let provider):
            return "\(provider) is not connected. Add a key in Settings → Intelligence → Providers."
        case .noSubscriptions:
            return "Add at least one podcast subscription before generating categories."
        case .noModelSelected:
            return "Choose a categorization model in Settings → Intelligence → Models."
        case .invalidResponse:
            return "The model returned an invalid response. Try again."
        case .httpError(let status, let body):
            return "Categorization API error (\(status)): \(body.prefix(200))"
        }
    }
}

// MARK: - Service

/// Drives one-shot LLM categorization of the user's subscription library.
///
/// Why a singleton + `@Observable`: the recompute is rare (user-triggered
/// from Settings), but the UI wants to reflect `isRunning` from anywhere
/// the action can be kicked off. Single-flight is enforced by checking
/// `isRunning` before starting; `recompute(store:)` returns immediately
/// if a run is already in progress.
///
/// Networking goes through `WikiOpenRouterClient`, which already owns the
/// provider-specific OpenRouter/Ollama request shape. `BYOKConnectService`
/// only handles key acquisition.
@MainActor
@Observable
final class PodcastCategorizationService {

    static let shared = PodcastCategorizationService()

    nonisolated private static let logger = Logger.app("PodcastCategorizationService")

    /// Wall-clock of the last successful recompute. Drives the "last run"
    /// row that the Settings sheet renders.
    private(set) var lastRun: Date?

    /// True while a recompute is in flight. UI binds to this for spinner +
    /// disabled-button state.
    private(set) var isRunning: Bool = false

    private let urlSession: URLSession

    init(urlSession: URLSession = .shared) {
        self.urlSession = urlSession
    }

    // MARK: - Public entry point

    /// Sends one prompt to the configured model/provider, validates the
    /// response, and persists the resulting categories on the store.
    ///
    /// Throws `.noAPIKey` if the selected provider isn't connected, `.noSubscriptions`
    /// if the library is empty, `.invalidResponse` for any parser/validation
    /// failure, and `.httpError` for non-2xx responses.
    func recompute(store: AppStateStore) async throws {
        // Single-flight: a second concurrent caller silently no-ops rather
        // than queueing or throwing. The only entry-point today is the
        // Settings sheet, which gates its own UI on `isRunning`; future
        // callers must check `isRunning` themselves before calling.
        guard !isRunning else { return }
        // Categorize only podcasts the user actively follows — synthetic
        // (Agent Generated, Unknown) and orphan single-episode podcasts
        // should never end up in the user's category taxonomy.
        let followedPodcastIDs = Set(store.state.subscriptions.map(\.podcastID))
        let podcasts = store.state.podcasts.filter { followedPodcastIDs.contains($0.id) && $0.kind == .rss }
        guard !podcasts.isEmpty else {
            throw CategorizationError.noSubscriptions
        }
        let modelReference = LLMModelReference(storedID: store.state.settings.categorizationModel)
        guard !modelReference.isEmpty else {
            throw CategorizationError.noModelSelected
        }

        let apiKey: String
        do {
            guard let key = try LLMProviderCredentialResolver.apiKey(for: modelReference.provider),
                  !key.isEmpty else {
                throw CategorizationError.noAPIKey(provider: modelReference.provider.displayName)
            }
            apiKey = key
        } catch let error as CategorizationError {
            throw error
        } catch {
            Self.logger.error("credential resolve failed: \(error, privacy: .public)")
            throw CategorizationError.noAPIKey(provider: modelReference.provider.displayName)
        }

        let requestedModel = modelReference.storedID
        Self.logger.info("recompute starting subs=\(podcasts.count, privacy: .public) model=\(requestedModel, privacy: .public)")

        isRunning = true
        defer { isRunning = false }

        let client = WikiOpenRouterClient(
            mode: .live(apiKey: apiKey, modelReference: modelReference),
            urlSession: urlSession
        )
        let rawContent = try await client.compile(
            systemPrompt: PodcastCategorizationPrompt.systemPrompt(),
            userPrompt: PodcastCategorizationPrompt.userPrompt(podcasts: podcasts),
            feature: CostFeature.categorizationRecompute
        )

        let generatedAt = Date()
        let categories = try PodcastCategorizationParser.categories(
            from: rawContent,
            podcasts: podcasts,
            generatedAt: generatedAt,
            model: requestedModel
        )

        store.setCategories(categories)
        lastRun = generatedAt
        Self.logger.info("recompute complete categories=\(categories.count, privacy: .public)")
    }

}
