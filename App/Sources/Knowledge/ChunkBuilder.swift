import Foundation

// Lane 6 — RAG: transcript chunker.
//
// Slides a 400-512 token window across a transcript with 15% overlap, snapping
// to speaker-turn boundaries when a snap is within tolerance. We deliberately
// do NOT depend on Lane 5's concrete `Transcript` struct — Lane 5 will arrive
// with its own segments/turns shape. We define a minimal `TranscriptLike`
// protocol that any future transcript model can conform to, plus a
// `TranscriptSegment` protocol for individual time-stamped utterances.

// MARK: - Transcript protocols

/// Anything that can be chunked. Lane 5's `Transcript` will conform.
protocol TranscriptLike {
    associatedtype Segment: TranscriptSegment
    /// Foreign key to the originating episode.
    var episodeID: UUID { get }
    /// Foreign key to the owning podcast subscription.
    var podcastID: UUID { get }
    /// Time-ordered segments produced by the transcription pipeline. Each
    /// segment is a single utterance from a single speaker.
    var segments: [Segment] { get }
}

/// A single time-stamped utterance within a transcript.
protocol TranscriptSegment {
    /// Raw text of the utterance.
    var text: String { get }
    /// Start of the utterance in milliseconds, relative to episode start.
    var startMS: Int { get }
    /// End of the utterance in milliseconds, relative to episode start.
    var endMS: Int { get }
    /// Optional foreign key to a speaker profile (when diarization succeeded).
    var speakerID: UUID? { get }
}

// MARK: - Builder

/// Builds embedding-ready `Chunk`s from a transcript.
///
/// Defaults: 400 target tokens, 15% overlap, ±20% boundary-snap tolerance to
/// align chunk edges with speaker turns. These defaults are tuned for podcast
/// transcripts per `docs/spec/research/embeddings-rag-stack.md`.
struct ChunkBuilder: Sendable {
    let targetTokens: Int
    let overlapFraction: Double
    let snapTolerance: Double

    init(
        targetTokens: Int = 400,
        overlapFraction: Double = 0.15,
        snapTolerance: Double = 0.20
    ) {
        self.targetTokens = max(20, targetTokens)
        self.overlapFraction = max(0, min(0.5, overlapFraction))
        self.snapTolerance = max(0, min(0.5, snapTolerance))
    }

    /// Approximate token count for a piece of text. We don't ship a tokenizer
    /// dependency just for chunking; whitespace-split words × 1.3 is a stable
    /// heuristic that lands within ~15% of true OpenAI BPE counts on English
    /// prose, which is good enough for chunk-size budgeting.
    static func approximateTokenCount(_ text: String) -> Int {
        let words = text.split { $0.isWhitespace }.count
        return Int((Double(words) * 1.3).rounded(.up))
    }

    /// Produce chunks for `transcript`. Stable across runs given identical
    /// input, so re-ingestion is idempotent on chunk index.
    func build<T: TranscriptLike>(from transcript: T) -> [Chunk] {
        let segments = transcript.segments
        guard !segments.isEmpty else { return [] }

        let target = targetTokens
        let overlap = Int(Double(target) * overlapFraction)
        let snapWindow = Int(Double(target) * snapTolerance)

        // Pre-compute per-segment token counts so the sliding window is a
        // single linear scan instead of a quadratic word-count loop.
        let segTokens = segments.map { Self.approximateTokenCount($0.text) }

        var chunks: [Chunk] = []
        var cursor = 0
        var chunkIndex = 0

        while cursor < segments.count {
            // Greedily extend until we hit the target token budget.
            var endIdx = cursor
            var running = 0
            while endIdx < segments.count && running + segTokens[endIdx] <= target {
                running += segTokens[endIdx]
                endIdx += 1
            }
            // If a single segment is larger than the target, take it whole —
            // splitting mid-utterance loses time grounding.
            if endIdx == cursor {
                endIdx = cursor + 1
                running = segTokens[cursor]
            }

            // Snap-to-speaker-boundary: if there's a speaker turn within
            // ±snapWindow tokens of `endIdx`, prefer that boundary.
            endIdx = snapToSpeakerBoundary(
                segments: segments,
                tokens: segTokens,
                startIdx: cursor,
                preferredEndIdx: endIdx,
                snapWindowTokens: snapWindow
            )

            let slice = segments[cursor..<endIdx]
            let chunk = makeChunk(
                index: chunkIndex,
                segments: Array(slice),
                episodeID: transcript.episodeID,
                podcastID: transcript.podcastID
            )
            chunks.append(chunk)
            chunkIndex += 1

            // Advance the cursor by `endIdx - overlap-segments`, where
            // overlap-segments is the smallest k such that the trailing k
            // segments sum to ≥ overlap tokens. Guarantees forward progress
            // even when overlap is larger than the chunk (degenerate case).
            let advance = computeAdvance(
                tokens: segTokens,
                from: cursor,
                to: endIdx,
                overlapTokens: overlap
            )
            cursor += max(1, advance)
        }

        return chunks
    }

    // MARK: - Helpers

    /// If a speaker turn boundary is within `snapWindowTokens` of the
    /// preferred end index, snap to it (prefer the closer boundary).
    private func snapToSpeakerBoundary<S: TranscriptSegment>(
        segments: [S],
        tokens: [Int],
        startIdx: Int,
        preferredEndIdx: Int,
        snapWindowTokens: Int
    ) -> Int {
        guard snapWindowTokens > 0, preferredEndIdx > startIdx else { return preferredEndIdx }
        guard preferredEndIdx < segments.count else { return preferredEndIdx }

        // Find candidate turn boundaries within the snap window.
        // A "turn boundary" is an index `i` where segments[i-1].speakerID !=
        // segments[i].speakerID. We measure window in tokens, not indices.
        var bestIdx = preferredEndIdx
        var bestDelta = Int.max

        // Walk backward from preferredEndIdx until we exceed the snap window.
        var back = preferredEndIdx
        var backTokens = 0
        while back > startIdx + 1, backTokens <= snapWindowTokens {
            if segments[back - 1].speakerID != segments[back].speakerID {
                let delta = backTokens
                if delta < bestDelta {
                    bestDelta = delta
                    bestIdx = back
                }
            }
            back -= 1
            backTokens += tokens[back]
        }

        // Walk forward from preferredEndIdx within the snap window.
        var fwd = preferredEndIdx
        var fwdTokens = 0
        while fwd < segments.count - 1, fwdTokens <= snapWindowTokens {
            if segments[fwd].speakerID != segments[fwd + 1].speakerID {
                let delta = fwdTokens
                if delta < bestDelta {
                    bestDelta = delta
                    bestIdx = fwd + 1
                }
            }
            fwdTokens += tokens[fwd]
            fwd += 1
        }

        return bestIdx
    }

    /// How many segments to advance the sliding window so that the trailing
    /// chunk overlaps by approximately `overlapTokens`.
    private func computeAdvance(
        tokens: [Int],
        from startIdx: Int,
        to endIdx: Int,
        overlapTokens: Int
    ) -> Int {
        guard overlapTokens > 0, endIdx > startIdx else { return endIdx - startIdx }
        var k = 0
        var running = 0
        var i = endIdx - 1
        while i >= startIdx, running < overlapTokens {
            running += tokens[i]
            k += 1
            i -= 1
        }
        let advance = (endIdx - startIdx) - k
        return advance > 0 ? advance : 1
    }

    /// Build a single `Chunk` from a contiguous slice of segments.
    /// ID is deterministic from `(episodeID, chunkIndex)` so re-ingest is
    /// idempotent — the vector store can `INSERT OR REPLACE` without dups.
    private func makeChunk<S: TranscriptSegment>(
        index: Int,
        segments: [S],
        episodeID: UUID,
        podcastID: UUID
    ) -> Chunk {
        let text = segments.map(\.text).joined(separator: " ")
        let startMS = segments.first?.startMS ?? 0
        let endMS = segments.last?.endMS ?? startMS
        let speakerID = dominantSpeakerID(in: segments)
        return Chunk(
            id: deterministicID(episodeID: episodeID, chunkIndex: index),
            episodeID: episodeID,
            podcastID: podcastID,
            text: text,
            startMS: startMS,
            endMS: endMS,
            speakerID: speakerID
        )
    }

    /// Whichever speaker contributed the most tokens in this slice.
    private func dominantSpeakerID<S: TranscriptSegment>(in segments: [S]) -> UUID? {
        var counts: [UUID: Int] = [:]
        for seg in segments {
            guard let id = seg.speakerID else { continue }
            counts[id, default: 0] += Self.approximateTokenCount(seg.text)
        }
        return counts.max(by: { $0.value < $1.value })?.key
    }

    /// Deterministic UUIDv5-style ID for a chunk position. We don't pull in a
    /// crypto dep just for UUIDv5 — combining the episode bytes with the
    /// index in a stable way is sufficient for our idempotency need.
    private func deterministicID(episodeID: UUID, chunkIndex: Int) -> UUID {
        var bytes = episodeID.uuid
        // Mix the chunk index into the last 4 bytes. UUIDv5 would do this
        // via SHA-1; we just XOR. Collisions across episodes are impossible
        // because the first 12 bytes are episode-scoped.
        let idx = UInt32(truncatingIfNeeded: chunkIndex)
        bytes.12 ^= UInt8(truncatingIfNeeded: idx >> 24)
        bytes.13 ^= UInt8(truncatingIfNeeded: idx >> 16)
        bytes.14 ^= UInt8(truncatingIfNeeded: idx >> 8)
        bytes.15 ^= UInt8(truncatingIfNeeded: idx)
        return UUID(uuid: bytes)
    }
}
