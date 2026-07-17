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
    func refresh(_ podcastID: UUID, store: AppStateStore) async throws {
        guard let podcast = store.podcast(id: podcastID),
              podcast.feedURL != nil else {
            return
        }
        let result = try await client.fetch(podcast)
        _ = apply(
            outcome: .success(
                originalID: podcastID,
                original: podcast,
                result: result
            ),
            store: store
        )
    }

    /// Refreshes every followed podcast (joined via `subscriptions`),
    /// bounded to `maxConcurrent` in-flight fetches. Errors are logged and
    /// swallowed per-podcast so one failing feed doesn't sink the whole sweep.
    ///
    /// Episode upserts are applied as feeds resolve so the UI sees new rows
    /// without delay. Follow-on intent is recorded atomically with each
    /// discovery batch and later admitted by WorkCoordinator.
    func refreshAll(store: AppStateStore, maxConcurrent: Int = 4) async {
        let podcasts = store.sortedFollowedPodcastsByRecency.filter { $0.feedURL != nil }
        guard !podcasts.isEmpty else { return }
        let bounded = max(1, maxConcurrent)
        let client = self.client

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
                _ = apply(outcome: outcome, store: store)
            }

            index = upper
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
    /// side effects (auto-download + notifications) to dispatch. Returns
    /// `nil` for `notModified`, failures, and feeds that had no newly
    /// inserted episodes.
    private func apply(
        outcome: PodcastRefreshOutcome,
        store: AppStateStore
    ) -> [UUID] {
        switch outcome {
        case .success(_, let original, let result):
            switch result {
            case .notModified(let lastRefreshedAt):
                var bumped = original
                bumped.lastRefreshedAt = lastRefreshedAt
                store.updatePodcast(bumped)
                return []
            case .updated(let updatedPodcast, let episodes, _):
                let priorGUIDs = Set(
                    store.episodes(forPodcast: updatedPodcast.id).map(\.guid)
                )
                let firstEverFetch = priorGUIDs.isEmpty
                let newlyInsertedIDs = store.upsertEpisodes(
                    episodes,
                    forPodcast: updatedPodcast.id,
                    evaluateAutoDownload: true,
                    notificationDiscoveredAt: firstEverFetch ? nil : Date()
                )
                store.updatePodcast(updatedPodcast)
                return newlyInsertedIDs
            }
        case .failure(let originalID, let error):
            Self.logger.notice(
                "refresh failed for \(originalID, privacy: .public): \(error.localizedDescription, privacy: .public)"
            )
            return []
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
