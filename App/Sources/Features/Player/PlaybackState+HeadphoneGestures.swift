import Foundation

// MARK: - Headphone gestures
//
// Routes AirPods double/triple-tap (and any other surface that emits
// `MPRemoteCommandCenter.nextTrackCommand` / `.previousTrackCommand`) into
// the user-configured `HeadphoneGestureAction`. The action enum lives on
// `Settings`; the gesture-to-action selection lives on `PlaybackState`
// (mirrored in `applyPreferences(from:)`); this dispatcher is the bridge.

extension PlaybackState {

    /// Dispatch a configured `HeadphoneGestureAction`. Called from both
    /// remote-command paths (`nextTrack` / `previousTrack`) — the underlying
    /// gesture decides which Settings field selects the action, but the
    /// dispatch logic is identical. Chapter-aware actions fall back to the
    /// matching skip when the current episode has no navigable chapters.
    func performHeadphoneGesture(_ action: HeadphoneGestureAction) {
        switch action {
        case .skipForward:
            skipForward()
        case .skipBackward:
            skipBackward()
        case .nextChapter:
            let chapters = episode.map(resolveNavigableChapters) ?? []
            if chapters.isEmpty {
                skipForward()
            } else {
                seekToNextChapter(in: chapters)
            }
        case .previousChapter:
            let chapters = episode.map(resolveNavigableChapters) ?? []
            if chapters.isEmpty {
                skipBackward()
            } else {
                seekToPreviousChapter(in: chapters)
            }
        case .clipNow:
            onClipRequested()
        case .none:
            break
        }
    }
}
