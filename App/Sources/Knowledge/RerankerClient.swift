import Foundation

// Lane 6 — RAG: provider-backed Cohere `rerank-v3.5` passthrough.
//
// Provider HTTP is Rust-owned. Swift keeps only a small async facade over the
// `nmp_app_podcast_rerank` FFI call and maps the returned error envelope to the
// app's existing `RerankerError` cases.
//
// Used as the optional final stage of `RAGSearch`: take the top-K hybrid
// results, rerank, take the top-N. Skipped under "rapid voice" latency
// budgets — per the spec, hybrid RRF alone is shippable quality.

/// Anything that can re-order candidate documents by relevance to a query.
protocol RerankerClient: Sendable {
    /// Given a `query` and a list of `documents`, return the indices of
    /// `documents` reordered from most-relevant to least-relevant. The
    /// returned array contains a permutation of `0..<documents.count`,
    /// possibly truncated to `topN` if specified.
    func rerank(query: String, documents: [String], topN: Int?) async throws -> [Int]
}

// MARK: - Settings-aware wrapper

struct SettingsAwareRerankerClient<Base: RerankerClient>: RerankerClient {
    private let base: Base
    private let isEnabled: @Sendable () async -> Bool

    init(
        base: Base,
        isEnabled: @Sendable @escaping () async -> Bool
    ) {
        self.base = base
        self.isEnabled = isEnabled
    }

    func rerank(
        query: String,
        documents: [String],
        topN: Int?
    ) async throws -> [Int] {
        guard await isEnabled() else {
            let limit = min(topN ?? documents.count, documents.count)
            return Array(0..<limit)
        }
        return try await base.rerank(query: query, documents: documents, topN: topN)
    }
}

// MARK: - OpenRouter implementation

struct OpenRouterRerankerClient: RerankerClient {
    static let defaultModel = "cohere/rerank-v3.5"

    private let model: String

    init(
        model: String = OpenRouterRerankerClient.defaultModel
    ) {
        self.model = model
    }

    func rerank(
        query: String,
        documents: [String],
        topN: Int? = nil
    ) async throws -> [Int] {
        guard !documents.isEmpty else { return [] }

        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw RerankerError.transport(detail: "kernel handle unavailable")
        }

        let request = RerankFFIRequest(
            model: model,
            query: query,
            documents: documents,
            top_n: topN
        )
        let requestData = try JSONEncoder().encode(request)
        guard let requestJSON = String(data: requestData, encoding: .utf8) else {
            throw RerankerError.decoding
        }

        let responseJSON: String = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":{"kind":"transport","message":"null kernel handle"}}"#
            }
            return requestJSON.withCString { requestPtr in
                guard let ptr = nmp_app_podcast_rerank(handle, requestPtr) else {
                    return #"{"error":{"kind":"transport","message":"null response from Rust"}}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        guard let responseData = responseJSON.data(using: .utf8) else {
            throw RerankerError.decoding
        }
        let envelope = try JSONDecoder().decode(RerankFFIResponse.self, from: responseData)
        if let error = envelope.error {
            throw RerankerError(rustError: error)
        }
        guard let indices = envelope.indices else {
            throw RerankerError.decoding
        }
        return indices
    }

    private struct RerankFFIRequest: Encodable {
        let model: String
        let query: String
        let documents: [String]
        let top_n: Int?
    }
}

// MARK: - Errors

enum RerankerError: LocalizedError {
    case missingAPIKey
    case unauthorized
    case rateLimited
    case serverError(statusCode: Int)
    case transport(detail: String)
    case decoding
    case invalidRequest(detail: String)

    var errorDescription: String? {
        switch self {
        case .missingAPIKey:
            return "OpenRouter API key not configured. Add it in Settings → Intelligence → Providers."
        case .unauthorized:
            return "OpenRouter rejected the API key."
        case .rateLimited:
            return "OpenRouter is rate-limiting rerank requests. Try again shortly."
        case let .serverError(code):
            return "OpenRouter rerank returned HTTP \(code)."
        case let .transport(detail):
            return "Network error contacting OpenRouter: \(detail)."
        case .decoding:
            return "Could not decode the OpenRouter rerank response."
        case let .invalidRequest(detail):
            return "Invalid rerank request: \(detail)."
        }
    }
}

private struct RerankFFIResponse: Decodable {
    let indices: [Int]?
    let error: RerankFFIError?
}

private struct RerankFFIError: Decodable {
    let kind: String
    let message: String
    let statusCode: Int?

    private enum CodingKeys: String, CodingKey {
        case kind
        case message
        case statusCode = "status_code"
    }
}

private extension RerankerError {
    init(rustError: RerankFFIError) {
        switch rustError.kind {
        case "missing_api_key":
            self = .missingAPIKey
        case "unauthorized":
            self = .unauthorized
        case "rate_limited":
            self = .rateLimited
        case "server_error":
            self = .serverError(statusCode: rustError.statusCode ?? -1)
        case "decoding":
            self = .decoding
        case "invalid_request":
            self = .invalidRequest(detail: rustError.message)
        default:
            self = .transport(detail: rustError.message)
        }
    }
}
