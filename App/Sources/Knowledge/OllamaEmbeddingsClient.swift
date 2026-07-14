import Foundation
import os.log

struct OllamaEmbeddingsClient: EmbeddingsClient {
    static let maxBatchSize = 100

    private static let logger = Logger.app("OllamaEmbeddingsClient")
    private static let endpoint = URL(string: "https://ollama.com/api/embed")!

    private let apiKeyProvider: @Sendable () throws -> String?
    private let model: String
    private let expectedDimensions: Int
    private let session: URLSession
    private let timeout: TimeInterval

    init(
        apiKeyProvider: @Sendable @escaping () throws -> String? = { try OllamaCredentialStore.apiKey() },
        model: String,
        expectedDimensions: Int = Settings.embeddingsDimensions,
        session: URLSession = .shared,
        timeout: TimeInterval = 30
    ) {
        self.apiKeyProvider = apiKeyProvider
        self.model = model
        self.expectedDimensions = expectedDimensions
        self.session = session
        self.timeout = timeout
    }

    func embed(_ texts: [String]) async throws -> [[Float]] {
        guard !texts.isEmpty else { return [] }
        guard let apiKey = try apiKeyProvider(), !apiKey.isEmpty else {
            throw EmbeddingsError.providerMissingAPIKey(provider: LLMProvider.ollama.displayName)
        }

        var output: [[Float]] = []
        output.reserveCapacity(texts.count)
        for batch in texts.batched(by: Self.maxBatchSize) {
            let vectors = try await embedBatch(batch, apiKey: apiKey)
            guard vectors.count == batch.count else {
                throw EmbeddingsError.shapeMismatch(expected: batch.count, got: vectors.count)
            }
            for vector in vectors where vector.count != expectedDimensions {
                throw EmbeddingsError.dimensionMismatch(
                    provider: LLMProvider.ollama.displayName,
                    expected: expectedDimensions,
                    got: vector.count
                )
            }
            output.append(contentsOf: vectors)
        }
        return output
    }

    private func embedBatch(_ batch: [String], apiKey: String) async throws -> [[Float]] {
        var req = URLRequest(url: Self.endpoint, timeoutInterval: timeout)
        req.httpMethod = "POST"
        req.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let bodyData = try JSONEncoder().encode(RequestPayload(model: model, input: batch))
        req.httpBody = bodyData
        let requestPayloadJSON = String(data: bodyData, encoding: .utf8)

        let start = Date()
        let (data, response) = try await session.data(for: req)
        let latencyMs = Int(Date().timeIntervalSince(start) * 1000)

        guard let http = response as? HTTPURLResponse else {
            throw EmbeddingsError.providerTransport(provider: LLMProvider.ollama.displayName, detail: "no HTTPURLResponse")
        }
        switch http.statusCode {
        case 200..<300:
            break
        case 401, 403:
            throw EmbeddingsError.providerUnauthorized(provider: LLMProvider.ollama.displayName)
        case 429:
            throw EmbeddingsError.providerRateLimited(provider: LLMProvider.ollama.displayName)
        default:
            let body = String(data: data, encoding: .utf8) ?? "<binary>"
            Self.logger.warning("Ollama embeddings HTTP \(http.statusCode, privacy: .public): \(body, privacy: .public)")
            throw EmbeddingsError.providerServerError(provider: LLMProvider.ollama.displayName, statusCode: http.statusCode)
        }

        let embeddings: [[Float]]
        do {
            embeddings = try JSONDecoder().decode(ResponsePayload.self, from: data).embeddings
        } catch {
            Self.logger.error("Ollama embeddings decode failed: \(error, privacy: .public)")
            throw EmbeddingsError.providerDecoding(provider: LLMProvider.ollama.displayName)
        }

        let promptTokens = (try? JSONSerialization.jsonObject(with: data) as? [String: Any])
            .flatMap { $0["prompt_eval_count"] as? Int } ?? 0
        let preview = "embed: \(batch.count) input(s)"
        Task { @MainActor in
            CostLedger.shared.logOllama(
                feature: CostFeature.embeddingsOllama,
                model: model,
                promptTokens: promptTokens,
                completionTokens: 0,
                latencyMs: latencyMs,
                requestPayloadJSON: requestPayloadJSON,
                responseContentPreview: preview
            )
        }

        return embeddings
    }

    private struct RequestPayload: Encodable {
        let model: String
        let input: [String]
    }

    private struct ResponsePayload: Decodable {
        let embeddings: [[Float]]
    }
}
