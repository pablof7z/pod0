import Foundation
import os.log

/// Concrete `PerplexityClientProtocol` backed by shared Rust provider transport.
///
/// Swift supplies only the typed search intent and maps the normalized Rust
/// response into the agent's value type. Rust owns Perplexity/OpenRouter
/// provider selection, URLs, auth headers, request bodies, status handling, and
/// response parsing.
public actor PerplexityClient: PerplexityClientProtocol {

    private static let encoder = JSONEncoder()
    private static let decoder = JSONDecoder()
    private let logger = Logger.app("PerplexityClient")

    public init() {}

    // MARK: - PerplexityClientProtocol

    public func search(query: String) async throws -> PerplexityResult {
        try await searchViaRust(query: query)
    }

    // MARK: - Legacy response parser

    static func parseResponse(_ data: Data) throws -> PerplexityResult {
        let payload = try Self.decoder.decode(LegacyPerplexityPayload.self, from: data)
        let answer = payload.choices.first?.message.content ?? ""
        let sources = payload.searchResults?.compactMap { result -> PerplexityResult.Source? in
            guard let url = result.url, !url.isEmpty else {
                return nil
            }
            return PerplexityResult.Source(title: result.title ?? url, url: url)
        } ?? payload.citations.map {
            PerplexityResult.Source(title: $0, url: $0)
        }
        return PerplexityResult(answer: answer, sources: sources)
    }

    // MARK: - Rust transport

    private func searchViaRust(query: String) async throws -> PerplexityResult {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            throw PerplexityClientError.kernelUnavailable
        }

        let requestData = try Self.encoder.encode(PerplexitySearchIntent(query: query))
        guard let requestJSON = String(data: requestData, encoding: .utf8) else {
            throw PerplexityClientError.malformedResponse("Could not encode search request.")
        }

        logger.info("submitting online search through Rust provider transport")
        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":{"kind":"store_unavailable","message":"Kernel handle unavailable"}}"#
            }
            return requestJSON.withCString { requestPtr in
                guard let ptr = nmp_app_podcast_perplexity_search(handle, requestPtr) else {
                    return #"{"error":{"kind":"store_unavailable","message":"null response from Rust"}}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        guard let data = responseJSON.data(using: .utf8) else {
            throw PerplexityClientError.malformedResponse("Rust returned non-UTF8 search data.")
        }
        do {
            let envelope = try Self.decoder.decode(PerplexitySearchEnvelope.self, from: data)
            if let error = envelope.error {
                throw Self.clientError(from: error)
            }
            guard let result = envelope.result else {
                throw PerplexityClientError.malformedResponse("missing search result")
            }
            return PerplexityResult(
                answer: result.answer,
                sources: result.sources.map {
                    PerplexityResult.Source(title: $0.title, url: $0.url)
                }
            )
        } catch let error as PerplexityClientError {
            throw error
        } catch {
            logger.error("Perplexity FFI decode failed: \(String(describing: error), privacy: .public)")
            throw PerplexityClientError.malformedResponse(error.localizedDescription)
        }
    }

    private static func clientError(from error: PerplexityBackendError) -> PerplexityClientError {
        switch error.kind {
        case "invalid_query":
            return .invalidQuery
        case "missing_api_key":
            return .missingAPIKey
        case "invalid_key":
            return .httpStatus(error.statusCode ?? 401)
        case "rate_limited":
            return .httpStatus(error.statusCode ?? 429)
        case "server_error":
            return .httpStatus(error.statusCode ?? 500)
        case "network_error":
            return .transport(error.message)
        case "timed_out":
            return .transport("Online search timed out.")
        case "store_unavailable":
            return .kernelUnavailable
        default:
            return .malformedResponse(error.message)
        }
    }
}

// MARK: - DTOs

private struct PerplexitySearchIntent: Encodable {
    let query: String
}

private struct PerplexitySearchEnvelope: Decodable {
    let result: PerplexitySearchPayload?
    let error: PerplexityBackendError?
}

private struct PerplexitySearchPayload: Decodable {
    let answer: String
    let sources: [PerplexitySourcePayload]
}

private struct PerplexitySourcePayload: Decodable {
    let title: String
    let url: String
}

private struct PerplexityBackendError: Decodable {
    let kind: String
    let message: String
    let statusCode: Int?

    enum CodingKeys: String, CodingKey {
        case kind, message
        case statusCode = "status_code"
    }
}

private struct LegacyPerplexityPayload: Decodable {
    struct Choice: Decodable {
        let message: Message
    }

    struct Message: Decodable {
        let content: String
    }

    struct SearchResult: Decodable {
        let title: String?
        let url: String?
    }

    let choices: [Choice]
    let citations: [String]
    let searchResults: [SearchResult]?

    enum CodingKeys: String, CodingKey {
        case choices, citations
        case searchResults = "search_results"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        choices = try container.decodeIfPresent([Choice].self, forKey: .choices) ?? []
        citations = try container.decodeIfPresent([String].self, forKey: .citations) ?? []
        searchResults = try container.decodeIfPresent([SearchResult].self, forKey: .searchResults)
    }
}

// MARK: - Errors

public enum PerplexityClientError: LocalizedError {
    case invalidQuery
    case missingAPIKey
    case keychain(String)
    case transport(String)
    case httpStatus(Int)
    case kernelUnavailable
    case malformedResponse(String)

    public var errorDescription: String? {
        switch self {
        case .invalidQuery:
            return "Empty Perplexity query."
        case .missingAPIKey:
            return "No Perplexity or OpenRouter key configured. Add one in Settings → Providers."
        case .keychain(let detail):
            return "Keychain error reading Perplexity key: \(detail)"
        case .transport(let detail):
            return "Network error talking to Perplexity: \(detail)"
        case .httpStatus(let code):
            return "Perplexity returned HTTP \(code)."
        case .kernelUnavailable:
            return "Search backend is unavailable. Restart the app and try again."
        case .malformedResponse(let detail):
            return "Couldn't parse Perplexity response: \(detail)"
        }
    }
}
