import Foundation

extension AppStateStore {
    func applySharedLibrary(_ projection: SharedLibrarySnapshot) {
        guard isSharedLibraryAuthoritative else { return }
        let existingEpisodes = Dictionary(uniqueKeysWithValues: state.episodes.map { ($0.id, $0) })
        let projectedPodcasts = projection.podcasts.map(\.swiftValue)
        let projectedPodcastIDs = Set(projectedPodcasts.map(\.id))
        let preservedPodcasts = state.podcasts.filter {
            $0.kind == .synthetic && !projectedPodcastIDs.contains($0.id)
        }
        let projectedEpisodes = projection.episodes.compactMap { record in
            record.episodeId.uuid.flatMap { record.swiftValue(preserving: existingEpisodes[$0]) }
        }
        let preservedPodcastIDs = Set(preservedPodcasts.map(\.id))
        let preservedEpisodes = state.episodes.filter {
            preservedPodcastIDs.contains($0.podcastID)
                || $0.podcastID == Podcast.unknownID
        }
        performMutationBatch {
            isApplyingSharedLibraryProjection = true
            defer { isApplyingSharedLibraryProjection = false }
            mutateState {
                $0.podcasts = projectedPodcasts + preservedPodcasts
                $0.subscriptions = projection.subscriptions.map(\.swiftValue)
                $0.episodes = projectedEpisodes + preservedEpisodes
            }
            invalidateEpisodeProjections()
        }
    }
}
