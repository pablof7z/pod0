import Foundation
import Pod0Core

extension AppStateStore {
    /// Marks an episode played through the Rust owner after cutover. The
    /// fallback remains only for pre-cutover state and is deleted by #83.
    func markEpisodePlayed(_ id: UUID) {
        if isSharedLibraryAuthoritative {
            sharedLibrary?.dispatchPlayback(.setCompletion(
                episodeId: EpisodeId(uuid: id),
                completion: .completed(cause: .explicitUserAction)
            ))
            if let episode = episode(id: id),
               case .downloaded = episode.downloadState,
               state.settings.autoDeleteDownloadsAfterPlayed {
                EpisodeDownloadService.shared.delete(episodeID: id)
            }
            return
        }

        flushPendingPositions()
        guard let index = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        let wasDownloaded: Bool
        if case .downloaded = state.episodes[index].downloadState {
            wasDownloaded = true
        } else {
            wasDownloaded = false
        }
        var episodes = state.episodes
        episodes[index].played = true
        episodes[index].playbackPosition = 0
        performMutationBatch {
            mutateState { $0.episodes = episodes }
            positionCache.removeValue(forKey: id)
            invalidateEpisodeProjections()
        }
        if wasDownloaded, state.settings.autoDeleteDownloadsAfterPlayed {
            EpisodeDownloadService.shared.delete(episodeID: id)
        }
    }

    /// Clears progress without completing the episode.
    func resetEpisodeProgress(_ id: UUID) {
        if isSharedLibraryAuthoritative {
            sharedLibrary?.dispatchPlayback(.resetProgress(episodeId: EpisodeId(uuid: id)))
            return
        }

        flushPendingPositions()
        guard let index = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = state.episodes
        episodes[index].playbackPosition = 0
        performMutationBatch {
            mutateState { $0.episodes = episodes }
            positionCache.removeValue(forKey: id)
            invalidateEpisodeProjections()
        }
    }

    /// Reverts an explicit completion through the current authoritative owner.
    func markEpisodeUnplayed(_ id: UUID) {
        if isSharedLibraryAuthoritative {
            sharedLibrary?.dispatchPlayback(.setCompletion(
                episodeId: EpisodeId(uuid: id),
                completion: .inProgress
            ))
            return
        }

        guard let index = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = state.episodes
        episodes[index].played = false
        performMutationBatch {
            mutateState { $0.episodes = episodes }
            invalidateEpisodeProjections()
        }
    }
}
