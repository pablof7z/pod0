import Foundation
import os

// MARK: - One-shot kernel knowledge re-backfill migration (Slice 4)
//
// Context: STT stores completed transcripts via `kernelTranscriptReport` →
// `set_transcript`, but `index_episode` was never dispatched. As a result
// the kernel KnowledgeStore is EMPTY even on a device with many transcribed
// episodes. This one-shot migration fires once after slice-4 ships and
// re-indexes every episode whose transcript is already in the kernel by
// dispatching `podcast.knowledge.index_episode` for each.
//
// Design constraints (memory: oneshot_migration_flag_after_work):
//   - Guard flag is set AFTER the dispatch loop, not before. A mid-loop
//     crash leaves no partial state — `index_episode` is idempotent
//     (deletes + re-upserts) — and the migration retries on next launch.
//   - Idempotent: safe to run multiple times; the kernel deduplicates.
//   - No data loss: vectors.sqlite is untouched. The Swift VectorIndex /
//     RAGService / agent adapter remain intact as dormant fallback (slice 5
//     territory).
//   - Reactive: waits for the first kernel snapshot via `awaitState` so we
//     don't race against a cold-start with no episodes yet.
//   - PACED: each `index_episode` synchronously chunks on the actor, calls
//     `infra.bump()` (⇒ a full-library snapshot decode on the iOS MAIN thread,
//     memory perf_snapshot_decode_hotpath), AND eagerly spawns an embed API
//     call. Firing N of those at launch in one tight loop pegs the CPU. So we
//     dispatch in batches with an inter-batch sleep, mirroring the
//     metadata-indexer (#511): the actor chunk passes, the main-thread
//     snapshot-bump pipeline, and the per-episode embed spawns get spread out
//     instead of all firing at once.

extension AppStateStore {

    /// UserDefaults key guarding the one-shot knowledge re-backfill.
    ///
    /// Versioned ("V1") so a future schema change can bump the suffix and
    /// force a re-index without needing to wipe the flag manually.
    private static let knowledgeBackfillFlagKey = "kernelKnowledgeBackfillV1"

    /// How many `index_episode` dispatches fire before the driver yields.
    /// Mirrors the metadata-indexer batch size (#511); keeps each burst of
    /// actor chunk passes + main-thread snapshot bumps + embed spawns bounded.
    private static let knowledgeBackfillBatchSize = 16

    /// Cooldown between batches. Spreads the main-thread snapshot-decode
    /// pipeline (one decode per `infra.bump()`) so the backfill never pegs the
    /// CPU at launch. Matches the metadata-indexer inter-batch delay (#511).
    private static let knowledgeBackfillBatchDelayNanos: UInt64 = 200_000_000

    /// Wire point: call once from `attachKernel(_:)` after the kernel is
    /// attached and the data dir is bound. Fast-path no-op on every
    /// subsequent launch (UserDefaults guard).
    func backfillKernelKnowledge() {
        guard !UserDefaults.standard.bool(forKey: Self.knowledgeBackfillFlagKey) else { return }
        guard let kern = kernel else { return }

        Task { @MainActor [weak self] in
            guard let self else { return }

            // Wait for the first kernel snapshot to populate `self.episodes`.
            // `awaitState` re-evaluates the closure on every observation tick
            // and returns the first non-nil value — no polling (project rule).
            //
            // The closure returns `nil` until the episode list is non-empty
            // (i.e. the first snapshot has landed); once populated it
            // immediately filters to the transcribed subset and returns it
            // (possibly an empty array on a device with no transcripts).
            //
            // CRITICAL: `awaitState` returns `nil` ON TIMEOUT, which is NOT the
            // same as "loaded, zero transcripts" (a genuine empty `[]`). If we
            // collapsed timeout into `[]` and set the flag, a single slow
            // cold-start would permanently consume the one-shot, stranding an
            // EMPTY kernel store with Search no longer on the Swift fallback
            // (memory oneshot_migration_flag_after_work — the stranding class).
            // So we keep the optional and treat `nil` as "retry next launch".
            let loaded: [UUID]? = await awaitState(
                timeout: .seconds(60),
                body: { [weak self] () -> [UUID]? in
                    guard let self else { return nil }
                    // Keep waiting while the library hasn't arrived yet.
                    guard !self.episodes.isEmpty else { return nil }
                    return self.episodes.compactMap { episode -> UUID? in
                        guard case .ready = episode.transcriptState else { return nil }
                        return episode.id
                    }
                }
            )

            // Timeout (the snapshot never landed within 60 s): do NOT set the
            // flag. The migration retries on the next launch.
            guard let transcribedIDs = loaded else {
                os_log(.info,
                       log: OSLog(subsystem: "io.f7z.podcast", category: "KnowledgeBackfill"),
                       "Knowledge backfill: snapshot did not arrive within timeout — leaving flag unset to retry next launch")
                return
            }

            // Genuine empty set (snapshot landed, zero transcribed episodes):
            // mark done — the store is indexed as-is and fresh transcripts
            // arrive via the live `index_episode` dispatch in the STT pipeline.
            guard !transcribedIDs.isEmpty else {
                UserDefaults.standard.set(true, forKey: Self.knowledgeBackfillFlagKey)
                os_log(.info,
                       log: OSLog(subsystem: "io.f7z.podcast", category: "KnowledgeBackfill"),
                       "Knowledge backfill: no transcribed episodes to index — done")
                return
            }

            os_log(.info,
                   log: OSLog(subsystem: "io.f7z.podcast", category: "KnowledgeBackfill"),
                   "Knowledge backfill: indexing %d transcribed episodes (paced, batch=%d)",
                   transcribedIDs.count, Self.knowledgeBackfillBatchSize)

            // Dispatch `index_episode` in PACED batches. The kernel handler
            // (`KnowledgeState::index_episode`) chunks the stored transcript
            // text synchronously on the actor thread, bumps the rev (⇒ a
            // main-thread snapshot decode), and spawns an embed call. Sleeping
            // between batches spreads all three cost classes so a large
            // backfill never pegs the CPU at launch. BM25 search responds as
            // soon as each batch's chunks land; semantic search refines as the
            // embeds complete.
            let batchSize = Self.knowledgeBackfillBatchSize
            var dispatched = 0
            var index = 0
            while index < transcribedIDs.count {
                let end = min(index + batchSize, transcribedIDs.count)
                for id in transcribedIDs[index..<end] {
                    kern.dispatch(namespace: "podcast.knowledge",
                                  body: ["op": "index_episode",
                                         "episode_id": id.uuidString])
                    dispatched += 1
                }
                index = end
                // Yield between batches (skip the trailing sleep after the
                // final batch). A cancellation/throw here just stops early —
                // the flag stays unset so the next launch resumes (idempotent).
                if index < transcribedIDs.count {
                    try? await Task.sleep(nanoseconds: Self.knowledgeBackfillBatchDelayNanos)
                }
            }

            // Flag set AFTER the dispatch loop completes (memory:
            // oneshot_migration_flag_after_work). Never before — a crash
            // mid-loop leaves the migration incomplete, but idempotency
            // (index_episode deletes + re-upserts) means the next launch
            // safely re-runs the whole set.
            UserDefaults.standard.set(true, forKey: Self.knowledgeBackfillFlagKey)

            os_log(.info,
                   log: OSLog(subsystem: "io.f7z.podcast", category: "KnowledgeBackfill"),
                   "Knowledge backfill complete — %d episodes indexed into kernel KnowledgeStore",
                   dispatched)
        }
    }
}
