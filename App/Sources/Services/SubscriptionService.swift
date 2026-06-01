import Foundation
import os.log

/// Coordinates `FeedClient` fetches with `AppStateStore` writes.
///
/// Single entry point shared by:
///   - "Add by URL" — first-time subscribe to an unknown feed.
///   - OPML import — enrich each parsed entry then store + episodes.
///   - Pull-to-refresh — re-fetch one show, or every show in parallel.
///   - Agent `play_external_episode` — `ensurePodcast(feedURLString:)` makes
///     the metadata available without forcing a follow.
///
/// All work is `@MainActor`-isolated because the store is `@MainActor` and we
/// dispatch the I/O via `URLSession.shared` which already hops off-main
/// internally. Keeping this service main-actor avoids a fan of explicit
/// `await MainActor.run` blocks at the call sites.
@MainActor
struct SubscriptionService {

    private static let logger = Logger.app("SubscriptionService")

    /// Underlying feed client. Tests can pass a stub session via
    /// `FeedClient(session:)`.
    let client: FeedClient

    /// The destination store. Podcasts, subscriptions and episodes all land
    /// here.
    let store: AppStateStore

    init(store: AppStateStore, client: FeedClient = FeedClient()) {
        self.store = store
        self.client = client
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
    /// upserting it when the app doesn't already know about it.
    ///
    /// Does NOT create a `PodcastSubscription` — call `addSubscription` for
    /// that. The agent's external-play path uses this to capture proper
    /// metadata for episodes the user hasn't followed.
    @discardableResult
    func ensurePodcast(feedURLString: String) async throws -> Podcast {
        let trimmed = feedURLString.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = normalizedURL(from: trimmed) else {
            throw AddError.invalidURL
        }
        if let existing = store.podcast(feedURL: url) {
            return existing
        }
        let placeholder = Podcast(
            kind: .rss,
            feedURL: url,
            title: url.host ?? trimmed,
            titleIsPlaceholder: true
        )
        let result: FeedClient.FeedFetchResult
        do {
            result = try await client.fetch(placeholder)
        } catch let feedError as FeedClient.FeedFetchError {
            throw map(feedError)
        }
        switch result {
        case .updated(let podcast, let episodes, _):
            // FeedClient returns a new Podcast with real metadata; its
            // titleIsPlaceholder defaults to false, so no explicit clear needed.
            let stored = store.upsertPodcast(podcast)
            store.upsertEpisodes(
                episodes,
                forPodcast: stored.id
            )
            return stored
        case .notModified:
            // First fetch can't realistically be 304 (no ETag was sent), but
            // if a server misbehaves we still want a record on disk.
            // titleIsPlaceholder stays true — real title still unknown.
            return store.upsertPodcast(placeholder)
        }
    }

    // MARK: - Add by URL (subscribe + fetch episodes)

    /// Subscribes to a feed URL. Resolves the podcast (fetching the feed
    /// fresh when the row is brand-new or only a thin metadata
    /// placeholder created by a prior external play), persists the
    /// resulting podcast + subscription, and upserts the parsed episodes
    /// — the user is now following the show, so they should see the
    /// backlog like a normal subscribe.
    ///
    /// Returns the live podcast on success. Throws `AddError` for any UI-
    /// reportable failure (including the duplicate-follow case so callers
    /// can surface a friendly "you're already subscribed" notice).
    @discardableResult
    func addSubscription(feedURLString: String) async throws -> Podcast {
        // Delegate entirely to the Rust kernel: it validates, fetches,
        // ingests episodes, and projects the new podcast into the store.
        // `kernelSubscribe` blocks until the podcast appears in the library
        // snapshot (or times out with an informative error).
        return try await store.kernelSubscribe(feedURL: feedURLString)
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

    private func normalizedURL(from input: String) -> URL? {
        guard !input.isEmpty else { return nil }
        let candidate = input.contains("://") ? input : "https://\(input)"
        guard let url = URL(string: candidate),
              let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https",
              url.host?.isEmpty == false
        else { return nil }
        return url
    }

    private func map(_ error: FeedClient.FeedFetchError) -> AddError {
        switch error {
        case .transport(let underlying):
            return .transport(underlying)
        case .http(let status):
            return .http(status)
        case .parse(let parseError):
            let message = parseError.errorDescription
                ?? (parseError as NSError).localizedDescription
            return .parse(message)
        case .missingFeedURL:
            return .invalidURL
        }
    }
}
