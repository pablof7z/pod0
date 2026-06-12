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
            return try await RustProviderEmbeddingsClient(
                provider: .openRouter,
                model: reference.modelID,
                dimensions: Settings.embeddingsDimensions,
                expectedDimensions: Settings.embeddingsDimensions,
                feature: CostFeature.embeddingsOpenRouter
            ).embed(texts)
        case .ollama:
            return try await RustProviderEmbeddingsClient(
                provider: .ollama,
                model: reference.modelID,
                dimensions: nil,
                expectedDimensions: Settings.embeddingsDimensions,
                feature: CostFeature.embeddingsOllama
            ).embed(texts)
        case .local:
            // On-device local models are chat-only; they don't expose an
            // embeddings endpoint. Embeddings must use a cloud provider.
            throw EmbeddingsError.missingAPIKey
        }
    }
}

struct RustProviderEmbeddingsClient: EmbeddingsClient {
    static let maxBatchSize = 100

    private static let encoder = JSONEncoder()
    private static let decoder = JSONDecoder()

    private let provider: LLMProvider
    private let model: String
    private let dimensions: Int?
    private let expectedDimensions: Int?
    private let feature: String
    private let maxBatchSize: Int

    init(
        provider: LLMProvider,
        model: String,
        dimensions: Int?,
        expectedDimensions: Int?,
        feature: String,
        maxBatchSize: Int = RustProviderEmbeddingsClient.maxBatchSize
    ) {
        self.provider = provider
        self.model = model
        self.dimensions = dimensions
        self.expectedDimensions = expectedDimensions
        self.feature = feature
        self.maxBatchSize = maxBatchSize
    }

    func embed(_ texts: [String]) async throws -> [[Float]] {
        guard !texts.isEmpty else { return [] }
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw EmbeddingsError.providerTransport(provider: provider.displayName, detail: "kernel handle unavailable")
        }

        var output: [[Float]] = []
        output.reserveCapacity(texts.count)
        for batch in texts.batched(by: maxBatchSize) {
            let response = try await embedBatch(batch, handleBits: handleBits)
            guard response.embeddings.count == batch.count else {
                throw EmbeddingsError.shapeMismatch(expected: batch.count, got: response.embeddings.count)
            }
            if let expectedDimensions {
                for vector in response.embeddings where vector.count != expectedDimensions {
                    throw EmbeddingsError.dimensionMismatch(
                        provider: provider.displayName,
                        expected: expectedDimensions,
                        got: vector.count
                    )
                }
            }
            log(response: response, batchCount: batch.count)
            output.append(contentsOf: response.embeddings)
        }
        return output
    }

    private func embedBatch(_ batch: [String], handleBits: Int) async throws -> ProviderEmbeddingResult {
        let intent = ProviderEmbeddingIntent(
            provider: provider.rawValue,
            model: model,
            input: batch,
            dimensions: dimensions
        )
        let intentData = try Self.encoder.encode(intent)
        guard let intentString = String(data: intentData, encoding: .utf8) else {
            throw EmbeddingsError.providerDecoding(provider: provider.displayName)
        }
        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"kernel handle unavailable"}"#
            }
            return intentString.withCString { intentPtr in
                guard let ptr = nmp_app_podcast_provider_embed(handle, intentPtr) else {
                    return #"{"error":"null response from Rust"}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        guard let data = responseJSON.data(using: .utf8) else {
            throw EmbeddingsError.providerDecoding(provider: provider.displayName)
        }
        let envelope = try Self.decoder.decode(ProviderEmbeddingEnvelope.self, from: data)
        if let error = envelope.error {
            throw EmbeddingsError.providerTransport(provider: provider.displayName, detail: error)
        }
        guard let result = envelope.result else {
            throw EmbeddingsError.providerDecoding(provider: provider.displayName)
        }
        return result
    }

    private func log(response: ProviderEmbeddingResult, batchCount: Int) {
        let requestPreview = "embed: \(batchCount) input(s)"
        Task { @MainActor in
            switch provider {
            case .openRouter:
                CostLedger.shared.log(
                    feature: feature,
                    model: response.model,
                    usage: response.usage,
                    latencyMs: response.latencyMs,
                    requestPayloadJSON: requestPreview,
                    responseContentPreview: requestPreview
                )
            case .ollama:
                CostLedger.shared.logOllama(
                    feature: feature,
                    model: response.model,
                    promptTokens: response.promptTokens,
                    completionTokens: 0,
                    latencyMs: response.latencyMs,
                    requestPayloadJSON: requestPreview,
                    responseContentPreview: requestPreview
                )
            case .local:
                break
            }
        }
    }

    private struct ProviderEmbeddingIntent: Encodable {
        let provider: String
        let model: String
        let input: [String]
        let dimensions: Int?
    }

    private struct ProviderEmbeddingEnvelope: Decodable {
        let result: ProviderEmbeddingResult?
        let error: String?
    }

    private struct ProviderEmbeddingResult: Decodable {
        let embeddings: [[Float]]
        let provider: String
        let model: String
        let latencyMs: Int
        let usage: OpenRouterUsagePayload?
        let promptTokens: Int

        private enum CodingKeys: String, CodingKey {
            case embeddings, provider, model, usage
            case latencyMs = "latency_ms"
            case promptTokens = "prompt_tokens"
        }
    }
}
