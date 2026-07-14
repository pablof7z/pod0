import Foundation
import os.log

// Lane 6 — RAG: OpenRouter embeddings client.
//
// Calls `POST https://openrouter.ai/api/v1/embeddings` (OpenAI-compatible
// schema) with model `openai/text-embedding-3-large` at 1024 dimensions
// (Matryoshka truncation — see `docs/spec/research/embeddings-rag-stack.md`).
// Authentication uses the existing `OpenRouterCredentialStore`.
//
// Batching: OpenAI's hard limit is 2048 inputs per request, but OpenRouter
// docs and downstream providers vary; we cap at 100 to stay well clear of
// payload-size and provider-specific limits, and to keep latency predictable.

/// Anything that can turn texts into vectors. The protocol stays minimal so
/// `VectorIndex` can be tested with a synthetic embedder if we ever wire up
/// integration tests, and so we can swap providers without touching the
/// vector store.
protocol EmbeddingsClient: Sendable {
    /// Embed `texts` in input order. The returned array has the same length
    /// as the input; each vector is `dimensions` floats long.
    func embed(_ texts: [String]) async throws -> [[Float]]
}

// MARK: - OpenRouter implementation

struct OpenRouterEmbeddingsClient: EmbeddingsClient {
    static let defaultModel = "openai/text-embedding-3-large"
    static let defaultDimensions = 1024
    static let maxBatchSize = 100

    private static let logger = Logger.app("OpenRouterEmbeddingsClient")
    private static let endpoint = URL(string: "https://openrouter.ai/api/v1/embeddings")!
    private static let xTitle = "Podcastr"

    /// Shared encoder/decoder. Each batch in `embedBatch` was minting
    /// one encoder + two decoders, and a full transcript ingest spends
    /// dozens of batches — that's a lot of Foundation allocator
    /// pressure on an already-network-bound path. Both types are
    /// reentrant for `encode` / `decode` after construction.
    nonisolated(unsafe) private static let encoder = JSONEncoder()
    nonisolated(unsafe) private static let decoder = JSONDecoder()

    private let apiKeyProvider: @Sendable () throws -> String?
    private let model: String
    private let dimensions: Int
    private let session: URLSession
    private let timeout: TimeInterval

    init(
        apiKeyProvider: @Sendable @escaping () throws -> String? = { try OpenRouterCredentialStore.apiKey() },
        model: String = OpenRouterEmbeddingsClient.defaultModel,
        dimensions: Int = OpenRouterEmbeddingsClient.defaultDimensions,
        session: URLSession = .shared,
        timeout: TimeInterval = 30
    ) {
        self.apiKeyProvider = apiKeyProvider
        self.model = model
        self.dimensions = dimensions
        self.session = session
        self.timeout = timeout
    }

    func embed(_ texts: [String]) async throws -> [[Float]] {
        guard !texts.isEmpty else { return [] }
        guard let apiKey = try apiKeyProvider(), !apiKey.isEmpty else {
            throw EmbeddingsError.missingAPIKey
        }

        // Slice into batches and reassemble in input order. Sequential
        // execution keeps this simple and avoids saturating the rate-limit
        // budget; concurrent batches can be added later if ingest latency
        // proves to be a bottleneck.
        var output: [[Float]] = []
        output.reserveCapacity(texts.count)
        for batch in texts.batched(by: Self.maxBatchSize) {
            let vectors = try await embedBatch(batch, apiKey: apiKey)
            guard vectors.count == batch.count else {
                throw EmbeddingsError.shapeMismatch(
                    expected: batch.count, got: vectors.count)
            }
            output.append(contentsOf: vectors)
        }
        return output
    }

    // MARK: - Single batch

    private func embedBatch(_ batch: [String], apiKey: String) async throws -> [[Float]] {
        var req = URLRequest(url: Self.endpoint, timeoutInterval: timeout)
        req.httpMethod = "POST"
        req.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue(Self.xTitle, forHTTPHeaderField: "X-Title")

        let payload = RequestPayload(
            model: model,
            input: batch,
            dimensions: dimensions
        )
        let bodyData = try Self.encoder.encode(payload)
        req.httpBody = bodyData
        let requestPayloadJSON = String(data: bodyData, encoding: .utf8)

        let start = Date()
        let (data, response) = try await session.data(for: req)
        let latencyMs = Int(Date().timeIntervalSince(start) * 1000)

        guard let http = response as? HTTPURLResponse else {
            throw EmbeddingsError.transport(detail: "no HTTPURLResponse")
        }
        switch http.statusCode {
        case 200..<300:
            break
        case 401, 403:
            throw EmbeddingsError.unauthorized
        case 429:
            throw EmbeddingsError.rateLimited
        default:
            let body = String(data: data, encoding: .utf8) ?? "<binary>"
            Self.logger.warning("OpenRouter embeddings HTTP \(http.statusCode, privacy: .public): \(body, privacy: .public)")
            throw EmbeddingsError.serverError(statusCode: http.statusCode)
        }

        let decoded: ResponsePayload
        do {
            decoded = try Self.decoder.decode(ResponsePayload.self, from: data)
        } catch {
            Self.logger.error("OpenRouter embeddings decode failed: \(error, privacy: .public)")
            throw EmbeddingsError.decoding
        }

        if let usage = decoded.usage {
            // `decoded` already carries `model` + `usage` via the typed
            // `ResponsePayload` — the previous shape re-parsed `data`
            // through `JSONSerialization` to fish them out, which
            // allocated a `[String: Any]` dictionary plus a follow-up
            // `JSONSerialization.data` + `JSONDecoder` round-trip per
            // batch. One typed parse is enough.
            let modelUsed = decoded.model ?? model
            let preview = "embed: \(batch.count) input(s)"
            Task { @MainActor in
                CostLedger.shared.log(
                    feature: CostFeature.embeddingsOpenRouter,
                    model: modelUsed,
                    usage: usage,
                    latencyMs: latencyMs,
                    requestPayloadJSON: requestPayloadJSON,
                    responseContentPreview: preview
                )
            }
        }

        // Provider-side ordering is guaranteed by OpenAI-compatible schema
        // via the `index` field, but always defensively re-sort.
        let ordered = decoded.data.sorted { $0.index < $1.index }
        return ordered.map(\.embedding)
    }

    // MARK: - DTOs

    private struct RequestPayload: Encodable {
        let model: String
        let input: [String]
        let dimensions: Int
    }

    private struct ResponsePayload: Decodable {
        let data: [Item]
        let model: String?
        let usage: OpenRouterUsagePayload?
        struct Item: Decodable {
            let index: Int
            let embedding: [Float]
        }
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
