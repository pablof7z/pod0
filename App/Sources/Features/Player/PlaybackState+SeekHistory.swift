import Foundation

// MARK: - Seek History ("browser back")

/// A snapshot of where playback was before a navigational jump.
struct SeekHistoryEntry {
    let id = UUID()
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
        clearJumpForward()
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
    /// (and episode for cross-episode jumps). The position being left becomes
    /// available briefly through `jumpForward()` in case the jump was
    /// accidental.
    func jumpBack() {
        guard let entry = seekHistory.popLast() else { return }
        if let episode {
            let forward = SeekHistoryEntry(
                episodeID: episode.id,
                position: engine.currentTime,
                episode: episode
            )
            offerJumpForward(forward)
        }
        restore(entry)
    }

    /// Returns to the position captured immediately before `jumpBack()`.
    /// Consuming the offer also restores the current location to the back
    /// stack, preserving the familiar back/forward relationship.
    func jumpForward() {
        guard let entry = jumpForwardEntry else { return }
        clearJumpForward()
        if let episode {
            seekHistory.append(
                SeekHistoryEntry(
                    episodeID: episode.id,
                    position: engine.currentTime,
                    episode: episode
                )
            )
            if seekHistory.count > 20 { seekHistory.removeFirst() }
        }
        restore(entry)
    }

    private func restore(_ entry: SeekHistoryEntry) {
        let wasPlaying = isPlaying
        if entry.episodeID != episode?.id {
            setEpisode(entry.episode)
            if wasPlaying { play() }
        }
        seek(to: entry.position)
    }

    private func offerJumpForward(_ entry: SeekHistoryEntry) {
        jumpForwardExpiryTask?.cancel()
        jumpForwardEntry = entry
        jumpForwardExpiryTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(6))
            guard !Task.isCancelled, self?.jumpForwardEntry?.id == entry.id else { return }
            self?.jumpForwardEntry = nil
            self?.jumpForwardExpiryTask = nil
        }
    }

    private func clearJumpForward() {
        jumpForwardExpiryTask?.cancel()
        jumpForwardExpiryTask = nil
        jumpForwardEntry = nil
    }
}
