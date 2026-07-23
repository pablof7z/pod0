import SwiftUI

extension RootView {

    /// Attaches the Rust playback owner and native-only presentation adapters.
    /// Called from `.onAppear` after the live store and player exist.
    func setupPlaybackHandlers() {
        store.sharedLibrary?.attachPlayback(playbackState, store: store)
        store.sharedLibrary?.attachAgent(
            approvalPresenter: approvalCoordinator,
            store: store
        )
        playbackState.productSignals = store.productSignals
        playbackState.onEnsureDownloadEnqueued = { [store] id in
            store.sharedLibrary?.requestDownload(episodeID: id, origin: .playback)
        }
        // Cold-launch quick-action routing.
        if let delegate = UIApplication.shared.delegate as? AppDelegate,
           let url = delegate.pendingShortcutURL {
            delegate.pendingShortcutURL = nil
            handleDeepLink(url)
        }
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

    }
}
