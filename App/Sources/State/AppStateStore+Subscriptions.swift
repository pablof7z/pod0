import Foundation
import Pod0Core

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
        guard !isSharedLibraryAuthoritative else { return false }
        let isFirst = state.subscriptions.isEmpty
        guard state.podcasts.contains(where: { $0.id == podcastID }) else { return false }
        guard !state.subscriptions.contains(where: { $0.podcastID == podcastID }) else { return false }
        mutateState { $0.subscriptions.append(PodcastSubscription(podcastID: podcastID)) }
        if isFirst { recordProductSignal(.init(name: .firstSubscription, outcome: .created)) }
        return true
    }

    /// Inserts a follow row with an explicit subscription record. Used by
    /// the OPML import path which materializes the row inline.
    @discardableResult
    func addSubscription(_ subscription: PodcastSubscription) -> Bool {
        guard !isSharedLibraryAuthoritative else { return false }
        let isFirst = state.subscriptions.isEmpty
        guard state.podcasts.contains(where: { $0.id == subscription.podcastID }) else { return false }
        guard !state.subscriptions.contains(where: { $0.podcastID == subscription.podcastID }) else { return false }
        mutateState { $0.subscriptions.append(subscription) }
        if isFirst { recordProductSignal(.init(name: .firstSubscription, outcome: .created)) }
        return true
    }

    /// Imports a batch of podcasts the user wants to follow, each with its
    /// initial episode set. Pre-existing podcasts (matched by feed URL)
    /// are skipped — call refresh on them instead.
    @discardableResult
    func addSubscriptions(_ payloads: [SubscriptionImportPayload]) -> SubscriptionImportResult {
        guard !isSharedLibraryAuthoritative else {
            return SubscriptionImportResult(imported: 0, skipped: payloads.count)
        }
        let isFirst = state.subscriptions.isEmpty
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
            mutateState { $0 = next }
        }
        if isFirst { recordProductSignal(.init(name: .firstSubscription, outcome: .created)) }

        return SubscriptionImportResult(imported: imported, skipped: skipped)
    }

    /// Fully removes a podcast — its metadata row, any follow row, and
    /// every episode that belonged to it. Used both by the "Unsubscribe"
    /// destructive action on followed podcasts and by the swipe-to-delete
    /// on the all-podcasts list for podcasts the user never followed.
    func deletePodcast(podcastID: UUID) {
        if isSharedLibraryAuthoritative,
           podcast(id: podcastID)?.kind == .rss {
            Task { @MainActor [weak self] in
                try? await self?.deletePodcastAndWait(podcastID: podcastID)
            }
            return
        }
        deleteSwiftPodcast(podcastID: podcastID)
    }

    /// Executes an RSS deletion through the authoritative core and returns
    /// only after the resulting projection has replaced the Swift read model.
    func deletePodcastAndWait(podcastID: UUID) async throws {
        if isSharedLibraryAuthoritative {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            _ = try await sharedLibrary.execute(.unsubscribe(
                podcastId: PodcastId(uuid: podcastID)
            ))
            return
        }
        deleteSwiftPodcast(podcastID: podcastID)
    }

    private func deleteSwiftPodcast(podcastID: UUID) {
        var next = state
        next.subscriptions.removeAll { $0.podcastID == podcastID }
        next.podcasts.removeAll { $0.id == podcastID }
        next.episodes.removeAll { $0.podcastID == podcastID }
        performMutationBatch {
            mutateState { $0 = next }
            invalidateEpisodeProjections()
        }
    }

    /// Toggles new-episode notifications for a subscribed podcast.
    func setSubscriptionNotificationsEnabled(_ podcastID: UUID, enabled: Bool) {
        if isSharedLibraryAuthoritative {
            Task { @MainActor [weak self] in
                try? await self?.setSubscriptionNotificationsAndWait(
                    podcastID,
                    enabled: enabled
                )
            }
            return
        }
        guard let idx = state.subscriptions.firstIndex(where: { $0.podcastID == podcastID }) else { return }
        mutateState { $0.subscriptions[idx].notificationsEnabled = enabled }
    }

    func setSubscriptionNotificationsAndWait(_ podcastID: UUID, enabled: Bool) async throws {
        if isSharedLibraryAuthoritative {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            _ = try await sharedLibrary.execute(.setSubscriptionNotifications(
                podcastId: PodcastId(uuid: podcastID),
                enabled: enabled
            ))
            return
        }
        setSubscriptionNotificationsEnabled(podcastID, enabled: enabled)
    }

    /// Replaces the per-podcast auto-download policy.
    func setSubscriptionAutoDownload(_ podcastID: UUID, policy: AutoDownloadPolicy) {
        if isSharedLibraryAuthoritative {
            Task { @MainActor [weak self] in
                try? await self?.setSubscriptionAutoDownloadAndWait(
                    podcastID,
                    policy: policy
                )
            }
            return
        }
        guard let idx = state.subscriptions.firstIndex(where: { $0.podcastID == podcastID }) else { return }
        mutateState { $0.subscriptions[idx].autoDownload = policy }
    }

    func setSubscriptionAutoDownloadAndWait(
        _ podcastID: UUID,
        policy: AutoDownloadPolicy
    ) async throws {
        if isSharedLibraryAuthoritative {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            _ = try await sharedLibrary.execute(.setSubscriptionAutoDownload(
                podcastId: PodcastId(uuid: podcastID),
                policy: policy.coreValue
            ))
            return
        }
        setSubscriptionAutoDownload(podcastID, policy: policy)
    }

    static func feedURLKey(_ url: URL) -> String {
        url.absoluteString.lowercased()
    }
}
