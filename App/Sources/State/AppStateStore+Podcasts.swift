import Foundation

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

}
