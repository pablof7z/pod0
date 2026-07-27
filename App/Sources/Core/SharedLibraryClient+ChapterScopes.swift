import Foundation

extension SharedLibraryClient {
    /// Retains one screen-shaped chapter projection. Multiple native views may
    /// retain the same episode; the final release removes the transient copy.
    func openChapterProjection(episodeID: UUID) {
        retainChapterProjection(episodeID: episodeID)
    }

    func closeChapterProjection(episodeID: UUID) {
        releaseChapterProjection(episodeID: episodeID)
    }

    /// Always admits the requested episode. Capacity evicts the coldest scope
    /// rather than refusing this one — refusing left the player rendering its
    /// "no chapters" placeholder over an episode whose chapters were present
    /// and selected, with nothing to retry it.
    func retainChapterProjection(episodeID: UUID) {
        switch chapterScopes.retain(episodeID) {
        case .alreadyRetained:
            return
        case .load(let evicted):
            if let evicted { tearDownChapterProjection(episodeID: evicted) }
            loadChapterProjection(episodeID: episodeID)
        }
    }

    func releaseChapterProjection(episodeID: UUID) {
        guard chapterScopes.release(episodeID) else { return }
        tearDownChapterProjection(episodeID: episodeID)
    }

    private func loadChapterProjection(episodeID: UUID) {
        let reader = authoritativeChapterReader
        chapterProjectionTasks[episodeID]?.cancel()
        chapterProjectionTasks[episodeID] = Task { @MainActor [weak self] in
            let snapshot = await Task.detached(priority: .userInitiated) {
                try? reader.load(episodeID: episodeID)
            }.value
            guard !Task.isCancelled,
                  let self,
                  chapterScopes.isRetained(episodeID)
            else { return }
            chapterProjectionTasks[episodeID] = nil
            guard let snapshot else {
                chapterSnapshots[episodeID] = nil
                store?.clearSharedChapter(episodeID: episodeID)
                return
            }
            chapterSnapshots[episodeID] = snapshot
            store?.applySharedChapter(snapshot)
        }
    }

    private func tearDownChapterProjection(episodeID: UUID) {
        chapterProjectionTasks[episodeID]?.cancel()
        chapterProjectionTasks[episodeID] = nil
        chapterSnapshots[episodeID] = nil
        store?.clearSharedChapter(episodeID: episodeID)
    }
}
