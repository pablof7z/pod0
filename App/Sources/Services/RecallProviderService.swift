import Foundation
import os.log

/// Native network-provider container for Rust-owned recall workflows.
/// It owns no durable index, evidence state, ranking, retries, or fallback.
@MainActor
final class RecallProviderService {
    static let shared = RecallProviderService()

    nonisolated private static let logger = Logger.app("RecallProviderService")

    let embedder: any EmbeddingsClient

    private let providerEmbedder: ProviderEmbeddingsClient

    func attach(appStore: AppStateStore) {
        providerEmbedder.attach(appStore: appStore)
    }

    private init() {
        let embedder = ProviderEmbeddingsClient()
        self.embedder = embedder
        self.providerEmbedder = embedder
        if !OpenRouterCredentialStore.hasAPIKey() && !OllamaCredentialStore.hasAPIKey() {
            Self.logger.warning("No recall embedding provider is configured")
        }
    }
}
