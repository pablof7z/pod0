import Foundation

// MARK: - Kernel-backed mutation entry points
//
// All domain mutations route through these methods. Each delegates to
// `kernel.dispatch`, which (1) synchronously enqueues the action in Rust,
// (2) calls `pullPodcastSnapshotIfChanged` immediately, and (3) triggers the
// `withObservationTracking` listener in `attachKernel` so `AppState` updates
// before the next frame.
//
// Namespaces (verified against apps/nmp-app-podcast/src/ffi/actions/):
//   "podcast"          – subscribe, unsubscribe, refresh/refresh_all,
//                        download, delete_download, star_episode
//   "podcast.inbox"    – mark_listened
//   "podcast.player"   – cancel_download

extension AppStateStore {

    // MARK: - Subscription / library

    /// Subscribe to a feed URL. Dispatches to Rust and waits (up to
    /// `timeout`) for the new podcast to appear in the projected state.
    /// Preserves the `throws Podcast` signature that `AddShowSheet`,
    /// `DiscoverSearchForm`, and `OPMLImportSheet` depend on.
    @discardableResult
    func kernelSubscribe(feedURL: String,
                         timeout: Duration = .seconds(30)) async throws -> Podcast {
        guard let kern = kernel else {
            throw SubscriptionService.AddError.transport("Kernel not available")
        }
        let trimmed = feedURL.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let url = URL(string: trimmed) else {
            throw SubscriptionService.AddError.invalidURL
        }
        if let existing = podcast(feedURL: url),
           subscription(podcastID: existing.id) != nil {
            throw SubscriptionService.AddError.alreadySubscribed(title: existing.title)
        }
        kern.dispatch(namespace: "podcast", body: ["op": "subscribe", "feed_url": trimmed])
        let deadline = ContinuousClock.now + timeout
        while ContinuousClock.now < deadline {
            if let p = podcast(feedURL: url),
               subscription(podcastID: p.id) != nil { return p }
            try await Task.sleep(for: .milliseconds(300))
        }
        throw SubscriptionService.AddError.transport(
            "Feed did not appear in library after \(timeout). It may still arrive.")
    }

    /// Unsubscribe from a podcast and remove it from the library.
    func kernelUnsubscribe(podcastID: UUID) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "unsubscribe", "podcast_id": podcastID.uuidString])
    }

    /// Trigger a full feed refresh for every subscription.
    func kernelRefreshAll() {
        kernel?.dispatch(namespace: "podcast", body: ["op": "refresh_all"])
    }

    /// Refresh a single podcast feed.
    func kernelRefresh(podcastID: UUID) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "refresh", "podcast_id": podcastID.uuidString])
    }

    // MARK: - Episode state

    /// Mark an episode as fully played (namespace: podcast.inbox).
    func kernelMarkPlayed(_ id: UUID) {
        kernel?.dispatch(namespace: "podcast.inbox",
                         body: ["op": "mark_listened", "episode_id": id.uuidString])
    }

    /// Toggle the starred flag for an episode (namespace: podcast).
    /// Pass the current starred state so Rust sets it explicitly rather than
    /// toggling from potentially-stale kernel state.
    func kernelToggleStar(_ id: UUID, currentlyStarred: Bool) {
        kernel?.dispatch(namespace: "podcast",
                         body: [
                             "op": "star_episode",
                             "episode_id": id.uuidString,
                             "starred": !currentlyStarred,
                         ])
    }

    // MARK: - Queue (podcast.queue namespace)

    /// Push an episode to the back of the Rust-owned Up Next queue.
    func kernelEnqueueLast(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast.queue",
                         body: ["op": "add_last", "episode_id": episodeID.uuidString])
    }

    /// Push an episode to the front of the Rust-owned Up Next queue (Play Next).
    func kernelEnqueueNext(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast.queue",
                         body: ["op": "add_next", "episode_id": episodeID.uuidString])
    }

    /// Remove all occurrences of an episode from the Rust-owned Up Next queue.
    func kernelDequeueEpisode(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast.queue",
                         body: ["op": "remove", "episode_id": episodeID.uuidString])
    }

    /// Empty the Rust-owned Up Next queue.
    func kernelClearQueue() {
        kernel?.dispatch(namespace: "podcast.queue", body: ["op": "clear"])
    }

    // MARK: - Chapters

    /// Fetch Podcasting 2.0 chapters JSON for an episode (namespace: podcast).
    /// Rust fetches the `chaptersURL`, parses the JSON, stores the results in
    /// `PodcastStore`, increments the snapshot rev, and the next projection
    /// tick maps them onto `Episode.chapters` via `applyKernelState`.
    func kernelFetchChapters(episodeID: UUID) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "fetch_chapters", "episode_id": episodeID.uuidString])
    }

    // MARK: - Subscription settings

    /// Update the auto-download policy for a single podcast (namespace: podcast).
    /// Rust treats this as a simple boolean; iOS `.latestN` and `.allNew`
    /// both map to `enabled: true` since the Rust store records only on/off.
    func kernelSetAutoDownload(podcastID: UUID, enabled: Bool) {
        kernel?.dispatch(namespace: "podcast",
                         body: [
                             "op": "set_auto_download",
                             "podcast_id": podcastID.uuidString,
                             "enabled": enabled
                         ])
    }

    // MARK: - Downloads

    /// Queue a download (namespace: podcast).
    func kernelDownload(_ id: UUID) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "download", "episode_id": id.uuidString])
    }

    /// Cancel an in-progress or queued download (namespace: podcast.player).
    func kernelCancelDownload(_ id: UUID) {
        kernel?.dispatch(namespace: "podcast.player",
                         body: ["op": "cancel_download", "episode_id": id.uuidString])
    }

    /// Delete a downloaded episode file (namespace: podcast).
    func kernelDeleteDownload(_ id: UUID) {
        kernel?.dispatch(namespace: "podcast",
                         body: ["op": "delete_download", "episode_id": id.uuidString])
    }
}
