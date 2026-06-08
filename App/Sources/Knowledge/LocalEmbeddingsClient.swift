import Foundation
import os.log

// MARK: - LocalEmbeddingsClient
//
// Adapts an on-device `EmbeddingProvider` (Core ML MiniLM, issue #236) to the
// `EmbeddingsClient` seam the vector index already consumes, with a cloud
// fallback while the local model is downloading.
//
// Dimension safety (the load-bearing decision):
//   The live `VectorIndex` is opened at a FIXED dimensionality
//   (`Settings.embeddingsDimensions`, currently 1024 for text-embedding-3-large).
//   MiniLM emits 384-dim vectors. Writing 384-dim vectors into a 1024-dim index
//   corrupts cosine search and trips `VectorStoreError.dimensionMismatch`.
//   So this client routes to the local provider ONLY when the active index
//   dimensionality equals the provider's dimensionality; otherwise it falls
//   back to the cloud client. Flipping the index to 384-dim (and re-embedding
//   the corpus) is a deliberate migration tracked in BACKLOG, not something to
//   trigger implicitly by swapping the embedder.
//
// Fallback ladder (per call, per the issue's "keep OpenRouter as cloud fallback
// while the local model is downloading"):
//   1. local provider, IFF ready AND dimensions match the index  → on-device
//   2. otherwise                                                  → cloud client
//
// This type is `@unchecked Sendable` for the same reason as
// `ProviderEmbeddingsClient`: it holds immutable references to `Sendable`
// collaborators; the annotation documents that the shared state is read-only.

final class LocalEmbeddingsClient: EmbeddingsClient, @unchecked Sendable {

    private static let logger = Logger.app("LocalEmbeddingsClient")

    private let provider: EmbeddingProvider
    private let cloud: EmbeddingsClient
    /// Dimensionality the active vector index was opened at. The local provider
    /// is used only when it matches `provider.dimensions`.
    private let indexDimensions: Int

    init(
        provider: EmbeddingProvider,
        cloud: EmbeddingsClient,
        indexDimensions: Int = Settings.embeddingsDimensions
    ) {
        self.provider = provider
        self.cloud = cloud
        self.indexDimensions = indexDimensions
    }

    /// Whether the on-device path will actually be taken for the active index.
    /// Surfaced for UI readiness ("on-device embeddings active" vs "using cloud").
    var prefersLocal: Bool {
        provider.isReady && provider.dimensions == indexDimensions
    }

    func embed(_ texts: [String]) async throws -> [[Float]] {
        guard !texts.isEmpty else { return [] }

        if provider.dimensions != indexDimensions {
            // Local model exists but its dimensionality doesn't match the index.
            // Using it would corrupt search — stay on the cloud client. (No log
            // spam: this is the steady state until the index migrates to 384.)
            return try await cloud.embed(texts)
        }

        guard provider.isReady else {
            // Model still downloading / not installed → cloud fallback.
            return try await cloud.embed(texts)
        }

        do {
            return try await provider.embed(texts)
        } catch EmbeddingProviderError.modelNotReady {
            return try await cloud.embed(texts)
        } catch {
            // Inference failure: degrade to cloud rather than failing the whole
            // ingest. Logged so a persistent on-device failure is visible.
            Self.logger.error(
                "on-device embed failed (\(error.localizedDescription, privacy: .public)) — falling back to cloud"
            )
            return try await cloud.embed(texts)
        }
    }
}
