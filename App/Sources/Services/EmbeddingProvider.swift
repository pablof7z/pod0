import Foundation

// MARK: - EmbeddingProvider
//
// On-device sentence-embedding abstraction (issue #236). This is intentionally
// distinct from `EmbeddingsClient` (Knowledge/EmbeddingsClient.swift), which is
// the network-provider-routed, batch-text-in / vectors-out seam the vector
// index already consumes:
//
//   * `EmbeddingsClient` answers "give me vectors for these texts" and is wired
//     to OpenRouter / Ollama via the Rust provider transport.
//   * `EmbeddingProvider` answers "I am a concrete local model — here is my
//     fixed dimensionality and whether my weights are downloaded yet."
//
// Keeping them separate lets the on-device Core ML path advertise readiness and
// dimensionality (which the cloud clients don't need) without bloating the
// cloud client protocol. `CoreMLEmbeddingProvider` conforms to *both*: it is an
// `EmbeddingProvider` (the local-model identity) and adapts to `EmbeddingsClient`
// (so it can drop into the existing RAG seam) — see `LocalEmbeddingsClient`.
//
// The platform guard lives at the concrete type, not the protocol: the protocol
// is plain Swift so call sites and the Android target can reference it; only the
// Core ML implementation is `#if os(iOS)`.

/// A concrete, identifiable embedding model that runs locally on device.
protocol EmbeddingProvider: Sendable {
    /// Fixed output dimensionality of this model's sentence vectors.
    /// `all-MiniLM-L6-v2` is 384.
    var dimensions: Int { get }

    /// Whether the model weights are present on disk and inference can run.
    /// Mirrors the "not yet ready" ladder used for transcripts / Gemma: callers
    /// must check this before relying on `embed` rather than catching a throw on
    /// the hot path.
    var isReady: Bool { get }

    /// Embed `texts` in input order. Each returned vector has `dimensions`
    /// elements. Throws `EmbeddingProviderError.modelNotReady` when `isReady` is
    /// false rather than silently returning empty vectors.
    func embed(_ texts: [String]) async throws -> [[Float]]
}

// MARK: - Errors

enum EmbeddingProviderError: LocalizedError {
    /// Weights are not downloaded yet. Callers should fall back (e.g. to a cloud
    /// provider) or surface a "downloading" state.
    case modelNotReady
    /// The bundled tokenizer vocab could not be loaded.
    case tokenizerUnavailable(underlying: String)
    /// The Core ML model could not be loaded from disk.
    case modelLoadFailed(underlying: String)
    /// Inference produced an output whose shape we couldn't interpret.
    case inferenceFailed(detail: String)
    /// Inference ran but produced a vector whose length isn't `dimensions`.
    case dimensionMismatch(expected: Int, got: Int)

    var errorDescription: String? {
        switch self {
        case .modelNotReady:
            return "On-device embedding model is not downloaded yet."
        case let .tokenizerUnavailable(underlying):
            return "On-device embedding tokenizer unavailable: \(underlying)"
        case let .modelLoadFailed(underlying):
            return "Failed to load on-device embedding model: \(underlying)"
        case let .inferenceFailed(detail):
            return "On-device embedding inference failed: \(detail)"
        case let .dimensionMismatch(expected, got):
            return "On-device embedding dimension mismatch: expected \(expected), got \(got)."
        }
    }
}

// MARK: - MiniLM constants

/// Shared constants for the `all-MiniLM-L6-v2` on-device model. Centralized so
/// the provider, the download catalog id, and any future migration code agree on
/// one source of truth.
enum MiniLMEmbedding {
    /// Sentence-embedding dimensionality of `all-MiniLM-L6-v2`.
    static let dimensions = 384

    /// Catalog id used for the post-install model download. Matches the
    /// `LocalModelSpec.id` a future catalog entry would carry, and the on-disk
    /// filename via `DownloadCapability.localModelFileURL(for:)`.
    static let modelID = "all-minilm-l6-v2"

    /// Token sequence length the model was traced at. MiniLM is trained at 128.
    static let maxSequenceLength = 128
}
