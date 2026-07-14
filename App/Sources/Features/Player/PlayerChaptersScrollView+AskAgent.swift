import Foundation

// MARK: - Ask-the-agent dispatcher (chapter flavour)
//
// Long-press on a chapter row in `PlayerChaptersScrollView` writes a
// `ChapterAgentContext` into the store and posts `.askAgentRequested`, which
// `RootView` observes to present the agent chat sheet. The chapter title plus
// `[startTime, endTime]` are all the agent needs — it pulls the transcript
// window itself via its tool inventory; the user never sees raw transcript
// text.
//
// Mirrors `AskAgentDispatcher` (transcript-segment flavour) so the two paths
// can co-exist while transcript-segment dispatches stay internal to clip /
// quote surfaces.

enum ChapterAskAgentDispatcher {

    /// Builds the `ChapterAgentContext` for the long-pressed chapter and
    /// publishes it. `chapters` is the same list visible in the player rail
    /// — used to resolve the chapter's implicit `endTime` as the next
    /// chapter's `startTime` when the row itself has no explicit end.
    /// Silently no-ops if `episode` is missing.
    @MainActor
    static func dispatch(
        chapter: Episode.Chapter,
        in chapters: [Episode.Chapter],
        episode: Episode?,
        store: AppStateStore
    ) {
        guard let episode else { return }
        let title = store.podcast(id: episode.podcastID)?.title ?? ""
        let resolvedEnd = chapter.endTime ?? Self.nextChapterStart(after: chapter, in: chapters)
        let context = ChapterAgentContext(
            episodeID: episode.id,
            subscriptionTitle: title,
            episodeTitle: episode.title,
            chapterTitle: chapter.title,
            startTime: chapter.startTime,
            endTime: resolvedEnd
        )
        store.pendingChapterAgentContext = context
        Haptics.light()
        NotificationCenter.default.post(name: .askAgentRequested, object: nil)
    }

    /// `startTime` of the chapter that follows `chapter` in `chapters`.
    /// Falls back to `nil` when `chapter` is the last entry — the agent can
    /// still scope to a half-open window in that case.
    static func nextChapterStart(
        after chapter: Episode.Chapter,
        in chapters: [Episode.Chapter]
    ) -> TimeInterval? {
        guard let idx = chapters.firstIndex(where: { $0.id == chapter.id }) else {
            return nil
        }
        let next = chapters.index(after: idx)
        return next < chapters.endIndex ? chapters[next].startTime : nil
    }
}
