import Foundation

// Lane 6 — RAG: shared highlight computation used by both `VectorIndex`
// and `InMemoryVectorStore`. Lives in its own file so the in-memory
// fallback never has to depend on the SQLiteVec-importing VectorIndex.
//
// The highlights are deliberately approximate: case-insensitive token
// occurrences, ≥3 chars per token. Good enough for a snippet UI; we can
// upgrade to FTS5's `offsets()` when the SQLiteVec binding exposes it.

enum ChunkHighlights {

    /// Highlight every case-insensitive occurrence of each query token in
    /// `text`. Tokens shorter than 3 characters are ignored to avoid
    /// pathological highlights on stop words.
    static func compute(in text: String, query: String) -> [Range<String.Index>] {
        let tokens = query
            .split { !$0.isLetter && !$0.isNumber }
            .map { String($0).lowercased() }
            .filter { $0.count >= 3 }
        guard !tokens.isEmpty else { return [] }

        var ranges: [Range<String.Index>] = []
        let lower = text.lowercased()
        for token in tokens {
            var search = lower.startIndex..<lower.endIndex
            while let r = lower.range(of: token, range: search) {
                // Lowercasing ASCII preserves byte length, so distance
                // arithmetic on `lower` translates directly to `text`.
                let start = text.index(
                    text.startIndex,
                    offsetBy: lower.distance(from: lower.startIndex, to: r.lowerBound)
                )
                let end = text.index(start, offsetBy: token.count)
                ranges.append(start..<end)
                search = r.upperBound..<lower.endIndex
            }
        }
        return ranges
    }
}
