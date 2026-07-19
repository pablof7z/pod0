import Foundation
@testable import Podcastr

/// Test-target-only projection fixtures for domains whose production Swift
/// writers were deleted after Rust cutover. These helpers never ship in the
/// app and must not be used to test listening policy itself.
@MainActor
extension AppStateStore {
    @discardableResult
    func installPodcastFixture(_ podcast: Podcast) -> Podcast {
        if let index = state.podcasts.firstIndex(where: { $0.id == podcast.id }) {
            mutateState { $0.podcasts[index] = podcast }
        } else {
            mutateState { $0.podcasts.append(podcast) }
        }
        return podcast
    }

    @discardableResult
    func installSubscriptionFixture(podcastID: UUID) -> Bool {
        installSubscriptionFixture(PodcastSubscription(podcastID: podcastID))
    }

    @discardableResult
    func installSubscriptionFixture(_ subscription: PodcastSubscription) -> Bool {
        guard !state.subscriptions.contains(where: {
            $0.podcastID == subscription.podcastID
        }) else { return false }
        mutateState { $0.subscriptions.append(subscription) }
        return true
    }

    @discardableResult
    func installEpisodeFixtures(
        _ incoming: [Episode],
        forPodcast podcastID: UUID
    ) -> [UUID] {
        var episodes = state.episodes
        var inserted: [UUID] = []
        for episode in incoming {
            if let index = episodes.firstIndex(where: {
                $0.podcastID == podcastID && $0.guid == episode.guid
            }) {
                episodes[index] = episode
            } else {
                episodes.append(episode)
                inserted.append(episode.id)
            }
        }
        mutateState { $0.episodes = episodes }
        invalidateEpisodeProjections()
        return inserted
    }
}
