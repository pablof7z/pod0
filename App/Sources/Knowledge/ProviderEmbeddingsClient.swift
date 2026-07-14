import Foundation

final class ProviderEmbeddingsClient: EmbeddingsClient, @unchecked Sendable {
    @MainActor private weak var appStore: AppStateStore?

    @MainActor
    func attach(appStore: AppStateStore) {
        self.appStore = appStore
    }

    func embed(_ texts: [String]) async throws -> [[Float]] {
        let settings = await MainActor.run {
            appStore?.state.settings ?? Settings()
        }
        let reference = LLMModelReference(storedID: settings.embeddingsModel)
        switch reference.provider {
        case .openRouter:
            return try await OpenRouterEmbeddingsClient(
                model: reference.modelID,
                dimensions: Settings.embeddingsDimensions
            )
            .embed(texts)
        case .ollama:
            return try await OllamaEmbeddingsClient(model: reference.modelID)
                .embed(texts)
        }
    }
}
