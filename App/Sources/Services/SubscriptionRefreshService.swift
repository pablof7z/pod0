import Foundation
import Pod0Core
import UIKit
import os.log

/// Records refresh intents while Rust owns conditional-fetch policy, feed
/// normalization, durable metadata, episode admission, coalescing, and retry.
/// Native networking is executed by the typed `CoreFeedHost` capability.
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
        _ = try await sharedLibrary.executeCommitted(.refreshPodcast(
            podcastId: PodcastId(uuid: podcastID)
        ))
    }

    /// Records a refresh intent for every followed podcast. Each command
    /// commits a durable workflow row immediately; admission, coalescing,
    /// and retry pacing are Rust workflow policy, so native no longer
    /// batches or bounds concurrency here.
    func refreshAll(store: AppStateStore) async {
        let podcasts = store.sortedFollowedPodcastsByRecency.filter { $0.feedURL != nil }
        for podcast in podcasts {
            do {
                try await refresh(podcast.id, store: store)
            } catch {
                Self.logger.notice(
                    "shared refresh failed for \(podcast.id, privacy: .public): \(error.localizedDescription, privacy: .public)"
                )
            }
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
