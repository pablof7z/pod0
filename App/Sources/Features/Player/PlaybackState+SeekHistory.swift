import Foundation

// MARK: - Seek History ("browser back")

/// A snapshot of where playback was before a navigational jump.
struct SeekHistoryEntry {
    let episodeID: UUID
    let position: TimeInterval
    let episode: Episode
}

extension PlaybackState {

    /// Seeks to `time` and pushes the current (episode, playhead) onto the
    /// back stack. Use for intentional navigation jumps — chapter taps,
    /// clip taps, agent seeks, deep-link seeks — so the user can return
    /// via `jumpBack()`. Skips pushing when the move is less than 2 s so
    /// accidental near-taps don't pollute the stack.
    func navigationalSeek(to time: TimeInterval) {
        guard let episode else { seek(to: time); return }
        let current = engine.currentTime
        if abs(current - time) > 2.0 {
            let entry = SeekHistoryEntry(
                episodeID: episode.id,
                position: current,
                episode: episode
            )
            seekHistory.append(entry)
            if seekHistory.count > 20 { seekHistory.removeFirst() }
        }
        seek(to: time)
    }

    /// Pops the most recent history entry and restores the playhead
    /// (and episode for cross-episode jumps). Mirrors browser-back semantics.
    func jumpBack() {
        guard let entry = seekHistory.popLast() else { return }
        let wasPlaying = isPlaying
        if entry.episodeID != episode?.id {
            setEpisode(entry.episode)
            if wasPlaying { play() }
        }
        seek(to: entry.position)
    }
}
