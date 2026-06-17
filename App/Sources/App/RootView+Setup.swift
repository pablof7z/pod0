import SwiftUI

extension RootView {

    func setupPlaybackHandlers() {
        // Inject store reference so PlaybackState can dispatch kernel
        // actions directly (queue, triage, download).
        playbackState.store = store

        // Engine metadata resolvers — lock-screen / NowPlayingCenter.
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
            return live.imageURL ?? store.podcast(id: live.podcastID)?.imageURL
        }

        // Render the Up Next queue from the Rust-owned projection.
        let seedQueue: ([QueueItem]) -> Void = { [playbackState] items in
            playbackState.queue = items
        }
        if !store.pendingKernelQueue.isEmpty {
            seedQueue(store.pendingKernelQueue)
            store.pendingKernelQueue = []
        }
        store.onQueueFromKernel = seedQueue

        // Cold-launch quick-action routing.
        if let delegate = UIApplication.shared.delegate as? AppDelegate,
           let url = delegate.pendingShortcutURL {
            delegate.pendingShortcutURL = nil
            handleDeepLink(url)
        }

        playbackState.applyPreferences(from: store.state.settings)

        AutoSnipController.shared.attach(playback: playbackState, store: store)

        // Restore last-played episode so the mini-player reappears on restart.
        if playbackState.episode == nil,
           let lastID = store.state.lastPlayedEpisodeID,
           let episode = store.episode(id: lastID) {
            playbackState.setEpisode(episode)
        }
    }

    func handleShake() {
        let now = Date()
        guard now.timeIntervalSince(lastShakeTime) > 1.0 else { return }
        lastShakeTime = now

        if feedbackWorkflow.phase == .awaitingScreenshot {
            feedbackWorkflow.screenshot = captureScreenshot()
            feedbackWorkflow.phase = .annotating
        } else {
            Haptics.medium()
            feedbackWorkflow.draft = ""
            feedbackWorkflow.screenshot = nil
            feedbackWorkflow.annotatedImage = nil
            feedbackWorkflow.phase = .composing
            showFeedback = true
        }
    }

    func captureScreenshot() -> UIImage? {
        guard
            let windowScene = UIApplication.shared.connectedScenes.first as? UIWindowScene,
            let window = windowScene.windows.first
        else { return nil }
        let renderer = UIGraphicsImageRenderer(bounds: window.bounds)
        return renderer.image { ctx in window.layer.render(in: ctx.cgContext) }
    }
}
