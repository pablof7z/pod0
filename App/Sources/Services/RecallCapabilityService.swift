import Foundation
import os.log

/// Native capability container for the shared Rust recall workflow.
///
/// The SQLite vector/FTS rows are reconstructible from the Rust-selected
/// evidence projection. This object owns no evidence identity, selection,
/// ranking, fallback, citation, or result-state decisions.
@MainActor
final class RecallCapabilityService {
    static let shared = RecallCapabilityService()

    nonisolated private static let logger = Logger.app("RecallCapabilityService")

    let index: VectorIndex
    let embedder: any EmbeddingsClient
    let storeURL: URL?

    private(set) weak var appStore: AppStateStore?
    private let providerEmbedder: ProviderEmbeddingsClient

    func attach(appStore: AppStateStore) {
        self.appStore = appStore
        providerEmbedder.attach(appStore: appStore)
    }

    private init() {
        let resolvedURL: URL?
        let openedIndex: VectorIndex
        let embedder = ProviderEmbeddingsClient()
        do {
            let url = try VectorIndex.defaultStoreURL()
            openedIndex = try VectorIndex(embedder: embedder, fileURL: url)
            resolvedURL = url
            Self.logger.info(
                "Opened reconstructible recall index at \(url.path, privacy: .public)"
            )
        } catch {
            Self.logger.error(
                "Failed to open the recall capability index; using an in-memory rebuild"
            )
            do {
                openedIndex = try VectorIndex(embedder: embedder, inMemory: true)
                resolvedURL = nil
            } catch {
                fatalError("Recall capability index initialization failed: \(error)")
            }
        }

        if !OpenRouterCredentialStore.hasAPIKey() && !OllamaCredentialStore.hasAPIKey() {
            Self.logger.warning(
                "No recall embedding provider is configured"
            )
        }

        self.index = openedIndex
        self.embedder = embedder
        self.providerEmbedder = embedder
        self.storeURL = resolvedURL
    }
}
