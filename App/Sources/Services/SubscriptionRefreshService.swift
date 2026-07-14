import Foundation
import UIKit
import os.log

// MARK: - SubscriptionRefreshService

/// Background poller for the user's podcast subscriptions.
///
/// Owns the foreground refresh loop:
///   1. On `startPeriodicRefresh(...)` we kick a `Task` that refreshes once
///      immediately, then sleeps `interval` between later `refreshAll` calls.
///      Re-entrant calls cancel and replace the in-flight task so the call
///      site is idempotent.
///   2. We register for `UIApplication.didEnterBackgroundNotification` to
///      cancel the loop when the app suspends, and
///      `UIApplication.willEnterForegroundNotification` to restart it.
///   3. Every refresh round runs `FeedClient.fetch` against each followed
///      podcast in parallel, bounded by `maxConcurrent` so a 200-podcast user
///      doesn't open 200 simultaneous sockets.
///
/// `FeedClient.fetch` is async + `Sendable` — the network hop happens off the
/// main actor. The service hops back to the main actor to apply each parsed
/// result via `AppStateStore`'s mutation methods, which keeps the store's
/// `didSet` persistence path single-threaded.
@MainActor
final class SubscriptionRefreshService {

    // MARK: Singleton

    static let shared = SubscriptionRefreshService()

    // MARK: Configuration

    /// Default polling cadence when callers don't override `interval` —
    /// 30 minutes mirrors the baseline-podcast-features brief §2.
    static let defaultInterval: Duration = .seconds(30 * 60)

    // MARK: State

    private static let logger = Logger.app("SubscriptionRefreshService")
    private let client: FeedClient
    private var pollingTask: Task<Void, Never>?
    private var foregroundObserver: NSObjectProtocol?
    private var backgroundObserver: NSObjectProtocol?
    private weak var registeredStore: AppStateStore?
    private var registeredInterval: Duration = SubscriptionRefreshService.defaultInterval

    // MARK: Init

    init(client: FeedClient = FeedClient()) {
        self.client = client
    }

    // MARK: - Public API

    /// Refreshes a single podcast. Idempotent — issues a conditional GET via
    /// the podcast's stored `etag` / `lastModified`. A `304` only bumps
    /// `lastRefreshedAt`; an updated feed upserts every parsed episode and
    /// writes the new cache headers back via `updatePodcast`.
    ///
    /// Same triage gate as `refreshAll`: auto-download and new-episode
    /// notifications are deferred until the AI Inbox triage pass settles
    /// so archived episodes never produce a banner or auto-download.
    func refresh(_ podcastID: UUID, store: AppStateStore) async throws {
        guard let podcast = store.podcast(id: podcastID),
              podcast.feedURL != nil else {
            return
        }
        let result = try await client.fetch(podcast)
        let pending = apply(
            outcome: .success(
                originalID: podcastID,
                original: podcast,
                result: result
            ),
            store: store
        )
        InboxTriageService.shared.triageNewEpisodes(store: store)
        await waitForTriageToSettle()
        if let pending {
            dispatchSideEffects(pending, store: store)
        }
    }

    /// Refreshes every followed podcast (joined via `subscriptions`),
    /// bounded to `maxConcurrent` in-flight fetches. Errors are logged and
    /// swallowed per-podcast so one failing feed doesn't sink the whole sweep.
    ///
    /// Side-effect ordering: episode upserts are applied as feeds resolve
    /// (so the UI sees new rows without delay), but auto-download evaluation
    /// and new-episode notifications are deferred until AI Inbox triage has
    /// classified the new arrivals. This guarantees archived episodes never
    /// produce a banner or kick off a background download.
    func refreshAll(store: AppStateStore, maxConcurrent: Int = 4) async {
        let podcasts = store.sortedFollowedPodcastsByRecency.filter { $0.feedURL != nil }
        guard !podcasts.isEmpty else { return }
        let bounded = max(1, maxConcurrent)
        let client = self.client

        var pendingSideEffects: [PendingSideEffects] = []
        var index = 0
        while index < podcasts.count {
            let upper = min(index + bounded, podcasts.count)
            let slice = Array(podcasts[index..<upper])
            let outcomes = await withTaskGroup(
                of: PodcastRefreshOutcome.self,
                returning: [PodcastRefreshOutcome].self
            ) { group in
                for podcast in slice {
                    group.addTask {
                        do {
                            let result = try await client.fetch(podcast)
                            return .success(originalID: podcast.id, original: podcast, result: result)
                        } catch {
                            return .failure(originalID: podcast.id, error: error)
                        }
                    }
                }
                var collected: [PodcastRefreshOutcome] = []
                collected.reserveCapacity(slice.count)
                for await outcome in group {
                    collected.append(outcome)
                }
                return collected
            }

            for outcome in outcomes {
                if let pending = apply(outcome: outcome, store: store) {
                    pendingSideEffects.append(pending)
                }
            }

            index = upper
        }

        // Kick autonomous AI Inbox triage and wait for it to settle before
        // letting auto-downloads or new-episode banners fire. The service
        // writes decisions into the store via `applyTriageDecisions`, after
        // which we filter side effects against the per-episode triage
        // verdict so archived episodes stay silent on the same cycle they
        // were created in.
        InboxTriageService.shared.triageNewEpisodes(store: store)
        await waitForTriageToSettle()

        for pending in pendingSideEffects {
            dispatchSideEffects(pending, store: store)
        }
    }

    /// Blocks until the active triage pass (if any) finishes, capped at a
    /// per-cycle deadline so a stuck LLM call can't hang the refresh loop.
    /// We pause briefly first so the task spawned by `triageNewEpisodes`
    /// has a chance to enter its run body and flip `isRunning` true; if it
    /// returns early (no candidates), the poll exits on the first read.
    private func waitForTriageToSettle() async {
        try? await Task.sleep(for: .milliseconds(50))
        let deadline = Date().addingTimeInterval(60)
        while InboxTriageService.shared.isRunning, Date() < deadline {
            try? await Task.sleep(for: .milliseconds(200))
        }
    }

    /// Starts the periodic refresh loop. Idempotent — the existing in-flight
    /// loop is cancelled and replaced if `start` is called twice. Also
    /// registers for foreground / background lifecycle notifications so the
    /// loop pauses while the app is suspended.
    func startPeriodicRefresh(
        store: AppStateStore,
        interval: Duration = SubscriptionRefreshService.defaultInterval
    ) {
        registeredStore = store
        registeredInterval = interval
        registerLifecycleObserversIfNeeded()
        startLoop(store: store, interval: interval)
    }

    /// Cancels the active polling loop, if any. Lifecycle observers stay
    /// registered so a later `startPeriodicRefresh` can re-arm without
    /// re-installing them.
    func stopPeriodicRefresh() {
        pollingTask?.cancel()
        pollingTask = nil
    }

    // MARK: - Private

    /// Applies a single feed-fetch outcome to the store and returns the
    /// side effects (auto-download + notifications) that must be deferred
    /// until after AI Inbox triage runs. Returns `nil` for `notModified`,
    /// failures, and feeds that had no prior episode set (the first-ever
    /// fetch suppresses notifications regardless of triage).
    private func apply(
        outcome: PodcastRefreshOutcome,
        store: AppStateStore
    ) -> PendingSideEffects? {
        switch outcome {
        case .success(_, let original, let result):
            switch result {
            case .notModified(let lastRefreshedAt):
                var bumped = original
                bumped.lastRefreshedAt = lastRefreshedAt
                store.updatePodcast(bumped)
                return nil
            case .updated(let updatedPodcast, let episodes, _):
                let priorGUIDs = Set(
                    store.episodes(forPodcast: updatedPodcast.id).map(\.guid)
                )
                let firstEverFetch = priorGUIDs.isEmpty
                let newlyInsertedIDs = store.upsertEpisodes(
                    episodes,
                    forPodcast: updatedPodcast.id,
                    evaluateAutoDownload: false
                )
                store.updatePodcast(updatedPodcast)
                guard !newlyInsertedIDs.isEmpty else { return nil }
                return PendingSideEffects(
                    podcast: updatedPodcast,
                    newEpisodeIDs: newlyInsertedIDs,
                    suppressNotifications: firstEverFetch
                )
            }
        case .failure(let originalID, let error):
            Self.logger.notice(
                "refresh failed for \(originalID, privacy: .public): \(error.localizedDescription, privacy: .public)"
            )
            return nil
        }
    }

    /// Fires the deferred auto-download evaluation and new-episode
    /// notifications for episodes that survived triage (i.e. weren't
    /// archived). Archived episodes are silent by contract: no banner,
    /// no background download, no presence on any "new" surface.
    private func dispatchSideEffects(_ pending: PendingSideEffects, store: AppStateStore) {
        let survivors: [UUID] = pending.newEpisodeIDs.filter { id in
            guard let ep = store.episode(id: id) else { return false }
            return !ep.isTriageArchived
        }
        guard !survivors.isEmpty else { return }

        EpisodeDownloadService.shared.attach(appStore: store)
        EpisodeDownloadService.shared.evaluateAutoDownload(
            forPodcast: pending.podcast.id,
            newEpisodeIDs: survivors
        )
        TranscriptIngestService.shared.evaluateAutoIngest(
            newEpisodeIDs: survivors
        )

        guard !pending.suppressNotifications,
              let subscription = store.subscription(podcastID: pending.podcast.id),
              subscription.notificationsEnabled else { return }
        let episodes = survivors
            .compactMap { store.episode(id: $0) }
            .sorted { $0.pubDate > $1.pubDate }
        guard !episodes.isEmpty else { return }
        let podcast = pending.podcast
        Task {
            await NotificationService.notifyNewEpisodes(episodes, podcast: podcast)
        }
    }

    private func startLoop(store: AppStateStore, interval: Duration) {
        pollingTask?.cancel()
        pollingTask = Task { [weak self] in
            guard let self else { return }
            await self.refreshAll(store: store)

            while !Task.isCancelled {
                do {
                    try await Task.sleep(for: interval)
                } catch {
                    return
                }
                if Task.isCancelled { return }
                await self.refreshAll(store: store)
            }
        }
    }

    private func registerLifecycleObserversIfNeeded() {
        if backgroundObserver == nil {
            backgroundObserver = NotificationCenter.default.addObserver(
                forName: UIApplication.didEnterBackgroundNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    self?.stopPeriodicRefresh()
                }
            }
        }
        if foregroundObserver == nil {
            foregroundObserver = NotificationCenter.default.addObserver(
                forName: UIApplication.willEnterForegroundNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self,
                          let store = self.registeredStore
                    else { return }
                    self.startLoop(store: store, interval: self.registeredInterval)
                }
            }
        }
    }
}

// MARK: - Outcome

private enum PodcastRefreshOutcome: Sendable {
    case success(originalID: UUID, original: Podcast, result: FeedClient.FeedFetchResult)
    case failure(originalID: UUID, error: Error)
}

// MARK: - Pending side effects

/// Per-feed bundle of post-triage side effects. Created during the upsert
/// sweep and dispatched only after the AI Inbox triage pass settles so
/// archived episodes never produce a notification or auto-download.
private struct PendingSideEffects {
    let podcast: Podcast
    let newEpisodeIDs: [UUID]
    /// Suppress new-episode banners on the very first fetch of a podcast,
    /// mirroring the legacy `notifyIfNeeded` semantics (a fresh follow
    /// shouldn't carpet-bomb the lock screen with back-catalog episodes).
    let suppressNotifications: Bool
}
