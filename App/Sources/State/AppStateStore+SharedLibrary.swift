import Foundation

extension AppStateStore {
    func applySharedLibrary(_ projection: SharedLibrarySnapshot) {
        let existingEpisodes = Dictionary(uniqueKeysWithValues: state.episodes.map { ($0.id, $0) })
        let projectedPodcasts = projection.podcasts.map(\.swiftValue)
        let projectedEpisodes = projection.episodes.compactMap { record in
            record.episodeId.uuid.flatMap { record.swiftValue(preserving: existingEpisodes[$0]) }
        }
        performMutationBatch {
            mutateState {
                $0.podcasts = projectedPodcasts
                $0.subscriptions = projection.subscriptions.map(\.swiftValue)
                $0.episodes = projectedEpisodes
            }
            invalidateEpisodeProjections()
        }
    }

    /// Replaces the native read model from a bounded Rust projection. This is
    /// the only production assignment to `AppState.notes`; Persistence strips
    /// it from metadata once shared note authority is active.
    func applySharedNotes(_ projection: SharedNoteSnapshot) {
        mutateState { $0.notes = projection.notes }
    }

    /// The sole production assignment to the replaceable native clip read model.
    func applySharedClips(_ projection: SharedClipSnapshot) {
        mutateState { $0.clips = projection.clips }
    }
}
