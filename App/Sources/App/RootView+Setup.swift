import SwiftUI

extension RootView {

    /// Wires all playback-state callbacks. Called from `.onAppear` so the
    /// closures reference the live `store` and `playbackState` values that
    /// exist at the point the root view first appears.
    func setupPlaybackHandlers() {
        playbackState.productSignals = store.productSignals
        playbackState.onPersistPosition = { [store] id, position in
            store.setEpisodePlaybackPosition(id, position: position)
            store.setLastPlayedEpisode(id)
        }
        playbackState.onEpisodeFinished = { [store, playbackState] id in
            store.markEpisodePlayed(id)
            let endOfEpisodeArmed: Bool
            switch playbackState.engine.sleepTimer.phase {
            case .armedEndOfEpisode, .fired:
                endOfEpisodeArmed = true
            default:
                endOfEpisodeArmed = false
            }
            guard store.state.settings.autoPlayNext, !endOfEpisodeArmed else { return }
            playbackState.playNext { store.episode(id: $0) }
        }
        playbackState.onFlushPositions = { [store] in
            store.flushPendingPositions()
        }
        playbackState.onEnsureDownloadEnqueued = { [store] id in
            EpisodeDownloadService.shared.attach(appStore: store)
            EpisodeDownloadService.shared.ensureDownloadEnqueued(episodeID: id)
        }
        playbackState.onSegmentFinished = { [store, playbackState] in
            let advanced = playbackState.playNext { store.episode(id: $0) }
            if !advanced {
                playbackState.pause()
            }
        }
        // Cold-launch quick-action routing.
        if let delegate = UIApplication.shared.delegate as? AppDelegate,
           let url = delegate.pendingShortcutURL {
            delegate.pendingShortcutURL = nil
            handleDeepLink(url)
        }
        playbackState.autoMarkPlayedOnFinish = store.state.settings.autoMarkPlayedAtEnd
        playbackState.applyPreferences(from: store.state.settings)
        playbackState.resolveShowName = { [store] episode in
            store.podcast(id: episode.podcastID)?.title ?? ""
        }
        playbackState.resolveShowImage = { [store] episode in
            store.podcast(id: episode.podcastID)?.imageURL
        }
        playbackState.engine.resolveShowName = { [store] episode in
            store.podcast(id: episode.podcastID)?.title
        }
        playbackState.engine.resolveActiveChapterTitle = { [store] episode, playhead in
            let live = store.episode(id: episode.id) ?? episode
            let navigable = live.chapters?.filter(\.includeInTableOfContents) ?? []
            return navigable.active(at: playhead)?.title
        }
        playbackState.engine.resolveArtworkURL = { [store] episode, playhead in
            let live = store.episode(id: episode.id) ?? episode
            let navigable = live.chapters?.filter(\.includeInTableOfContents) ?? []
            if let chapterURL = navigable.active(at: playhead)?.imageURL {
                return chapterURL
            }
            return live.imageURL
                ?? store.podcast(id: live.podcastID)?.imageURL
        }
        playbackState.resolveNavigableChapters = { [store] episode in
            let live = store.episode(id: episode.id) ?? episode
            return live.chapters?.filter(\.includeInTableOfContents) ?? []
        }
        playbackState.onClipRequested = {
            AutoSnipController.shared.captureSnip(source: .headphone)
        }
        AutoSnipController.shared.attach(playback: playbackState, store: store)

        // Restore the last-played episode so the mini-player reappears after
        // an app restart. Loads the episode in a paused state — the user taps
        // play to resume. Only runs when no deep-link or shortcut has already
        // loaded an episode.
        if playbackState.episode == nil,
           let lastID = store.state.lastPlayedEpisodeID,
           let episode = store.episode(id: lastID) {
            playbackState.setEpisode(episode)
        }
    }
}
