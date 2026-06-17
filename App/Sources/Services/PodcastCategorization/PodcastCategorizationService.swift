import Foundation
import Observation
import os.log

// MARK: - Errors

enum CategorizationError: LocalizedError {
    case noSubscriptions
    case noModelSelected
    case invalidResponse
    case httpError(status: Int, body: String)

    var errorDescription: String? {
        switch self {
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
/// Networking goes through `ProviderCompletionClient`, whose live mode delegates
/// provider request shaping and credential checks to Rust.
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
    /// Throws `.noSubscriptions` if the library is empty, `.invalidResponse`
    /// for any parser/validation failure, and `.httpError` for non-2xx responses.
    func recompute(store: AppStateStore) async throws {
        // Single-flight: a second concurrent caller silently no-ops rather
        // than queueing or throwing. The only entry-point today is the
        // Settings sheet, which gates its own UI on `isRunning`; future
        // callers must check `isRunning` themselves before calling.
        guard !isRunning else { return }
        let plan = try Self.categorizationPrompt(store: store)
        let modelReference = LLMModelReference(storedID: plan.model)

        let requestedModel = modelReference.storedID
        Self.logger.info("recompute starting model=\(requestedModel, privacy: .public)")

        isRunning = true
        defer { isRunning = false }

        let client = ProviderCompletionClient(
            mode: .live(modelReference: modelReference),
            urlSession: urlSession
        )
        let rawContent = try await client.compile(
            systemPrompt: plan.systemPrompt,
            userPrompt: plan.userPrompt,
            feature: CostFeature.categorizationRecompute
        )

        let categories = try Self.parseCategorizationResponse(rawContent, store: store)
        let generatedAt = categories.first?.generatedAt ?? Date()

        store.setCategories(categories)
        // Mirror the fresh assignments into the kernel-owned substate the UI
        // reads from. Reconcile against the followed set so podcasts dropped
        // from all categories by this recompute have their stale kernel labels
        // cleared (not just the ones that gained labels).
        store.syncUserCategoriesToKernel(reconcilingFollowed: followedPodcastIDs)
        // Mirror per-category transcription disabled state into per-podcast kernel
        // overrides so both projection fields stay in sync after AI recompute.
        store.syncTranscriptionSettingsToKernel()
        lastRun = generatedAt
        Self.logger.info("recompute complete categories=\(categories.count, privacy: .public)")
    }

    private static func categorizationPrompt(store: AppStateStore) throws -> CategorizationPromptPlan {
        guard let envelope = store.kernel?.libraryCategorizationPromptEnvelope(),
              let data = envelope.data(using: .utf8),
              let decoded = try? categorizationDecoder.decode(RustCategorizationPrompt.self, from: data)
        else {
            throw CategorizationError.invalidResponse
        }
        if let error = decoded.error {
            throw mapRustError(error)
        }
        guard let model = decoded.model, !model.isEmpty,
              let systemPrompt = decoded.systemPrompt, !systemPrompt.isEmpty,
              let userPrompt = decoded.userPrompt, !userPrompt.isEmpty
        else {
            throw CategorizationError.invalidResponse
        }
        return CategorizationPromptPlan(
            model: model,
            systemPrompt: systemPrompt,
            userPrompt: userPrompt
        )
    }

    private static func parseCategorizationResponse(
        _ rawContent: String,
        store: AppStateStore
    ) throws -> [PodcastCategory] {
        guard let envelope = store.kernel?.libraryCategorizationParseEnvelope(rawContent: rawContent),
              let data = envelope.data(using: .utf8),
              let decoded = try? categorizationDecoder.decode(RustCategorizationParse.self, from: data)
        else {
            throw CategorizationError.invalidResponse
        }
        if let error = decoded.error {
            throw mapRustError(error)
        }
        let categories = decoded.categories ?? []
        guard !categories.isEmpty else {
            throw CategorizationError.invalidResponse
        }
        return categories
    }

    private static func mapRustError(_ error: String) -> CategorizationError {
        switch error {
        case "no_subscriptions":
            return .noSubscriptions
        case "no_model_selected":
            return .noModelSelected
        default:
            return .invalidResponse
        }
    }

}

private struct CategorizationPromptPlan {
    let model: String
    let systemPrompt: String
    let userPrompt: String
}

private struct RustCategorizationPrompt: Decodable {
    let error: String?
    let model: String?
    let systemPrompt: String?
    let userPrompt: String?
}

private struct RustCategorizationParse: Decodable {
    let error: String?
    let categories: [PodcastCategory]?
}

private let categorizationDecoder: JSONDecoder = {
    let decoder = JSONDecoder()
    decoder.keyDecodingStrategy = .convertFromSnakeCase
    decoder.dateDecodingStrategy = .iso8601
    return decoder
}()
