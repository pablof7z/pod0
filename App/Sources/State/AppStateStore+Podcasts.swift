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
    /// don't create duplicates. Feed-less podcasts (no `feedURL`) never match.
    func podcast(feedURL: URL) -> Podcast? {
        state.podcasts.first { existing in
            guard let existingURL = existing.feedURL else { return false }
            return existingURL.absoluteString.caseInsensitiveCompare(feedURL.absoluteString) == .orderedSame
        }
    }

    /// Inserts a podcast metadata row into the Swift render store. Returns the
    /// existing row unchanged if one already matches by id.
    ///
    /// This is an INSERT seam, not a merge: the legacy RSS pull-merge policy
    /// (`merged()` + feed-URL reconcile) was deleted because RSS subscribe /
    /// refresh / OPML now ingest exclusively through the Rust kernel
    /// (`kernelSubscribe` / `kernelRefresh`), and `applyKernelState` is the
    /// sole production writer of feed-backed podcast rows. This helper remains
    /// for focused AppStateStore tests and legacy non-feed fixtures; new durable
    /// podcast creation must route through the kernel (`kernelCreatePodcast` or
    /// `kernelEnsurePodcast`).
    @discardableResult
    func upsertPodcast(_ incoming: Podcast) -> Podcast {
        if let existing = state.podcasts.first(where: { $0.id == incoming.id }) {
            return existing
        }
        state.podcasts.append(incoming)
        return incoming
    }

    /// Writes drifted metadata (title / author / artwork) back onto an
    /// existing podcast row by id. Used by the agent-owned podcast editor and
    /// the external-play placeholder hydration — a direct id-keyed write for
    /// feed-less / placeholder rows, not an RSS merge.
    func updatePodcast(_ updated: Podcast) {
        guard let idx = state.podcasts.firstIndex(where: { $0.id == updated.id }) else { return }
        state.podcasts[idx] = updated
    }
}
