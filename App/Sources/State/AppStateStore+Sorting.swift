import Foundation

// MARK: - Followed-podcast sorting (recency)

extension AppStateStore {

    /// Podcasts the user follows, sorted by their most-recent-episode
    /// `pubDate`, descending.
    ///
    /// Designed for the merged Home subscription list — the user wants to see
    /// the feed that just published an episode at the top, not the one whose
    /// title happens to start with "A". Followed podcasts with no known
    /// episode yet (fresh import, before the first feed fetch) sink to the
    /// bottom and fall back to alphabetical order so the list never
    /// collapses to a random arrangement.
    ///
    /// Compatibility wrapper over the Rust-owned Home subscription projection.
    ///
    /// Feed-less podcasts (Agent Generated, Unknown) are excluded by virtue
    /// of having no `PodcastSubscription` row in the new model — they're
    /// `Podcast`-only and never appear in the user's subscription list.
    var sortedFollowedPodcastsByRecency: [Podcast] {
        let followed = rustFollowedPodcasts()
        guard let envelope = kernel?.homeSubscriptionListEnvelope(
            filter: "all",
            podcastIDs: followed.map(\.id)
        ),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.sortedFollowedRecency.decode(
                SubscriptionListResponse.self,
                from: data
              )
        else { return [] }
        return decoded.podcastIds.compactMap { podcast(id: $0) }
    }

    /// Most-recent episode for the given `podcastID`, or `nil` when the
    /// podcast has no episodes yet.
    func mostRecentEpisode(forPodcast podcastID: UUID) -> Episode? {
        rustLatestEpisode(forPodcast: podcastID)
    }
}

private struct SubscriptionListResponse: Decodable {
    let podcastIds: [UUID]
}

private extension JSONDecoder {
    static let sortedFollowedRecency: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }()
}
