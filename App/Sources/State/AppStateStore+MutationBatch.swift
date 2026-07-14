import Foundation
import WidgetKit

// MARK: - AppStateStore + Mutation batching

extension AppStateStore {

    /// Central `state.didSet` handler. Most mutations should persist and
    /// refresh derived indexes immediately, but import/refresh flows can wrap
    /// many state edits in `performMutationBatch` so the expensive work runs
    /// once after the batch lands.
    func handleStateDidSet(previousEpisodes: [Episode]) {
        if Self.episodesFingerprintChanged(previousEpisodes, state.episodes) {
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
        let snapshot = state
        persistence.save(snapshot)
        scheduleWidgetReload()
        iCloudSettingsSync.shared.push(state.settings)
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
