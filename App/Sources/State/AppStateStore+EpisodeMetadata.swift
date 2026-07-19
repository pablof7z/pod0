import Foundation

extension AppStateStore {
    /// Flips the user-set "starred" flag for an episode.
    func toggleEpisodeStarred(_ id: UUID) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        var episodes = state.episodes
        episodes[idx].isStarred.toggle()
        performMutationBatch {
            mutateState { $0.episodes = episodes }
        }
    }

    /// Sets the user-set "starred" flag explicitly.
    func setEpisodeStarred(_ id: UUID, _ starred: Bool) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        guard state.episodes[idx].isStarred != starred else { return }
        var episodes = state.episodes
        episodes[idx].isStarred = starred
        performMutationBatch {
            mutateState { $0.episodes = episodes }
        }
    }

    /// Records the most-recently-loaded episode so the mini-player can be
    /// restored after an app restart. No-op when the value is unchanged.
    func setLastPlayedEpisode(_ id: UUID) {
        guard !isSharedLibraryAuthoritative else { return }
        guard state.lastPlayedEpisodeID != id else { return }
        mutateState { $0.lastPlayedEpisodeID = id }
    }

    /// Applies the stable projection of a verified chapter artifact. No-op
    /// when an empty result would overwrite real chapter data.
    func setEpisodeChapters(_ id: UUID, chapters: [Episode.Chapter]) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
        if chapters.isEmpty, let existing = state.episodes[idx].chapters, !existing.isEmpty {
            return
        }
        let projected = chapters.isEmpty ? nil : chapters
        guard state.episodes[idx].chapters != projected else { return }
        var episodes = state.episodes
        episodes[idx].chapters = projected
        mutateState { $0.episodes = episodes }
    }
}
