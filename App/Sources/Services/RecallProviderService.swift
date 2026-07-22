import Foundation
import Pod0Core
import os.log

protocol RecallProviderExecuting: Sendable {
    func embed(
        provider: RecallEmbeddingProvider,
        model: String,
        dimensions: Int,
        texts: [String]
    ) async throws -> [[Float]]

    func rerank(
        provider: RecallRerankProvider,
        model: String,
        query: String,
        documents: [String]
    ) async throws -> [Int]
}

/// Native network-provider container for Rust-owned recall workflows.
/// It owns no durable index, evidence state, ranking, retries, or fallback.
final class RecallProviderService: RecallProviderExecuting, @unchecked Sendable {
    static let shared = RecallProviderService()

    nonisolated private static let logger = Logger.app("RecallProviderService")

    private init() {
        if !OpenRouterCredentialStore.hasAPIKey() && !OllamaCredentialStore.hasAPIKey() {
            Self.logger.warning("No recall embedding provider is configured")
        }
    }

    func embed(
        provider: RecallEmbeddingProvider,
        model: String,
        dimensions: Int,
        texts: [String]
    ) async throws -> [[Float]] {
        switch provider {
        case .openRouter:
            return try await OpenRouterEmbeddingsClient(
                model: model,
                dimensions: dimensions
            ).embed(texts)
        case .ollama:
            return try await OllamaEmbeddingsClient(
                model: model,
                expectedDimensions: dimensions
            ).embed(texts)
        case .unsupported:
            throw RecallProviderExecutionError.unsupportedProvider
        }
    }

    func rerank(
        provider: RecallRerankProvider,
        model: String,
        query: String,
        documents: [String]
    ) async throws -> [Int] {
        switch provider {
        case .openRouter:
            return try await OpenRouterRerankerClient(model: model).rerank(
                query: query,
                documents: documents,
                topN: documents.count
            )
        case .unsupported:
            throw RecallProviderExecutionError.unsupportedProvider
        }
    }
}

enum RecallProviderExecutionError: Error {
    case unsupportedProvider
}
