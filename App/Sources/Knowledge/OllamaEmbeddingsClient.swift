import Foundation

/// Ollama embeddings routed through Rust shared provider transport.
struct OllamaEmbeddingsClient: EmbeddingsClient {
    static let maxBatchSize = 100

    private let model: String
    private let expectedDimensions: Int

    init(
        apiKeyProvider: @Sendable @escaping () throws -> String? = { try OllamaCredentialStore.apiKey() },
        model: String,
        expectedDimensions: Int = Settings.embeddingsDimensions,
        session: URLSession = .shared,
        timeout: TimeInterval = 30
    ) {
        _ = apiKeyProvider
        _ = session
        _ = timeout
        self.model = model
        self.expectedDimensions = expectedDimensions
    }

    func embed(_ texts: [String]) async throws -> [[Float]] {
        try await RustProviderEmbeddingsClient(
            provider: .ollama,
            model: model,
            dimensions: nil,
            expectedDimensions: expectedDimensions,
            feature: CostFeature.embeddingsOllama,
            maxBatchSize: Self.maxBatchSize
        )
        .embed(texts)
    }
}
