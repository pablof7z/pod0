import Foundation
import WidgetKit

// MARK: - AppStateStore + Mutation batching

extension AppStateStore {

    /// Central `state.didSet` handler for the cold domains (settings,
    /// subscriptions, podcasts, nostr, agent, …). These mutate rarely, so the
    /// only side effect is persistence. Episode projections are *not* rebuilt
    /// here — episodes live in their own stored property whose `didSet`
    /// (`handleEpisodesDidSet`) owns projection invalidation.
    ///
    /// Most mutations should persist immediately, but import/refresh flows can
    /// wrap many edits in `performMutationBatch` so the expensive save runs
    /// once after the batch lands.
    func handleStateDidSet() {
        markStateSideEffectsDirty()
    }

    /// `episodes.didSet` handler. Episodes are the hot field: any change can
    /// affect the precomputed projections (unplayed counts, download/transcript
    /// presence, in-progress + recent feeds, triage roll-ups), so rebuild them
    /// when the array fingerprint changes, and persist either way (a per-element
    /// edit that leaves the fingerprint unchanged — e.g. a played flag flip on a
    /// stable array — still needs to reach disk, and the dedicated writers in
    /// `+Episodes` call `invalidateEpisodeProjections()` themselves for the
    /// projection side).
    func handleEpisodesDidSet(previousEpisodes: [Episode]) {
        if Self.episodesFingerprintChanged(previousEpisodes, episodes) {
            markEpisodeProjectionsDirty()
        }
        markStateSideEffectsDirty()
    }

    func performMutationBatch(_ body: () -> Void) {
        mutationBatchDepth += 1
        defer {
            mutationBatchDepth -= 1
            if mutationBatchDepth == 0 {
                flushDeferredMutationWork()
            }
        }
        body()
    }

    func markEpisodeProjectionsDirty() {
        if mutationBatchDepth > 0 {
            deferredEpisodeProjectionRebuild = true
        } else {
            recomputeEpisodeProjections()
        }
    }

    private func markStateSideEffectsDirty() {
        if mutationBatchDepth > 0 {
            deferredStateSideEffects = true
        } else {
            runStateSideEffects()
        }
    }

    private func flushDeferredMutationWork() {
        if deferredEpisodeProjectionRebuild {
            deferredEpisodeProjectionRebuild = false
            recomputeEpisodeProjections()
        }
        if deferredStateSideEffects {
            deferredStateSideEffects = false
            runStateSideEffects()
        }
    }

    private func runStateSideEffects() {
        // Episodes live outside `state` at runtime (their own `@Observable`
        // stored property). Re-compose the full `AppState` DTO at the save seam
        // so persistence still writes a complete snapshot — the SQLite episode
        // sidecar and the metadata JSON both read `snapshot.episodes`.
        var snapshot = state
        snapshot.episodes = episodes
        persistence.save(snapshot)
        scheduleWidgetReload()
    }

    /// Trailing-debounce `WidgetCenter.reloadAllTimelines()`. Bursts of
    /// state mutations (refresh round upserting episodes, mark-many-
    /// played, OPML import) collapse to a single reload signal so we
    /// don't burn WidgetKit's daily reload budget.
    func scheduleWidgetReload(delay: Duration = .milliseconds(500)) {
        widgetReloadTask?.cancel()
        widgetReloadTask = Task {
            do {
                try await Task.sleep(for: delay)
            } catch {
                return
            }
            guard !Task.isCancelled else { return }
            WidgetCenter.shared.reloadAllTimelines()
        }
    }
}
