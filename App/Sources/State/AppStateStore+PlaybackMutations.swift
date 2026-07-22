import Foundation
import Pod0Core

extension AppStateStore {
    /// Marks an episode played through the sole Rust owner.
    func markEpisodePlayed(_ id: UUID) {
        sharedLibrary?.dispatchPlayback(.setCompletion(
            episodeId: EpisodeId(uuid: id),
            completion: .completed(cause: .explicitUserAction)
        ))
        if let episode = episode(id: id),
           case .downloaded = episode.downloadState,
           state.settings.autoDeleteDownloadsAfterPlayed {
            sharedLibrary?.removeDownload(episodeID: id)
        }
    }

    /// Clears progress without completing the episode.
    func resetEpisodeProgress(_ id: UUID) {
        sharedLibrary?.dispatchPlayback(.resetProgress(episodeId: EpisodeId(uuid: id)))
    }

    /// Reverts an explicit completion through the current authoritative owner.
    func markEpisodeUnplayed(_ id: UUID) {
        sharedLibrary?.dispatchPlayback(.setCompletion(
            episodeId: EpisodeId(uuid: id),
            completion: .inProgress
        ))
    }
}
