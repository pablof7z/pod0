import Foundation

// MARK: - User follow state (`PodcastSubscription`)

extension AppStateStore {

    /// Podcasts the user actively follows, sorted alphabetically by title.
    /// Synthetic podcasts (Agent Generated, Unknown) are excluded by virtue
    /// of having no `PodcastSubscription` row in the new model — they're
    /// `Podcast`-only.
    var sortedFollowedPodcasts: [Podcast] {
        let podcastByID = Dictionary(uniqueKeysWithValues: state.podcasts.map { ($0.id, $0) })
        return state.subscriptions
            .compactMap { podcastByID[$0.podcastID] }
            .filter { $0.kind == .rss }
            .sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
    }

    /// Returns the subscription row for a podcast, or `nil` if the user does
    /// not follow it.
    func subscription(podcastID: UUID) -> PodcastSubscription? {
        state.subscriptions.first { $0.podcastID == podcastID }
    }

    /// Convenience: returns the podcast for an existing subscription row.
    func podcast(for subscription: PodcastSubscription) -> Podcast? {
        podcast(id: subscription.podcastID)
    }

    /// Inserts a follow row for the given podcast. Returns `false` if the
    /// user already follows this podcast. The podcast row must already
    /// exist (call `upsertPodcast` or `ensurePodcast(feedURL:)` first).
    @discardableResult
    func addSubscription(podcastID: UUID) -> Bool {
        guard state.podcasts.contains(where: { $0.id == podcastID }) else { return false }
        guard !state.subscriptions.contains(where: { $0.podcastID == podcastID }) else { return false }
        state.subscriptions.append(PodcastSubscription(podcastID: podcastID))
        return true
    }

    /// Inserts a follow row with an explicit subscription record. Used by
    /// the OPML import path which materializes the row inline.
    @discardableResult
    func addSubscription(_ subscription: PodcastSubscription) -> Bool {
        guard state.podcasts.contains(where: { $0.id == subscription.podcastID }) else { return false }
        guard !state.subscriptions.contains(where: { $0.podcastID == subscription.podcastID }) else { return false }
        state.subscriptions.append(subscription)
        return true
    }

    /// Imports a batch of podcasts the user wants to follow, each with its
    /// initial episode set. Pre-existing podcasts (matched by feed URL)
    /// are skipped — call refresh on them instead.
    @discardableResult
    func addSubscriptions(_ payloads: [SubscriptionImportPayload]) -> SubscriptionImportResult {
        guard !payloads.isEmpty else {
            return SubscriptionImportResult(imported: 0, skipped: 0)
        }

        var next = state
        // Pre-existing podcasts may already be in the store (e.g. from a
        // prior external play). Index by feed URL → podcast ID so we
        // promote the existing row to a follow rather than creating a
        // duplicate. Index of currently-followed podcasts dedupes against
        // re-import of an OPML you've already adopted.
        var podcastIDByFeedKey: [String: UUID] = [:]
        for podcast in next.podcasts {
            if let feedURL = podcast.feedURL {
                podcastIDByFeedKey[Self.feedURLKey(feedURL)] = podcast.id
            }
        }
        var subscribedPodcastIDs = Set(next.subscriptions.map(\.podcastID))
        var imported = 0
        var skipped = 0

        next.podcasts.reserveCapacity(next.podcasts.count + payloads.count)
        next.subscriptions.reserveCapacity(next.subscriptions.count + payloads.count)
        next.episodes.reserveCapacity(next.episodes.count + payloads.reduce(0) { $0 + $1.episodes.count })

        for payload in payloads {
            guard let feedURL = payload.podcast.feedURL else {
                skipped += 1
                continue
            }
            let key = Self.feedURLKey(feedURL)
            if let existingID = podcastIDByFeedKey[key] {
                // Known podcast — only count as imported if we still need
                // to add the follow row.
                guard subscribedPodcastIDs.insert(existingID).inserted else {
                    skipped += 1
                    continue
                }
                // Promote: keep the existing Podcast.id (existing episodes
                // already reference it) but adopt the freshly-fetched
                // metadata + backlog from the OPML import. Otherwise an
                // external-play placeholder's stub title/no-episodes would
                // win silently.
                if let podcastIdx = next.podcasts.firstIndex(where: { $0.id == existingID }) {
                    var merged = payload.podcast
                    merged.id = existingID
                    next.podcasts[podcastIdx] = merged
                }
                next.subscriptions.append(PodcastSubscription(podcastID: existingID))
                // Re-parent the OPML-fetched episodes to the existing
                // podcast id before appending so foreign keys stay consistent.
                let reparented = payload.episodes.map { episode -> Episode in
                    var copy = episode
                    copy.podcastID = existingID
                    return copy
                }
                next.episodes.append(contentsOf: reparented)
                imported += 1
                continue
            }
            podcastIDByFeedKey[key] = payload.podcast.id
            subscribedPodcastIDs.insert(payload.podcast.id)
            next.podcasts.append(payload.podcast)
            next.subscriptions.append(payload.subscription)
            next.episodes.append(contentsOf: payload.episodes)
            imported += 1
        }

        guard imported > 0 else {
            return SubscriptionImportResult(imported: imported, skipped: skipped)
        }

        performMutationBatch {
            state = next
        }

        return SubscriptionImportResult(imported: imported, skipped: skipped)
    }

    /// Fully removes a podcast — its metadata row, any follow row, and
    /// every episode that belonged to it. Used both by the "Unsubscribe"
    /// destructive action on followed podcasts and by the swipe-to-delete
    /// on the all-podcasts list for podcasts the user never followed.
    func deletePodcast(podcastID: UUID) {
        let removedEpisodeIDs = state.episodes
            .filter { $0.podcastID == podcastID }
            .map(\.id)

        var next = state
        next.subscriptions.removeAll { $0.podcastID == podcastID }
        next.podcasts.removeAll { $0.id == podcastID }
        next.episodes.removeAll { $0.podcastID == podcastID }
        performMutationBatch {
            state = next
            invalidateEpisodeProjections()
        }

        // Wiki citation invalidation — same fan-out as before.
        Task { @MainActor in
            let inventory = (try? WikiStorage.shared.loadInventory()) ?? WikiInventory()
            var jobs: [WikiTriggers.WikiRefreshJob] = []
            for episodeID in removedEpisodeIDs {
                jobs.append(contentsOf: WikiTriggers.jobs(
                    for: .episodeRemoved(episodeID: episodeID, podcastID: podcastID),
                    inventory: inventory
                ))
            }
            guard !jobs.isEmpty else { return }
            WikiRefreshExecutor.shared.run(jobs: jobs)
        }
    }

    /// Toggles new-episode notifications for a subscribed podcast.
    func setSubscriptionNotificationsEnabled(_ podcastID: UUID, enabled: Bool) {
        guard let idx = state.subscriptions.firstIndex(where: { $0.podcastID == podcastID }) else { return }
        state.subscriptions[idx].notificationsEnabled = enabled
    }

    /// Replaces the per-podcast auto-download policy.
    func setSubscriptionAutoDownload(_ podcastID: UUID, policy: AutoDownloadPolicy) {
        guard let idx = state.subscriptions.firstIndex(where: { $0.podcastID == podcastID }) else { return }
        state.subscriptions[idx].autoDownload = policy
    }

    static func feedURLKey(_ url: URL) -> String {
        url.absoluteString.lowercased()
    }
}
