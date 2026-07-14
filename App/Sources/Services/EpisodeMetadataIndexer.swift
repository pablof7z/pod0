import Foundation
import os.log

// MARK: - EpisodeMetadataIndexer
//
// Embeds a single "title + description" Chunk per Episode into the RAG
// vector index, so `search_episodes` / `find_similar_episodes` can surface
// episodes that have not been (or never will be) transcribed.
//
// Why a separate indexer rather than folding into TranscriptIngestService:
// transcript ingestion only runs for episodes the user opens or that the
// auto-ingest pipeline picks up. Subscribing to a new podcast dumps an
// entire back-catalog whose episodes never receive a transcript — those
// were previously invisible to similarity search. This service guarantees
// every episode gets at least the title/description signal indexed.
//
// Lifecycle:
// 1. `indexNewlyInserted(...)` — fired from `AppStateStore.upsertEpisodes`
//    immediately after new rows land. Covers both the steady-state feed
//    refresh path and the initial-subscribe back-catalog dump.
// 2. `runBackfill(appStore:)` — fired once at launch from `AppMain`. Picks
//    up everything `state.episodes.filter { !$0.metadataIndexed }` so
//    pre-existing libraries get covered exactly once.
//
// Coexistence with transcripts: `TranscriptIngestService.persistAndIndex`
// calls `rag.index.deleteAll(forEpisodeID:)` before upserting transcript
// chunks, so transcript ingestion automatically replaces a synthetic
// metadata chunk. The reverse race (metadata indexer running after a
// transcript already landed) is gated by the `metadataIndexed` flag,
// which transcript ingestion also flips on success.

@MainActor
final class EpisodeMetadataIndexer {

    // MARK: Singleton

    static let shared = EpisodeMetadataIndexer()

    // MARK: Logger

    nonisolated private static let logger = Logger.app("EpisodeMetadataIndexer")

    // MARK: Dependencies

    private let store: VectorStore

    // MARK: Tuning

    /// Episodes embedded per network round-trip. The provider embedder
    /// batches internally; this caps how many chunks we ship in one
    /// `upsert(chunks:)` so a 5,000-episode backfill stays responsive
    /// and respects rate limits.
    private let batchSize: Int = 32

    /// Pause between backfill batches. Cheap insurance against hammering
    /// the embeddings provider on a cold launch with a large library.
    private let interBatchDelayNanoseconds: UInt64 = 200_000_000  // 0.2s

    // MARK: In-flight tracking (dedup)

    /// Set of episode IDs currently being indexed. Prevents a backfill +
    /// `indexNewlyInserted` from racing on the same episode after a feed
    /// refresh fires mid-launch.
    private var inFlight: Set<UUID> = []

    /// `true` while `runBackfill` is iterating. Subsequent calls no-op
    /// — `upsertEpisodes`-driven incremental indexing still runs and
    /// keeps newly-arriving episodes covered while the backfill drains
    /// the back-catalog.
    private var backfillRunning: Bool = false

    // MARK: Init

    init(store: VectorStore = RAGService.shared.index) {
        self.store = store
    }

    // MARK: Public API

    /// Index the metadata for the given newly-inserted episodes. Called from
    /// `AppStateStore.upsertEpisodes`. Best-effort: on embed failure we log
    /// and leave `metadataIndexed=false` so the next launch's backfill
    /// retries.
    func indexNewlyInserted(_ ids: [UUID], appStore: AppStateStore) {
        guard !ids.isEmpty else { return }
        Task { @MainActor [weak self, weak appStore] in
            guard let self, let appStore else { return }
            await self.indexEpisodes(ids: ids, appStore: appStore)
        }
    }

    /// Walk the library once and embed metadata for every episode that
    /// doesn't yet have `metadataIndexed = true`. Reentrancy-safe: a
    /// second call while one is in flight is a no-op.
    func runBackfill(appStore: AppStateStore) async {
        guard !backfillRunning else { return }
        backfillRunning = true
        defer { backfillRunning = false }

        let pending = appStore.state.episodes
            .filter { !$0.metadataIndexed }
            .map(\.id)
        guard !pending.isEmpty else {
            Self.logger.debug("backfill: nothing to index")
            return
        }
        Self.logger.info(
            "backfill starting — \(pending.count, privacy: .public) episodes to metadata-index"
        )

        var indexedSoFar = 0
        for batch in pending.chunked(into: batchSize) {
            let succeeded = await indexEpisodes(ids: batch, appStore: appStore)
            indexedSoFar += succeeded
            // If a batch fails (e.g. provider down, missing key), stop
            // the backfill — there's no point burning more API calls in
            // the same condition. Next launch will resume.
            if succeeded == 0 { break }
            try? await Task.sleep(nanoseconds: interBatchDelayNanoseconds)
        }
        Self.logger.info(
            "backfill done — indexed \(indexedSoFar, privacy: .public) of \(pending.count, privacy: .public)"
        )
    }

    // MARK: Core

    /// Returns the number of episodes successfully indexed (and thus
    /// flagged `metadataIndexed = true`). Zero means the embed call
    /// failed; the caller can choose to halt a bulk backfill.
    @discardableResult
    private func indexEpisodes(ids: [UUID], appStore: AppStateStore) async -> Int {
        let claimable = ids.filter { !inFlight.contains($0) }
        guard !claimable.isEmpty else { return 0 }
        inFlight.formUnion(claimable)
        defer { inFlight.subtract(claimable) }

        // Resolve episodes + build chunks. Skip episodes that vanished
        // (deleted concurrently), episodes already indexed (transcript
        // beat us to it), and episodes with empty title+description.
        var chunks: [Chunk] = []
        var preparedIDs: [UUID] = []
        for id in claimable {
            guard let episode = appStore.episode(id: id),
                  !episode.metadataIndexed,
                  let chunk = Self.makeChunk(for: episode) else { continue }
            chunks.append(chunk)
            preparedIDs.append(id)
        }
        guard !chunks.isEmpty else { return 0 }

        do {
            try await store.upsert(chunks: chunks)
            appStore.setEpisodesMetadataIndexed(preparedIDs)
            Self.logger.debug(
                "indexed metadata for \(preparedIDs.count, privacy: .public) episodes"
            )
            return preparedIDs.count
        } catch {
            // Don't flip the flag — next backfill retries. Common causes
            // are missing embeddings key, rate limits, or transient
            // network errors; all resolved by a later run.
            Self.logger.notice(
                "metadata index batch failed (\(preparedIDs.count, privacy: .public) episodes): \(String(describing: error), privacy: .public)"
            )
            return 0
        }
    }

    // MARK: Helpers

    /// Build the synthetic chunk for an episode. Returns `nil` when there
    /// is no text worth embedding.
    private static func makeChunk(for episode: Episode) -> Chunk? {
        let titleText = episode.title.trimmingCharacters(in: .whitespacesAndNewlines)
        let descriptionPlain = EpisodeShowNotesFormatter.plainText(from: episode.description)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let parts = [titleText, descriptionPlain].filter { !$0.isEmpty }
        guard !parts.isEmpty else { return nil }
        let text = parts.joined(separator: "\n\n")
        return Chunk(
            episodeID: episode.id,
            podcastID: episode.podcastID,
            text: text,
            startMS: 0,
            endMS: 0,
            speakerID: nil
        )
    }
}

// MARK: - Array chunking

private extension Array {
    /// Splits the array into contiguous slices of at most `size` elements.
    /// `size` must be positive; a non-positive argument returns `[self]`.
    func chunked(into size: Int) -> [[Element]] {
        guard size > 0 else { return [self] }
        var result: [[Element]] = []
        result.reserveCapacity((count + size - 1) / size)
        var idx = 0
        while idx < count {
            let end = Swift.min(idx + size, count)
            result.append(Array(self[idx..<end]))
            idx = end
        }
        return result
    }
}
