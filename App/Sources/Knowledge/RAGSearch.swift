import Foundation
import os.log

// Lane 6 — RAG: end-to-end retrieval orchestrator.
//
// Pipeline (`docs/spec/research/embeddings-rag-stack.md` §5):
//   text query → embed → topK || hybridTopK → optional rerank → ChunkMatch[]
//
// Latency budget per the spec:
//   - hybrid path:           ~180ms  (no rerank)
//   - hybrid + rerank path:  ~400ms  (used by the agent for chat-quality answers)
//
// This is the single dependency Lane 10 (`query_transcripts`/`query_wiki`
// agent tools) calls into. Lane 7 (wiki indexer) calls `VectorStore.upsert`
// directly; only the read side runs through `RAGSearch`.

/// Orchestrates the embed → retrieve → (optional rerank) RAG flow.
struct RAGSearch: Sendable {
    /// Tunable knobs for a single query. Defaults match the "balanced"
    /// budget from the research note.
    struct Options: Sendable {
        /// How many results to return after reranking (or after retrieval if
        /// reranking is disabled).
        var k: Int
        /// Over-fetch factor: ask the store for `k * overfetchMultiplier`
        /// candidates so the reranker has more room to pick the best.
        var overfetchMultiplier: Int
        /// When true, runs the BM25 + vector hybrid path. When false, runs
        /// pure vector top-K.
        var hybrid: Bool
        /// When true, reranks the candidates with `cohere/rerank-v3.5`
        /// before truncating to `k`.
        var rerank: Bool

        init(
            k: Int = 5,
            overfetchMultiplier: Int = 4,
            hybrid: Bool = true,
            rerank: Bool = true
        ) {
            self.k = max(1, k)
            self.overfetchMultiplier = max(1, overfetchMultiplier)
            self.hybrid = hybrid
            self.rerank = rerank
        }

        /// Low-latency profile for voice-mode queries (no reranker,
        /// hybrid still on for quality).
        static let voice = Options(k: 5, overfetchMultiplier: 4, hybrid: true, rerank: false)

        /// Quality-first profile for the agent chat surface.
        static let chat = Options(k: 5, overfetchMultiplier: 5, hybrid: true, rerank: true)
    }

    private static let logger = Logger.app("RAGSearch")
    private let store: VectorStore
    private let embedder: EmbeddingsClient
    private let reranker: RerankerClient?

    init(
        store: VectorStore,
        embedder: EmbeddingsClient,
        reranker: RerankerClient? = nil
    ) {
        self.store = store
        self.embedder = embedder
        self.reranker = reranker
    }

    /// Run the full retrieval pipeline. Returns up to `options.k` matches.
    func search(
        query: String,
        scope: ChunkScope? = nil,
        options: Options = .chat
    ) async throws -> [ChunkMatch] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return [] }

        // 1. Embed the query (single-input batch).
        let qVec: [Float]
        do {
            let vectors = try await embedder.embed([trimmed])
            guard let v = vectors.first else { return [] }
            qVec = v
        }

        // 2. Retrieve a candidate window.
        let candidateK = options.k * options.overfetchMultiplier
        let candidates: [ChunkMatch]
        if options.hybrid {
            candidates = try await store.hybridTopK(
                candidateK,
                query: trimmed,
                queryVector: qVec,
                scope: scope
            )
        } else {
            candidates = try await store.topK(
                candidateK,
                for: qVec,
                scope: scope
            )
        }
        guard !candidates.isEmpty else { return [] }

        // 3. Optional rerank.
        guard options.rerank, let reranker, candidates.count > 1 else {
            return Array(candidates.prefix(options.k))
        }
        do {
            let docs = candidates.map(\.chunk.text)
            let order = try await reranker.rerank(
                query: trimmed,
                documents: docs,
                topN: options.k
            )
            // The reranker returns indices into the candidates array; map
            // them back to ChunkMatch values, preserving the new order. If
            // the reranker returns fewer/more than expected, clamp safely.
            var out: [ChunkMatch] = []
            out.reserveCapacity(min(order.count, options.k))
            for idx in order {
                guard idx >= 0, idx < candidates.count else { continue }
                out.append(candidates[idx])
                if out.count >= options.k { break }
            }
            return out
        } catch {
            // Reranking is a quality lift, not a correctness gate. If the
            // network call fails we still return the hybrid results — the
            // agent's answer is degraded but not broken.
            Self.logger.warning("Reranker failed; falling back to hybrid order: \(error.localizedDescription, privacy: .public)")
            return Array(candidates.prefix(options.k))
        }
    }
}
