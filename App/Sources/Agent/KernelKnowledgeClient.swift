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

struct HomeRelatedKernelRow: Decodable, Equatable {
    let id: String
    let episodeId: String
    let podcastId: String
    let episodeTitle: String
    let podcastTitle: String
    let chunkIndex: Int
    let text: String
}

private struct HomeRelatedResponseEnvelope: Decodable {
    let result: [HomeRelatedKernelRow]?
    let error: String?
}

private struct KnowledgeScopeResolution: Decodable {
    let podcastId: String?
    let episodeId: String?
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

    /// Ask the kernel for episodes similar to `episodeId`. Rust resolves the
    /// seed episode and owns the seed-query policy; Swift only passes intent.
    static func similarEpisodes(
        episodeId: String,
        limit: Int
    ) async throws -> [KnowledgeQueryRow] {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            logger.error("similar episode query skipped: kernel handle not available")
            throw KernelKnowledgeError.kernelUnavailable
        }

        let requestJSON = buildSimilarEpisodeRequestJSON(episodeId: episodeId, limit: limit)
        logger.info("knowledge_similar_episode episodeId=\(episodeId) limit=\(limit)")

        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"kernel handle unavailable"}"#
            }
            return requestJSON.withCString { reqPtr -> String in
                guard let ptr = nmp_app_podcast_knowledge_similar_episode(handle, reqPtr) else {
                    return #"{"error":"null response from nmp_app_podcast_knowledge_similar_episode"}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        return try parseResponse(responseJSON)
    }

    /// Ask the kernel for the Home "Related" sheet rows. Rust owns seed-query
    /// construction, topic/source lens behavior, seed filtering, show dedupe,
    /// and no-index fallback policy.
    static func homeRelated(
        episodeId: String,
        lens: String,
        limit: Int
    ) async throws -> [HomeRelatedKernelRow] {
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            logger.error("home related query skipped: kernel handle not available")
            throw KernelKnowledgeError.kernelUnavailable
        }

        let requestJSON = buildHomeRelatedRequestJSON(episodeId: episodeId, lens: lens, limit: limit)
        logger.info("knowledge_home_related episodeId=\(episodeId) lens=\(lens) limit=\(limit)")

        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"kernel handle unavailable"}"#
            }
            return requestJSON.withCString { reqPtr -> String in
                guard let ptr = nmp_app_podcast_knowledge_home_related(handle, reqPtr) else {
                    return #"{"error":"null response from nmp_app_podcast_knowledge_home_related"}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        return try parseHomeRelatedResponse(responseJSON)
    }

    /// Resolve a raw tool-provided scope reference into the knowledge query's
    /// explicit podcast/episode scope. Rust owns canonical existence checks and
    /// preserves the product fallback of unknown UUIDs narrowing to episode
    /// scope.
    static func resolveScope(
        _ scope: String?
    ) async throws -> (podcastId: String?, episodeId: String?) {
        guard let scope, UUID(uuidString: scope) != nil else {
            return (nil, nil)
        }
        guard let handleBits = await MainActor.run(body: {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }) else {
            logger.error("knowledge scope resolve skipped: kernel handle not available")
            throw KernelKnowledgeError.kernelUnavailable
        }

        let requestJSON = buildScopeResolutionRequestJSON(scope: scope)
        let responseJSON = await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return #"{"error":"kernel handle unavailable"}"#
            }
            return requestJSON.withCString { reqPtr -> String in
                guard let ptr = nmp_app_podcast_knowledge_resolve_scope(handle, reqPtr) else {
                    return #"{"error":"null response from nmp_app_podcast_knowledge_resolve_scope"}"#
                }
                defer { nmp_free_string(ptr) }
                return String(cString: ptr)
            }
        }.value

        let resolved = try parseScopeResolution(responseJSON)
        return (resolved.podcastId, resolved.episodeId)
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

    private static func buildSimilarEpisodeRequestJSON(episodeId: String, limit: Int) -> String {
        let dict: [String: Any] = ["episode_id": episodeId, "limit": limit]
        guard let data = try? JSONSerialization.data(withJSONObject: dict),
              let str = String(data: data, encoding: .utf8) else {
            logger.fault("buildSimilarEpisodeRequestJSON: JSON serialization failed")
            return "{\"episode_id\":\"\",\"limit\":0}"
        }
        return str
    }

    private static func buildHomeRelatedRequestJSON(episodeId: String, lens: String, limit: Int) -> String {
        let dict: [String: Any] = ["episode_id": episodeId, "lens": lens, "limit": limit]
        guard let data = try? JSONSerialization.data(withJSONObject: dict),
              let str = String(data: data, encoding: .utf8) else {
            logger.fault("buildHomeRelatedRequestJSON: JSON serialization failed")
            return "{\"episode_id\":\"\",\"lens\":\"topic\",\"limit\":0}"
        }
        return str
    }

    private static func buildScopeResolutionRequestJSON(scope: String) -> String {
        let dict: [String: Any] = ["scope": scope]
        guard let data = try? JSONSerialization.data(withJSONObject: dict),
              let str = String(data: data, encoding: .utf8) else {
            logger.fault("buildScopeResolutionRequestJSON: JSON serialization failed")
            return "{\"scope\":\"\"}"
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

    private static func parseScopeResolution(_ json: String) throws -> KnowledgeScopeResolution {
        guard let data = json.data(using: .utf8) else {
            throw KernelKnowledgeError.malformedResponse("non-UTF8 response")
        }
        do {
            let resolution = try decoder.decode(KnowledgeScopeResolution.self, from: data)
            if let errorMessage = resolution.error {
                logger.error("knowledge_resolve_scope kernel error: \(errorMessage, privacy: .public)")
                throw KernelKnowledgeError.kernelError(errorMessage)
            }
            return resolution
        } catch let error as KernelKnowledgeError {
            throw error
        } catch {
            logger.error("knowledge_resolve_scope decode failed: \(error.localizedDescription, privacy: .public)")
            throw KernelKnowledgeError.malformedResponse(error.localizedDescription)
        }
    }

    private static func parseHomeRelatedResponse(_ json: String) throws -> [HomeRelatedKernelRow] {
        guard let data = json.data(using: .utf8) else {
            throw KernelKnowledgeError.malformedResponse("non-UTF8 response")
        }
        let envelope: HomeRelatedResponseEnvelope
        do {
            envelope = try decoder.decode(HomeRelatedResponseEnvelope.self, from: data)
        } catch {
            logger.error("knowledge_home_related decode failed: \(error.localizedDescription, privacy: .public)")
            throw KernelKnowledgeError.malformedResponse(error.localizedDescription)
        }
        if let errorMessage = envelope.error {
            logger.error("knowledge_home_related kernel error: \(errorMessage, privacy: .public)")
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
