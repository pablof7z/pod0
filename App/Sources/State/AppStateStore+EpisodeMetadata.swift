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

    /// Temporary agent-domain adjunct. Episode identity and durable library
    /// metadata remain core-owned; issue #60 moves this provenance into Rust.
    func setEpisodeGenerationSource(_ id: UUID, source: Episode.GenerationSource?) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        guard state.episodes[idx].generationSource != source else { return }
        mutateState { $0.episodes[idx].generationSource = source }
    }
}
