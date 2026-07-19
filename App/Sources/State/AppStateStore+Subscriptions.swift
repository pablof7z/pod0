import Foundation
import Pod0Core

// MARK: - User follow state (`PodcastSubscription`)

extension AppStateStore {

    /// Podcasts the user actively follows, sorted alphabetically by title.
    /// Synthetic podcasts (Agent Generated, Unknown) are excluded by virtue
    /// of having no `PodcastSubscription` row in the new model — they're
    /// `Podcast`-only.
    var sortedFollowedPodcasts: [Podcast] {
        let podcastByID = Dictionary(uniqueKeysWithValues: state.podcasts.map { ($0.id, $0) })
        return state.subscriptions
            .compactMap { podcastByID[$0.podcastID] }
            .filter { $0.kind == .rss }
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

    /// Fully removes a podcast — its metadata row, any follow row, and
    /// every episode that belonged to it. Used both by the "Unsubscribe"
    /// destructive action on followed podcasts and by the swipe-to-delete
    /// on the all-podcasts list for podcasts the user never followed.
    func deletePodcast(podcastID: UUID) {
        Task { @MainActor [weak self] in
            try? await self?.deletePodcastAndWait(podcastID: podcastID)
        }
    }

    /// Executes a deletion through the authoritative core and returns
    /// only after the resulting projection has replaced the Swift read model.
    func deletePodcastAndWait(podcastID: UUID) async throws {
        guard let sharedLibrary else { throw SharedLibraryError.unavailable }
        _ = try await sharedLibrary.execute(.unsubscribe(
            podcastId: PodcastId(uuid: podcastID)
        ))
    }

    /// Toggles new-episode notifications for a subscribed podcast.
    func setSubscriptionNotificationsEnabled(_ podcastID: UUID, enabled: Bool) {
        Task { @MainActor [weak self] in
            try? await self?.setSubscriptionNotificationsAndWait(
                podcastID,
                enabled: enabled
            )
        }
    }

    func setSubscriptionNotificationsAndWait(_ podcastID: UUID, enabled: Bool) async throws {
        guard let sharedLibrary else { throw SharedLibraryError.unavailable }
        _ = try await sharedLibrary.execute(.setSubscriptionNotifications(
            podcastId: PodcastId(uuid: podcastID),
            enabled: enabled
        ))
    }

    /// Replaces the per-podcast auto-download policy.
    func setSubscriptionAutoDownload(_ podcastID: UUID, policy: AutoDownloadPolicy) {
        Task { @MainActor [weak self] in
            try? await self?.setSubscriptionAutoDownloadAndWait(
                podcastID,
                policy: policy
            )
        }
    }

    func setSubscriptionAutoDownloadAndWait(
        _ podcastID: UUID,
        policy: AutoDownloadPolicy
    ) async throws {
        guard let sharedLibrary else { throw SharedLibraryError.unavailable }
        _ = try await sharedLibrary.execute(.setSubscriptionAutoDownload(
            podcastId: PodcastId(uuid: podcastID),
            policy: policy.coreValue
        ))
    }
}
