import Foundation
import Pod0Core
import os.log

/// Routes library intents through the sole Rust owner. Feed networking remains
/// native through `CoreFeedHost`; parsing, normalization, identity, and
/// subscription policy stay behind the typed application facade.
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
        case http(Int)
        case parse(String)

        var errorDescription: String? {
            switch self {
            case .invalidURL:
                "That doesn't look like a valid feed URL."
            case .alreadySubscribed(let title):
                "You're already subscribed to \(title)."
            case .transport:
                "Couldn't reach the feed. Check your connection and try again."
            case .http(let status):
                Self.humanizeHTTPStatus(status)
            case .parse:
                "Pod0 couldn't read a podcast feed at that address."
            }
        }

        private static func humanizeHTTPStatus(_ status: Int) -> String {
            switch status {
            case 401, 403:
                "This feed needs sign-in or isn't public — Podcastr can't subscribe to it."
            case 404, 410:
                "We couldn't find a feed at that URL. Double-check it and try again."
            case 408, 504:
                "The feed server took too long to respond. Try again in a moment."
            case 429:
                "The feed server is rate-limiting requests right now. Try again in a few minutes."
            case 500..<600:
                "The feed server hit an error (HTTP \(status)). Try again later."
            case 400..<500:
                "The feed server rejected the request (HTTP \(status))."
            default:
                "The feed server returned an unexpected status (HTTP \(status))."
            }
        }
    }

    @discardableResult
    func ensurePodcast(feedURLString: String) async throws -> Podcast {
        let result = try await executeShared(.ensurePodcast(feedUrl: feedURLString))
        return try resolvedPodcast(from: result)
    }

    @discardableResult
    func addSubscription(feedURLString: String) async throws -> Podcast {
        let result = try await executeShared(.subscribeToFeed(feedUrl: feedURLString))
        let podcast = try resolvedPodcast(from: result)
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
