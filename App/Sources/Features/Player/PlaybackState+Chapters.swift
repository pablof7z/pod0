import Foundation

// MARK: - Chapter navigation

extension PlaybackState {

    /// Above this many seconds into the current chapter, "previous chapter"
    /// restarts the current chapter instead of stepping back one.
    static let previousChapterRestartThreshold: TimeInterval = 3.0

    /// Jump to the next chapter's `startTime` in the supplied list. No-op
    /// when there is no next chapter (we're already in the last one).
    /// `navigable` is passed in by the UI so it stays in sync with whatever
    /// the live store reports (chapters can hydrate after playback starts —
    /// see `ChaptersHydrationService`).
    func seekToNextChapter(in navigable: [Episode.Chapter]) {
        guard let next = Self.nextChapter(after: currentTime, in: navigable) else { return }
        engine.seek(to: next.startTime)
        Haptics.selection()
        persistAndFlushAfterUserSeek()
    }

    /// Jump to the previous chapter's `startTime`, applying the iOS Music
    /// pattern: if the current chapter started more than
    /// `previousChapterRestartThreshold` seconds ago, restart the current
    /// chapter instead of going further back. This matches the user's
    /// muscle memory for "previous track."
    func seekToPreviousChapter(in navigable: [Episode.Chapter]) {
        guard let target = Self.previousChapter(
            from: currentTime,
            in: navigable,
            restartThreshold: Self.previousChapterRestartThreshold
        ) else { return }
        engine.seek(to: target.startTime)
        Haptics.selection()
        persistAndFlushAfterUserSeek()
    }

    /// Pure helper: chapter strictly after `playhead`, or nil when there
    /// isn't one. Exposed as `static nonisolated` so tests can drive it
    /// without spinning up the audio engine and without inheriting
    /// `PlaybackState`'s `@MainActor` isolation.
    nonisolated static func nextChapter(after playhead: TimeInterval, in chapters: [Episode.Chapter]) -> Episode.Chapter? {
        chapters.first(where: { $0.startTime > playhead })
    }

    /// Pure helper: chapter to seek to when the user requests "previous."
    /// Returns the current chapter (restart) when `playhead` is more than
    /// `restartThreshold` seconds into it; otherwise the chapter strictly
    /// before. Returns the first chapter when there is no earlier one.
    nonisolated static func previousChapter(
        from playhead: TimeInterval,
        in chapters: [Episode.Chapter],
        restartThreshold: TimeInterval
    ) -> Episode.Chapter? {
        guard let current = chapters.active(at: playhead) else { return nil }
        let elapsed = playhead - current.startTime
        if elapsed > restartThreshold {
            return current
        }
        guard let idx = chapters.firstIndex(where: { $0.id == current.id }), idx > 0 else {
            return current
        }
        return chapters[idx - 1]
    }
}
