import Foundation

// MARK: - AppStateStore + ad segments
//
// Lives in its own file (rather than `AppStateStore+Episodes.swift`) so the
// ad-detection feature stays self-contained. Persists the detector's output
// into `Episode.adSegments` via the same `performMutationBatch` discipline
// the chapters / transcript-state setters use.

extension AppStateStore {

    /// Persist the detected ad spans for an episode. Pass an empty array to
    /// signal "detection ran, found no ads" (distinct from `nil` which means
    /// detection hasn't been run yet). No-op when the episode is missing
    /// from the store.
    @MainActor
    func setEpisodeAdSegments(
        _ id: UUID,
        segments: [Episode.AdSegment]
    ) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = state.episodes
        episodes[idx].adSegments = segments
        state.episodes = episodes
    }
}
