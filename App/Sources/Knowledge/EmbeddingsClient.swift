import Foundation

/// Anything that can turn texts into vectors. The protocol stays minimal so
/// `VectorIndex` can be tested with a synthetic embedder if we ever wire up
/// integration tests, and so we can swap providers without touching the
/// vector store.
protocol EmbeddingsClient: Sendable {
    /// Embed `texts` in input order. The returned array has the same length
    /// as the input; each vector is `dimensions` floats long.
    func embed(_ texts: [String]) async throws -> [[Float]]
}

// MARK: - OpenRouter compatibility wrapper

/// OpenRouter embeddings routed through Rust shared provider transport.
struct OpenRouterEmbeddingsClient: EmbeddingsClient {
    static let defaultModel = "openai/text-embedding-3-large"
    static let defaultDimensions = 1024
    static let maxBatchSize = 100

    private let model: String
    private let dimensions: Int

    init(
        apiKeyProvider: @Sendable @escaping () throws -> String? = { try OpenRouterCredentialStore.apiKey() },
        model: String = OpenRouterEmbeddingsClient.defaultModel,
        dimensions: Int = OpenRouterEmbeddingsClient.defaultDimensions,
        session: URLSession = .shared,
        timeout: TimeInterval = 30
    ) {
        _ = apiKeyProvider
        _ = session
        _ = timeout
        self.model = model
        self.dimensions = dimensions
    }

    func embed(_ texts: [String]) async throws -> [[Float]] {
        try await RustProviderEmbeddingsClient(
            provider: .openRouter,
            model: model,
            dimensions: dimensions,
            expectedDimensions: dimensions,
            feature: CostFeature.embeddingsOpenRouter,
            maxBatchSize: Self.maxBatchSize
        )
        .embed(texts)
    }
}

// MARK: - Errors

enum EmbeddingsError: LocalizedError {
    case missingAPIKey
    case unauthorized
    case rateLimited
    case serverError(statusCode: Int)
    case transport(detail: String)
    case decoding
    case shapeMismatch(expected: Int, got: Int)
    case providerMissingAPIKey(provider: String)
    case providerUnauthorized(provider: String)
    case providerRateLimited(provider: String)
    case providerServerError(provider: String, statusCode: Int)
    case providerTransport(provider: String, detail: String)
    case providerDecoding(provider: String)
    case dimensionMismatch(provider: String, expected: Int, got: Int)

    var errorDescription: String? {
        switch self {
        case .missingAPIKey:
            return "OpenRouter API key not configured. Add it in Settings → Intelligence → Providers."
        case .unauthorized:
            return "OpenRouter rejected the API key."
        case .rateLimited:
            return "OpenRouter is rate-limiting embedding requests. Try again shortly."
        case let .serverError(code):
            return "OpenRouter embeddings returned HTTP \(code)."
        case let .transport(detail):
            return "Network error contacting OpenRouter: \(detail)."
        case .decoding:
            return "Could not decode the OpenRouter embeddings response."
        case let .shapeMismatch(expected, got):
            return "Embeddings batch shape mismatch: expected \(expected), got \(got)."
        case let .providerMissingAPIKey(provider):
            return "\(provider) API key not configured. Add it in Settings -> AI."
        case let .providerUnauthorized(provider):
            return "\(provider) rejected the API key."
        case let .providerRateLimited(provider):
            return "\(provider) is rate-limiting embedding requests. Try again shortly."
        case let .providerServerError(provider, code):
            return "\(provider) embeddings returned HTTP \(code)."
        case let .providerTransport(provider, detail):
            return "Network error contacting \(provider): \(detail)."
        case let .providerDecoding(provider):
            return "Could not decode the \(provider) embeddings response."
        case let .dimensionMismatch(provider, expected, got):
            return "\(provider) embedding dimension mismatch: expected \(expected), got \(got). Choose a \(expected)-dimension embedding model or rebuild the vector index."
        }
    }
}

// MARK: - Array batching

extension Array {
    /// Slice the array into chunks of at most `size` elements. Last chunk
    /// may be shorter. Used to honour OpenRouter's per-request input cap.
    func batched(by size: Int) -> [[Element]] {
        guard size > 0 else { return [self] }
        var out: [[Element]] = []
        out.reserveCapacity((count + size - 1) / size)
        var i = 0
        while i < count {
            let end = Swift.min(i + size, count)
            out.append(Array(self[i..<end]))
            i = end
        }
        return out
    }
}
