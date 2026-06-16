import Foundation

// MARK: - User follow state (`PodcastSubscription`)

extension AppStateStore {

    /// Podcasts the user actively follows, sorted alphabetically by title.
    /// Feed-less podcasts (Agent Generated, Unknown) are excluded by virtue
    /// of having no `PodcastSubscription` row in the new model — they're
    /// `Podcast`-only. The `feedURL != nil` filter additionally guards against
    /// a stray subscription row pointing at a feed-less show.
    var sortedFollowedPodcasts: [Podcast] {
        let podcastByID = Dictionary(uniqueKeysWithValues: state.podcasts.map { ($0.id, $0) })
        return state.subscriptions
            .compactMap { podcastByID[$0.podcastID] }
            .filter { $0.feedURL != nil }
            .sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
    }

    /// Returns the subscription row for a podcast, or `nil` if the user does
    /// not follow it.
    func subscription(podcastID: UUID) -> PodcastSubscription? {
        state.subscriptions.first { $0.podcastID == podcastID }
    }

    /// Convenience: returns the podcast for an existing subscription row.
    func podcast(for subscription: PodcastSubscription) -> Podcast? {
        podcast(id: subscription.podcastID)
    }

    /// Inserts a follow row for the given podcast. Returns `false` if the
    /// user already follows this podcast. The podcast row must already
    /// exist (tests may call `upsertPodcast`; production should use a kernel
    /// action such as `kernelSubscribe` / `kernelEnsurePodcast`).
    @discardableResult
    func addSubscription(podcastID: UUID) -> Bool {
        guard state.podcasts.contains(where: { $0.id == podcastID }) else { return false }
        guard !state.subscriptions.contains(where: { $0.podcastID == podcastID }) else { return false }
        state.subscriptions.append(PodcastSubscription(podcastID: podcastID))
        return true
    }

    /// Fully removes a podcast — its metadata row, any follow row, and
    /// every episode that belonged to it. Used both by the "Unsubscribe"
    /// destructive action on followed podcasts and by the swipe-to-delete
    /// on the all-podcasts list for podcasts the user never followed.
    func deletePodcast(podcastID: UUID) {
        kernelUnsubscribe(podcastID: podcastID)
        let removedEpisodeIDs = episodes
            .filter { $0.podcastID == podcastID }
            .map(\.id)

        var next = state
        next.subscriptions.removeAll { $0.podcastID == podcastID }
        next.podcasts.removeAll { $0.id == podcastID }
        performMutationBatch {
            state = next
            // Episodes are a separate stored property now — remove them
            // directly rather than through the `next` DTO copy.
            episodes.removeAll { $0.podcastID == podcastID }
            invalidateEpisodeProjections()
        }
    }

    /// Toggles new-episode notifications for a subscribed podcast.
    func setSubscriptionNotificationsEnabled(_ podcastID: UUID, enabled: Bool) {
        guard let idx = state.subscriptions.firstIndex(where: { $0.podcastID == podcastID }) else { return }
        state.subscriptions[idx].notificationsEnabled = enabled
    }

    /// Replaces the per-podcast auto-download policy.
    func setSubscriptionAutoDownload(_ podcastID: UUID, policy: AutoDownloadPolicy) {
        guard let idx = state.subscriptions.firstIndex(where: { $0.podcastID == podcastID }) else { return }
        state.subscriptions[idx].autoDownload = policy
        kernelSetAutoDownload(podcastID: podcastID, policy: policy)
    }
}
