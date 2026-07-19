import Foundation
import Pod0Core
import UIKit
import os.log

/// Schedules refresh intents while Rust owns conditional-fetch policy, feed
/// normalization, durable metadata, and episode admission. Native networking
/// is executed by the typed `CoreFeedHost` capability.
@MainActor
final class SubscriptionRefreshService {
    static let shared = SubscriptionRefreshService()

    private static let logger = Logger.app("SubscriptionRefreshService")
    private var foregroundObserver: NSObjectProtocol?
    private weak var registeredStore: AppStateStore?

    func refresh(_ podcastID: UUID, store: AppStateStore) async throws {
        guard let sharedLibrary = store.sharedLibrary else {
            throw SharedLibraryError.unavailable
        }
        let priorIDs = Set(store.episodes(forPodcast: podcastID).map(\.id))
        let firstEverFetch = priorIDs.isEmpty
        _ = try await sharedLibrary.execute(.refreshPodcast(
            podcastId: PodcastId(uuid: podcastID)
        ))
        let insertedIDs = store.episodes(forPodcast: podcastID)
            .map(\.id)
            .filter { !priorIDs.contains($0) }
        store.recordSharedFeedDiscovery(
            podcastID: podcastID,
            episodeIDs: insertedIDs,
            notificationDiscoveredAt: firstEverFetch ? nil : Date()
        )
    }

    /// Refreshes followed podcasts in bounded batches. Rust remains the only
    /// writer even when native scheduling runs several independent commands.
    func refreshAll(store: AppStateStore, maxConcurrent: Int = 4) async {
        let podcasts = store.sortedFollowedPodcastsByRecency.filter { $0.feedURL != nil }
        guard !podcasts.isEmpty else { return }
        let bounded = max(1, maxConcurrent)
        var index = 0
        while index < podcasts.count {
            let upper = min(index + bounded, podcasts.count)
            let identifiers = podcasts[index..<upper].map(\.id)
            let tasks = identifiers.map { podcastID in
                Task { @MainActor [weak self, weak store] in
                    guard let self, let store else { return }
                    do {
                        try await self.refresh(podcastID, store: store)
                    } catch {
                        Self.logger.notice(
                            "shared refresh failed for \(podcastID, privacy: .public): \(error.localizedDescription, privacy: .public)"
                        )
                    }
                }
            }
            for task in tasks { await task.value }
            index = upper
        }
    }

    /// Refreshes at explicit lifecycle opportunities. Background cadence is
    /// delegated to `BGTaskScheduler`; no native polling loop owns policy.
    func startLifecycleRefresh(store: AppStateStore) {
        registeredStore = store
        registerLifecycleObserversIfNeeded()
        Task { @MainActor [weak self, weak store] in
            guard let self, let store else { return }
            await self.refreshAll(store: store)
        }
    }

    private func registerLifecycleObserversIfNeeded() {
        if foregroundObserver == nil {
            foregroundObserver = NotificationCenter.default.addObserver(
                forName: UIApplication.willEnterForegroundNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self, let store = self.registeredStore else { return }
                    Task { @MainActor in await self.refreshAll(store: store) }
                }
            }
        }
    }
}
