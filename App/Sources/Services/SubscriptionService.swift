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
                return "This feed needs sign-in or isn't public — Podcastr can't subscribe to it."
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
                forPodcast: stored.id,
                evaluateAutoDownload: false
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
        let trimmed = feedURLString.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = normalizedURL(from: trimmed) else {
            throw AddError.invalidURL
        }

        // Refuse early when the user already follows the matching podcast.
        // We DON'T early-return for the "podcast known but not followed"
        // case — that's exactly the path we want to promote to a real
        // follow, including backlog ingestion.
        if let existing = store.podcast(feedURL: url),
           store.subscription(podcastID: existing.id) != nil {
            throw AddError.alreadySubscribed(title: existing.title.isEmpty ? trimmed : existing.title)
        }

        // Always do a fresh fetch on the follow path so the user sees the
        // show's current episode list, even when a placeholder Podcast row
        // already exists from an earlier external play.
        //
        // Critical: strip any cached `etag` / `lastModified` from the input
        // before fetching. A placeholder created by `play_external_episode`
        // may carry validators from its metadata-hydration pass, and we
        // intentionally did NOT upsert episodes during that pass. If we sent
        // those validators on the follow fetch, the server could 304 and
        // we'd add the subscription with an empty episode list. Force a 200
        // to guarantee the backlog backfill the user expects from
        // subscribing.
        var podcastForFetch = store.podcast(feedURL: url) ?? Podcast(
            kind: .rss,
            feedURL: url,
            title: url.host ?? trimmed,
            titleIsPlaceholder: true
        )
        podcastForFetch.etag = nil
        podcastForFetch.lastModified = nil
        let result: FeedClient.FeedFetchResult
        do {
            result = try await client.fetch(podcastForFetch)
        } catch let feedError as FeedClient.FeedFetchError {
            throw map(feedError)
        }
        let stored: Podcast
        switch result {
        case .updated(let fetched, let episodes, _):
            stored = store.upsertPodcast(fetched)
            store.upsertEpisodes(
                episodes,
                forPodcast: stored.id,
                evaluateAutoDownload: false
            )
        case .notModified:
            stored = store.upsertPodcast(podcastForFetch)
        }
        store.addSubscription(podcastID: stored.id)
        return stored
    }

    // MARK: - Adopt a parsed OPML entry

    /// Persists a single OPML-parsed entry, enriching it with a live fetch so
    /// the title / author / image / description come from the feed itself
    /// rather than the OPML attributes (which are usually sparse).
    ///
    /// Skips silently when the user already follows the feed; returns `nil`
    /// in that case so the caller can count duplicates separately from
    /// errors.
    @discardableResult
    func adopt(opmlEntry seed: Podcast) async throws -> Podcast? {
        guard let payload = try await fetchForAdoption(opmlEntry: seed) else { return nil }
        let result = store.addSubscriptions([payload])
        return result.imported == 1 ? payload.podcast : nil
    }

    /// Fetches and enriches an OPML entry without mutating the store. The
    /// import sheet uses this to gather many feeds and then commit them in
    /// one store batch.
    ///
    /// Skips ONLY when the user already follows the feed. A `Podcast` row
    /// that exists from a prior external play (no subscription) is still
    /// a valid OPML import — the import promotes it to a real follow.
    func fetchForAdoption(opmlEntry seed: Podcast) async throws -> SubscriptionImportPayload? {
        guard let feedURL = seed.feedURL else { return nil }
        if let existing = store.podcast(feedURL: feedURL),
           store.subscription(podcastID: existing.id) != nil {
            return nil
        }
        let result: FeedClient.FeedFetchResult
        do {
            result = try await client.fetch(seed)
        } catch let feedError as FeedClient.FeedFetchError {
            throw map(feedError)
        }
        switch result {
        case .updated(let podcast, let episodes, _):
            return SubscriptionImportPayload(
                podcast: podcast,
                subscription: PodcastSubscription(podcastID: podcast.id),
                episodes: episodes
            )
        case .notModified:
            return SubscriptionImportPayload(
                podcast: seed,
                subscription: PodcastSubscription(podcastID: seed.id),
                episodes: []
            )
        }
    }

    // MARK: - Refresh

    /// Re-fetches a single podcast and writes back the merged metadata + any
    /// new episodes. Errors are swallowed (logged in debug) so a single
    /// flaky feed doesn't poison a multi-feed refresh.
    func refresh(_ podcast: Podcast) async {
        guard let live = store.podcast(id: podcast.id) else { return }
        do {
            try await SubscriptionRefreshService(client: client).refresh(live.id, store: store)
        } catch {
            Self.logger.error("refresh failed for \(live.feedURL?.absoluteString ?? "(no feed)", privacy: .public): \(error.localizedDescription, privacy: .public)")
        }
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
