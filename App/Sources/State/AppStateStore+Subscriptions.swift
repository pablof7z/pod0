import Foundation

// MARK: - User follow state (`PodcastSubscription`)

extension AppStateStore {

    /// Podcasts the user actively follows. Rust owns follow membership,
    /// feed-backed eligibility, and alphabetical ordering; Swift resolves ids
    /// for native rendering and OPML export.
    var sortedFollowedPodcasts: [Podcast] {
        rustFollowedPodcasts()
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

    /// Fully removes a podcast through the Rust kernel. The snapshot
    /// projection removes the podcast row, follow row, and episodes from Swift.
    func deletePodcast(podcastID: UUID) {
        kernelUnsubscribe(podcastID: podcastID)
    }

    /// Toggles new-episode notifications for a subscribed podcast.
    func setSubscriptionNotificationsEnabled(_ podcastID: UUID, enabled: Bool) {
        kernel?.dispatch(namespace: "podcast",
                         body: [
                            "op": "set_podcast_notifications_enabled",
                            "podcast_id": podcastID.uuidString,
                            "enabled": enabled,
                         ])
    }

    /// Replaces the per-podcast auto-download policy.
    func setSubscriptionAutoDownload(_ podcastID: UUID, policy: AutoDownloadPolicy) {
        kernelSetAutoDownload(podcastID: podcastID, policy: policy)
    }
}
