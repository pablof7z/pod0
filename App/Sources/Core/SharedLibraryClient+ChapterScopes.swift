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

    func retainChapterProjection(episodeID: UUID) {
        guard chapterScopeCounts[episodeID] != nil
                || chapterScopeCounts.count < Self.maximumActiveChapterProjections else {
            return
        }
        chapterScopeCounts[episodeID, default: 0] += 1
        guard chapterScopeCounts[episodeID] == 1 else { return }
        guard let snapshot = try? authoritativeChapterReader.load(episodeID: episodeID) else {
            chapterSnapshots[episodeID] = nil
            store?.clearSharedChapter(episodeID: episodeID)
            return
        }
        chapterSnapshots[episodeID] = snapshot
        store?.applySharedChapter(snapshot)
    }

    func releaseChapterProjection(episodeID: UUID) {
        guard let count = chapterScopeCounts[episodeID] else { return }
        if count > 1 {
            chapterScopeCounts[episodeID] = count - 1
            return
        }
        chapterScopeCounts[episodeID] = nil
        chapterSnapshots[episodeID] = nil
        store?.clearSharedChapter(episodeID: episodeID)
    }
}
