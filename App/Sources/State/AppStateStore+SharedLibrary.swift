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
}
