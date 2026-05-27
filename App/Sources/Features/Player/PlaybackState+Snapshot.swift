import Foundation

// MARK: - Now Playing snapshot

extension PlaybackState {

    /// Write the current episode metadata into the App Group `UserDefaults` the
    /// widget reads from. `NowPlayingSnapshotStore.write` handles the WidgetKit
    /// timeline reload. Throttled to once per 5 s unless `force` is set.
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
    }
}
