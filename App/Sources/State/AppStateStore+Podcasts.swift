import Foundation

// MARK: - Podcast metadata lookups

extension AppStateStore {

    /// All podcasts known to the app — followed or not.
    var allPodcasts: [Podcast] { state.podcasts }

    /// Returns the podcast row matching `id`, or `nil` when not found.
    /// Synthesizes `Podcast.unknown` on the fly if a caller queries the
    /// Unknown ID before hydration has finished inserting it.
    func podcast(id: UUID) -> Podcast? {
        if let index = podcastIndexByID[id], state.podcasts.indices.contains(index),
           state.podcasts[index].id == id {
            return state.podcasts[index]
        }
        if let podcast = state.podcasts.first(where: { $0.id == id }) {
            return podcast
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

    /// True while Rust still owes a feed fetch for this podcast. Drives the
    /// projection-backed "Subscribing…" affordances that replaced per-row
    /// native spinner state: the workflow is durable, so the indicator
    /// survives relaunch instead of dying with a blocked continuation.
    func isFeedFetchInFlight(podcastID: UUID) -> Bool {
        state.feedFetches.contains { $0.isActive && $0.podcastID == podcastID }
    }

    func isFeedFetchInFlight(feedURL: URL) -> Bool {
        if let podcast = podcast(feedURL: feedURL),
           isFeedFetchInFlight(podcastID: podcast.id) {
            return true
        }
        return state.feedFetches.contains { fetch in
            fetch.isActive && fetch.feedURLString
                .caseInsensitiveCompare(feedURL.absoluteString) == .orderedSame
        }
    }

}
