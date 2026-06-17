import Foundation

/// Coordinates feed lifecycle requests with the Rust kernel.
///
/// Single entry point shared by:
///   - "Add by URL" — first-time subscribe to an unknown feed.
///   - OPML import — subscribe to each parsed feed.
///   - Pull-to-refresh — re-fetch one show, or every followed show.
///   - Agent `list_episodes` external paths — `ensurePodcast(feedURLString:)`
///     ingests metadata and episodes without forcing a follow.
///
/// All work is `@MainActor`-isolated because the store is `@MainActor`. Feed
/// I/O lives in the Rust kernel; Swift waits for projected state.
@MainActor
struct SubscriptionService {

    /// The destination store. Podcasts, subscriptions and episodes all land
    /// here via kernel projection.
    let store: AppStateStore

    init(store: AppStateStore) {
        self.store = store
    }

    // MARK: - Errors surfaced to UI

    enum AddError: Error, LocalizedError, Equatable {
        case invalidURL
        case alreadySubscribed(title: String)
        case transport(String)
        case http(Int)
        case parse(String)

        var errorDescription: String? {
            switch self {
            case .invalidURL:
                return "That doesn't look like a valid feed URL."
            case .alreadySubscribed(let title):
                return "You're already subscribed to \(title)."
            case .transport(let message):
                return "Couldn't reach the feed: \(message)"
            case .http(let status):
                return Self.humanizeHTTPStatus(status)
            case .parse(let message):
                return message
            }
        }

        private static func humanizeHTTPStatus(_ status: Int) -> String {
            switch status {
            case 401, 403:
                return "This feed needs sign-in or isn't public — Pod0 can't subscribe to it."
            case 404, 410:
                return "We couldn't find a feed at that URL. Double-check it and try again."
            case 408, 504:
                return "The feed server took too long to respond. Try again in a moment."
            case 429:
                return "The feed server is rate-limiting requests right now. Try again in a few minutes."
            case 500..<600:
                return "The feed server hit an error (HTTP \(status)). Try again later."
            case 400..<500:
                return "The feed server rejected the request (HTTP \(status))."
            default:
                return "The feed server returned an unexpected status (HTTP \(status))."
            }
        }
    }

    // MARK: - Ensure a podcast is known (without subscribing)

    /// Returns a `Podcast` row for the feed at `feedURLString`, fetching and
    /// ingesting it through the Rust kernel when needed.
    ///
    /// Does NOT create a `PodcastSubscription` — call `addSubscription` for
    /// that. This is the Rust-owned known-feed path for external listing.
    @discardableResult
    func ensurePodcast(feedURLString: String) async throws -> Podcast {
        let trimmed = feedURLString.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = Self.normalizedFeedURL(from: trimmed) else {
            throw AddError.invalidURL
        }
        return try await store.kernelEnsurePodcast(feedURL: url.absoluteString)
    }

    // MARK: - Add by URL (subscribe + fetch episodes)

    /// Subscribes to a feed URL through the Rust kernel. If the feed was
    /// already known but unfollowed, the kernel reuses that row and marks it
    /// followed; otherwise it fetches and ingests the feed before projecting
    /// the subscription back to Swift.
    ///
    /// Returns the live podcast on success. Throws `AddError` for any UI-
    /// reportable failure (including the duplicate-follow case so callers
    /// can surface a friendly "you're already subscribed" notice).
    @discardableResult
    func addSubscription(feedURLString: String) async throws -> Podcast {
        let trimmed = feedURLString.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = Self.normalizedFeedURL(from: trimmed) else {
            throw AddError.invalidURL
        }
        // Rust normalizes the user-entered URL into the same canonical shape
        // used by duplicate detection, then the kernel owns feed fetching and
        // ingestion.
        // `kernelSubscribe` blocks until the podcast appears in the library
        // snapshot (or times out with an informative error).
        return try await store.kernelSubscribe(feedURL: url.absoluteString)
    }

    // MARK: - Refresh

    /// Re-fetches a single podcast and writes back the merged metadata + any
    /// new episodes. Errors are swallowed (logged in debug) so a single
    /// flaky feed doesn't poison a multi-feed refresh.
    func refresh(_ podcast: Podcast) async {
        guard let live = store.podcast(id: podcast.id) else { return }
        store.kernelRefresh(podcastID: live.id)
    }

    // MARK: - Helpers

    nonisolated static func normalizedFeedURL(from input: String) -> URL? {
        FeedURLNormalizer.normalizedFeedURL(from: input)
    }

}
