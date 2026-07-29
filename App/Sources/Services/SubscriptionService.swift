import Foundation
import Pod0Core
import os.log

/// Routes library intents through the sole Rust owner. Feed networking remains
/// native through `CoreFeedHost`; parsing, normalization, identity, retry, and
/// subscription policy stay behind the typed application facade. Subscribing
/// commits durably before any fetch: success here means "durably queued", and
/// fetch progress is rendered from the `feedFetches` projection.
@MainActor
struct SubscriptionService {
    private static let logger = Logger.app("SubscriptionService")
    let store: AppStateStore

    init(store: AppStateStore) {
        self.store = store
    }

    enum AddError: Error, LocalizedError, Equatable {
        case invalidURL
        case alreadySubscribed(title: String)
        case transport(String)
        case parse(String)

        var errorDescription: String? {
            switch self {
            case .invalidURL:
                "That doesn't look like a valid feed URL."
            case .alreadySubscribed(let title):
                "You're already subscribed to \(title)."
            case .transport:
                "Couldn't reach the feed. Check your connection and try again."
            case .parse:
                "Pod0 couldn't read a podcast feed at that address."
            }
        }
    }

    @discardableResult
    func ensurePodcast(feedURLString: String) async throws -> Podcast {
        let result = try await executeShared(.ensurePodcast(feedUrl: feedURLString))
        return try await resolvedPodcast(from: result)
    }

    @discardableResult
    func addSubscription(feedURLString: String) async throws -> Podcast {
        let result = try await executeShared(.subscribeToFeed(feedUrl: feedURLString))
        let podcast = try await resolvedPodcast(from: result)
        if store.state.subscriptions.count == 1 {
            store.recordProductSignal(.init(name: .firstSubscription, outcome: .created))
        }
        return podcast
    }

    /// OPML entries use the same core subscribe flow as every other source.
    @discardableResult
    func adopt(opmlEntry seed: Podcast) async throws -> Podcast? {
        guard let feedURL = seed.feedURL else { return nil }
        do {
            return try await addSubscription(feedURLString: feedURL.absoluteString)
        } catch AddError.alreadySubscribed {
            return nil
        }
    }

    func refresh(_ podcast: Podcast) async {
        guard let live = store.podcast(id: podcast.id) else { return }
        do {
            try await SubscriptionRefreshService().refresh(live.id, store: store)
        } catch {
            let endpoint = PrivacySafeDiagnostics.endpoint(live.feedURL)
            Self.logger.error(
                "refresh failed for \(endpoint, privacy: .public): \(error.localizedDescription, privacy: .public)"
            )
        }
    }
}
