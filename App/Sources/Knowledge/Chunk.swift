import Foundation

// Lane 6 — RAG: chunk types shared by the chunk builder, the vector store,
// and the agent-tool layer. Milliseconds rather than seconds keeps the model
// integer-precise for player seek calls; speaker is a UUID linking to the
// speaker profile table (Lane 5/13 owns the speaker registry — we just store
// the foreign key).

/// A single retrievable unit produced by `ChunkBuilder` from a transcript.
///
/// Chunks are append-only: the `id` is stable, generated when the transcript
/// is first chunked, and used as the primary key in the vector store. The
/// embedding vector is intentionally **not** part of `Chunk`: it's an internal
/// implementation detail of the vector store, computed from `text`. Callers
/// (Lane 7 wiki indexer, Lane 10 agent tools) only ever read text + metadata.
struct Chunk: Sendable, Hashable, Codable, Identifiable {
    /// Stable identifier for this chunk. Used as the primary key in the
    /// vector store and as the dedup key on re-ingest.
    var id: UUID
    /// Foreign key to the originating `Episode`.
    var episodeID: UUID
    /// Foreign key to the owning `PodcastSubscription`.
    var podcastID: UUID
    /// Raw text of the chunk. Drives both the FTS5 lexical index and the
    /// embedding model input.
    var text: String
    /// Start timestamp in milliseconds, relative to episode start.
    /// Integer-precise so `play_episode(start_seconds)` is unambiguous.
    var startMS: Int
    /// End timestamp in milliseconds, relative to episode start.
    var endMS: Int
    /// Optional foreign key to a speaker profile (when diarization succeeded).
    var speakerID: UUID?

    init(
        id: UUID = UUID(),
        episodeID: UUID,
        podcastID: UUID,
        text: String,
        startMS: Int,
        endMS: Int,
        speakerID: UUID? = nil
    ) {
        self.id = id
        self.episodeID = episodeID
        self.podcastID = podcastID
        self.text = text
        self.startMS = startMS
        self.endMS = endMS
        self.speakerID = speakerID
    }
}

/// Filter applied at query time to narrow which chunks are eligible for a
/// retrieval. The agent picks scope based on the user's intent — "this
/// episode" vs "this podcast" vs "everything I've ever heard".
enum ChunkScope: Sendable, Hashable, Codable {
    /// All chunks, regardless of source.
    case all
    /// Only chunks from a specific podcast subscription.
    case podcast(UUID)
    /// Only chunks from a known set of episodes.
    case episodes(Set<UUID>)
    /// Only chunks from a specific episode.
    case episode(UUID)
    /// Only chunks attributed to a specific speaker (diarized).
    case speaker(UUID)
}

/// A retrieval result: one chunk plus the score that earned it the slot, plus
/// optional highlight ranges over `chunk.text` for the search-result UI.
struct ChunkMatch: Sendable, Hashable {
    /// The matched chunk.
    var chunk: Chunk
    /// Higher is better. For pure-vector queries this is the cosine similarity
    /// (1 - distance). For hybrid queries this is the RRF score, which is on a
    /// different scale — callers should not compare across query types.
    var score: Float
    /// Ranges over `chunk.text` to highlight in the UI. Populated by the
    /// hybrid path from FTS5 token offsets; empty for pure-vector queries.
    var textHighlights: [Range<String.Index>]

    init(
        chunk: Chunk,
        score: Float,
        textHighlights: [Range<String.Index>] = []
    ) {
        self.chunk = chunk
        self.score = score
        self.textHighlights = textHighlights
    }
}
