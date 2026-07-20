import Foundation

extension PlaybackState {
    func seekToNextChapter() {
        guard let sharedCore, let chapterContext else { return }
        sharedCore.dispatchPlayback(.nextChapter(
            context: chapterContext,
            positionMilliseconds: Self.coreMilliseconds(currentTime)
        ))
        Haptics.selection()
    }

    func seekToPreviousChapter() {
        guard let sharedCore, let chapterContext else { return }
        sharedCore.dispatchPlayback(.previousChapter(
            context: chapterContext,
            positionMilliseconds: Self.coreMilliseconds(currentTime)
        ))
        Haptics.selection()
    }
}
