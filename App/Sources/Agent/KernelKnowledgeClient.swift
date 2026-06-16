import Foundation
import os.log

// MARK: - KnowledgeQueryRow

/// Decodable row returned by `nmp_app_podcast_knowledge_query`.
///
/// Properties are camelCase. The bridge decoder applies `.convertFromSnakeCase`
/// (see `KernelDecoding.makeDecoder`), which maps kernel wire keys
/// (`episode_id`, `start_secs`, `end_secs`, `relevance_score`, …) to their
/// camelCase counterparts. NO explicit `CodingKeys` are permitted here: under
/// `.convertFromSnakeCase` an explicit snake_case mapping double-converts and
/// throws `keyNotFound`, dropping the entire row (the #371 hazard documented in
/// `ffi_decode_snakecase_contract`).
struct KnowledgeQueryRow: Decodable {
    let episodeId: String
    let podcastId: String
    let episodeTitle: String
    let podcastTitle: String
    let chunkIndex: Int
    let startSecs: Double
    let endSecs: Double
    let text: String
    let relevanceScore: Double
}

// MARK: - KnowledgeQueryResponseEnvelope

/// Top-level wrapper returned by `nmp_app_podcast_knowledge_query`.
/// Either `result` (array, possibly empty) or `error` (plain string) is
/// present; never both.
struct KnowledgeQueryResponseEnvelope: Decodable {
    let result: [KnowledgeQueryRow]?
    let error: String?
}

// MARK: - KernelKnowledgeClient

/// Thin Swift wrapper around the `nmp_app_podcast_knowledge_query` FFI.
///
/// The FFI issues a `block_on` network embed call; it MUST NOT run on the main
/// actor or any Swift cooperative thread. Every public entry point hops to a
/// `Task.detached` before touching the C function, mirroring the pattern used
/// by `PerplexityClient` (`nmp_app_podcast_perplexity_search`).
enum KernelKnowledgeClient {

    private static let logger = Logger.app("KernelKnowledgeClient")
    private static let decoder = KernelDecoding.makeDecoder()

    // MARK: - Public API

    /// Search the kernel's semantic index.
    ///
    /// - Parameters:
    ///   - query:     Natural-language search text.
    ///   - podcastId: Narrow to a single podcast subscription (mutually exclusive
    ///                with `episodeId`). `nil` searches the whole library.
    ///   - episodeId: Narrow to a single episode's chunks.
    ///   - limit:     Maximum number of chunk rows to return.
    static func query(
        query: String,
        podcastId: String? = nil,
        episodeId: String? = nil,
        limit: Int
    ) async throws -> [KnowledgeQueryRow] {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            logger.error("knowledge query skipped: kernel handle not available")
            throw KernelKnowledgeError.kernelUnavailable
        }

        let requestJSON = buildRequestJSON(
            query: query, podcastId: podcastId, episodeId: episodeId, limit: limit)
        logger.info("knowledge_query q=\(query.prefix(60)) podcastId=\(podcastId ?? "nil") episodeId=\(episodeId ?? "nil") limit=\(limit)")

        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"kernel handle unavailable"}"#
            }
            return requestJSON.withCString { reqPtr -> String in
                guard let ptr = nmp_app_podcast_knowledge_query(handle, reqPtr) else {
                    return #"{"error":"null response from nmp_app_podcast_knowledge_query"}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        return try parseResponse(responseJSON)
    }

    // MARK: - Private helpers

    private static func buildRequestJSON(
        query: String,
        podcastId: String?,
        episodeId: String?,
        limit: Int
    ) -> String {
        var dict: [String: Any] = ["query": query, "limit": limit]
        if let pid = podcastId {
            dict["scope"] = ["podcast_id": pid]
        } else if let eid = episodeId {
            dict["scope"] = ["episode_id": eid]
        }
        guard let data = try? JSONSerialization.data(withJSONObject: dict),
              let str = String(data: data, encoding: .utf8) else {
            logger.fault("buildRequestJSON: JSON serialization failed")
            return "{\"query\":\"\",\"limit\":0}"
        }
        return str
    }

    private static func parseResponse(_ json: String) throws -> [KnowledgeQueryRow] {
        guard let data = json.data(using: .utf8) else {
            throw KernelKnowledgeError.malformedResponse("non-UTF8 response")
        }
        let envelope: KnowledgeQueryResponseEnvelope
        do {
            envelope = try decoder.decode(KnowledgeQueryResponseEnvelope.self, from: data)
        } catch {
            logger.error("knowledge_query decode failed: \(error.localizedDescription, privacy: .public)")
            throw KernelKnowledgeError.malformedResponse(error.localizedDescription)
        }
        if let errorMessage = envelope.error {
            logger.error("knowledge_query kernel error: \(errorMessage, privacy: .public)")
            throw KernelKnowledgeError.kernelError(errorMessage)
        }
        return envelope.result ?? []
    }
}

// MARK: - KernelKnowledgeError

enum KernelKnowledgeError: LocalizedError {
    case kernelUnavailable
    case kernelError(String)
    case malformedResponse(String)

    var errorDescription: String? {
        switch self {
        case .kernelUnavailable:
            return "Knowledge search is unavailable: kernel not running."
        case .kernelError(let msg):
            return "Kernel knowledge query failed: \(msg)"
        case .malformedResponse(let detail):
            return "Could not parse knowledge query response: \(detail)"
        }
    }
}
