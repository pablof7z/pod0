import Foundation
import WidgetKit

// MARK: - Now Playing snapshot

extension PlaybackState {

    /// Write the current episode metadata into the App Group `UserDefaults` the
    /// widget reads from, then nudge WidgetKit to refresh. Throttled to once per
    /// 5 s unless `force` is set (e.g. on episode change), where the snapshot
    /// must update immediately.
    func writeNowPlayingSnapshot(force: Bool) {
        guard let episode else { return }
        let now = Date()
        if !force, let last = lastSnapshotWrite,
           now.timeIntervalSince(last) < 5 {
            return
        }
        let snapshot = NowPlayingSnapshot(
            episodeTitle: episode.title,
            showName: resolveShowName(episode),
            imageURLString: episode.imageURL?.absoluteString,
            position: engine.currentTime,
            duration: duration,
            chapterTitle: engine.resolveActiveChapterTitle(episode, engine.currentTime),
            isPlaying: isPlaying
        )
        NowPlayingSnapshotStore.write(snapshot)
        lastSnapshotWrite = now
        WidgetCenter.shared.reloadAllTimelines()
    }
}
