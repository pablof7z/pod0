import Foundation
import os.log

// MARK: - RAGService
//
// Singleton entry point for the live RAG stack: on-device sqlite-vec
// `VectorIndex`, provider-selected embeddings client, settings-gated reranker,
// and the `RAGSearch` orchestrator that wires them together.
//
// Why a `@MainActor` singleton:
//   - The wiki and briefing surfaces want a stable, ready-on-launch handle.
//   - State the rest of the app cares about (where the SQLite file lives,
//     when the index was opened) is UI-adjacent.
//   - The underlying `VectorIndex` is itself an `actor`, so DB work stays
//     off the main thread; this class just hands out references.
//
// Lazy lookup of `AppStateStore` is intentionally NOT done here — the
// briefing adapter needs episode/subscription metadata, so the bridge
// is set up via `attach(appStore:)` once `AppStateStore` finishes init.

@MainActor
final class RAGService {

    // MARK: Singleton

    static let shared = RAGService()

    // MARK: Logger

    nonisolated private static let logger = Logger.app("RAGService")

    // MARK: Public surface

    /// On-disk vector + FTS5 store. Opens at first method call (schema is
    /// created lazily inside the actor).
    let index: VectorIndex
    /// Provider-selected embeddings client. Defaults to OpenRouter, but can
    /// route to Ollama Cloud when the user chooses an Ollama embedding model.
    let embedder: any EmbeddingsClient
    /// Settings-aware reranker. When `settings.rerankerEnabled` is off, it
    /// preserves retrieval order without making an OpenRouter network call.
    let reranker: any RerankerClient
    /// End-to-end retrieval pipeline (embed → hybrid → rerank).
    let search: RAGSearch

    /// Resolved on-disk URL for the SQLite file. Surfaced for logging /
    /// debug commands.
    let storeURL: URL?

    // MARK: AppStateStore weak ref (briefing adapter needs episode lookup)

    /// Late-bound back-reference to the application's main state store.
    /// Set via `attach(appStore:)` from `AppStateStore.init` after the
    /// singleton is constructed; held weakly so we never extend the store's
    /// lifetime.
    private(set) weak var appStore: AppStateStore?
    private let providerEmbedder: ProviderEmbeddingsClient

    /// Wire the live `AppStateStore` so the briefing adapter can resolve
    /// episode + subscription metadata at retrieval time. Idempotent.
    func attach(appStore: AppStateStore) {
        self.appStore = appStore
        providerEmbedder.attach(appStore: appStore)
    }

    // MARK: Adapters (defined in RAGService+Adapters.swift)

    /// Adapter that conforms to `WikiRAGSearchProtocol` so `WikiGenerator`
    /// and `WikiVerifier` can take it directly.
    var wikiRAG: any WikiRAGSearchProtocol {
        WikiRAGSearchAdapter(search: search, index: index)
    }

    /// Adapter that conforms to `BriefingRAGSearchProtocol`.
    var briefingRAG: any BriefingRAGSearchProtocol {
        BriefingRAGSearchAdapter(service: self)
    }

    // MARK: Init

    private init() {
        // Resolve the on-disk URL up front so we can log it. Falling back to
        // an in-memory store on directory-creation failure keeps the app
        // functional even if Application Support is sandboxed off.
        let resolvedURL: URL?
        let openedIndex: VectorIndex
        let embedder = ProviderEmbeddingsClient()
        let reranker = SettingsAwareRerankerClient(
            base: OpenRouterRerankerClient(),
            isEnabled: {
                await MainActor.run {
                    RAGService.shared.appStore?.state.settings.rerankerEnabled ?? false
                }
            }
        )

        do {
            let url = try VectorIndex.defaultStoreURL()
            openedIndex = try VectorIndex(embedder: embedder, fileURL: url)
            resolvedURL = url
            Self.logger.info(
                "opened vectors.sqlite at \(url.path, privacy: .public)"
            )
        } catch {
            Self.logger.error(
                "failed to open on-disk vectors.sqlite (\(String(describing: error), privacy: .public)) — falling back to in-memory store"
            )
            // The in-memory `VectorIndex` keeps the app runnable; data won't
            // survive a relaunch but the wiki/briefing code paths still work.
            do {
                openedIndex = try VectorIndex(embedder: embedder, inMemory: true)
                resolvedURL = nil
            } catch {
                // SQLiteVec.initialize() failure is fatal — the build is
                // misconfigured. Crash with a clear message rather than
                // silently degrading to a no-op.
                fatalError("RAGService: VectorIndex init failed even in-memory: \(error)")
            }
        }

        // Probe provider keys once so devs see a clear "no key" log line
        // instead of waiting for the first failed embed.
        if !OpenRouterCredentialStore.hasAPIKey() && !OllamaCredentialStore.hasAPIKey() {
            Self.logger.warning(
                "No LLM provider key configured — RAG queries will return [] until the user adds one."
            )
        }

        self.index = openedIndex
        self.embedder = embedder
        self.providerEmbedder = embedder
        self.reranker = reranker
        self.storeURL = resolvedURL
        self.search = RAGSearch(
            store: openedIndex,
            embedder: embedder,
            reranker: reranker
        )
    }
}
