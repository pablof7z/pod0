import Foundation
import Pod0Core

extension AppStateStore {
    /// Flips the user-set "starred" flag for an episode.
    func toggleEpisodeStarred(_ id: UUID) {
        guard let episode = episode(id: id) else { return }
        setEpisodeStarred(id, !episode.isStarred)
    }

    /// Sets the user-set "starred" flag explicitly.
    func setEpisodeStarred(_ id: UUID, _ starred: Bool) {
        Task { @MainActor [weak self] in
            try? await self?.setEpisodeStarredAndWait(id, starred)
        }
    }

    func setEpisodeStarredAndWait(_ id: UUID, _ starred: Bool) async throws {
        guard let sharedLibrary else { throw SharedLibraryError.unavailable }
        _ = try await sharedLibrary.execute(.setEpisodeStarred(
            episodeId: EpisodeId(uuid: id),
            starred: starred
        ))
    }
}
