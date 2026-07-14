import Foundation

struct SubscriptionImportPayload: Sendable {
    let podcast: Podcast
    let subscription: PodcastSubscription
    let episodes: [Episode]
}

struct SubscriptionImportResult: Sendable, Equatable {
    let imported: Int
    let skipped: Int
}

// MARK: - Podcast metadata lookups

extension AppStateStore {

    /// All podcasts known to the app — followed or not.
    var allPodcasts: [Podcast] { state.podcasts }

    /// Returns the podcast row matching `id`, or `nil` when not found.
    /// Synthesizes `Podcast.unknown` on the fly if a caller queries the
    /// Unknown ID before hydration has finished inserting it.
    func podcast(id: UUID) -> Podcast? {
        if let hit = state.podcasts.first(where: { $0.id == id }) {
            return hit
        }
        if id == Podcast.unknownID {
            return Podcast.unknown
        }
        return nil
    }

    /// Returns the podcast row whose feed URL matches the input,
    /// case-insensitive so trailing-slash and scheme-case differences
    /// don't create duplicates. Synthetic podcasts (no `feedURL`) are
    /// looked up via this same path when callers use a sentinel URL.
    func podcast(feedURL: URL) -> Podcast? {
        state.podcasts.first { existing in
            guard let existingURL = existing.feedURL else { return false }
            return existingURL.absoluteString.caseInsensitiveCompare(feedURL.absoluteString) == .orderedSame
        }
    }

    /// Inserts a brand-new podcast metadata row, or merges fields into the
    /// existing row matched by id or feedURL. Returns the persisted podcast.
    @discardableResult
    func upsertPodcast(_ incoming: Podcast) -> Podcast {
        if let idx = state.podcasts.firstIndex(where: { $0.id == incoming.id }) {
            state.podcasts[idx] = merged(state.podcasts[idx], with: incoming)
            return state.podcasts[idx]
        }
        if let feedURL = incoming.feedURL,
           let idx = state.podcasts.firstIndex(where: {
               $0.feedURL?.absoluteString.caseInsensitiveCompare(feedURL.absoluteString) == .orderedSame
           }) {
            // Same feed under a different id — keep the existing row.
            state.podcasts[idx] = merged(state.podcasts[idx], with: incoming)
            return state.podcasts[idx]
        }
        state.podcasts.append(incoming)
        return incoming
    }

    /// Persists changes back to a podcast — used after a feed refresh to
    /// write the new HTTP cache (etag / lastModified) and any drifted
    /// metadata (title, imageURL).
    func updatePodcast(_ updated: Podcast) {
        guard let idx = state.podcasts.firstIndex(where: { $0.id == updated.id }) else { return }
        state.podcasts[idx] = updated
    }

    /// Merge policy: keep the existing podcast's identity, prefer non-empty
    /// incoming values for human-visible fields. HTTP cache always wins
    /// when present (otherwise we'd lose etags between refreshes).
    private func merged(_ existing: Podcast, with incoming: Podcast) -> Podcast {
        var out = existing
        if !incoming.title.isEmpty { out.title = incoming.title }
        if !incoming.author.isEmpty { out.author = incoming.author }
        if !incoming.description.isEmpty { out.description = incoming.description }
        if let img = incoming.imageURL { out.imageURL = img }
        if let lang = incoming.language { out.language = lang }
        if !incoming.categories.isEmpty { out.categories = incoming.categories }
        if let feedURL = incoming.feedURL { out.feedURL = feedURL }
        if let lr = incoming.lastRefreshedAt { out.lastRefreshedAt = lr }
        if let etag = incoming.etag { out.etag = etag }
        if let lm = incoming.lastModified { out.lastModified = lm }
        return out
    }
}
