import Foundation
import UserNotifications
import os.log

/// Local notifications for non-task surfaces (currently: Nostr contact requests).
@MainActor
enum NotificationService {

    private static let logger = Logger.app("NotificationService")

    /// `userInfo` key carrying the new episode's UUID string. Exposed so
    /// `AppDelegate` reads the same constant the writer uses — the
    /// previous shape duplicated the literal `"episodeID"` on the
    /// consumer side, so a rename of the writer-side constant would
    /// silently break notification-tap routing.
    ///
    /// `nonisolated` because the consumer (`AppDelegate`'s
    /// `userNotificationCenter(_:didReceive:...)`) is non-isolated and
    /// it's a plain `String` constant — no actor crossing concern.
    nonisolated static let episodeIDUserInfoKey = "episodeID"

    private enum Content {
        static let approvalTitle = "New contact request"
        static let approvalBody = "Someone wants to reach your agent. Open the app to review."
        static let approvalIDPrefix = "nostr-approval:"
        static let newEpisodeIDPrefix = "new-episode:"
    }

    /// Fan-out cap: at most this many new-episode notifications fire per
    /// subscription per refresh. Prevents a feed that just dumped its archive
    /// (or a one-off `If-None-Match` cache miss) from blowing up the user's
    /// notification stack with hundreds of banners.
    static let maxNewEpisodeNotificationsPerRefresh = 3

    // MARK: - Authorization

    /// Requests authorization for alerts, sounds, and badges.
    /// Returns `true` if permission was granted (or already granted).
    @discardableResult
    static func requestAuthorization() async -> Bool {
        let center = UNUserNotificationCenter.current()
        let settings = await center.notificationSettings()

        switch settings.authorizationStatus {
        case .authorized, .provisional, .ephemeral:
            return true
        case .denied:
            return false
        case .notDetermined:
            do {
                return try await center.requestAuthorization(options: [.alert, .sound, .badge])
            } catch {
                logger.error("requestAuthorization failed: \(error, privacy: .public)")
                return false
            }
        @unknown default:
            return false
        }
    }

    // MARK: - Nostr approval

    /// Fires an immediate notification when an unknown Nostr sender requests access.
    /// Deduped by pubkey — won't fire again if one is already pending for that key.
    static func notifyPendingApproval(pubkeyHex: String) async {
        let center = UNUserNotificationCenter.current()
        let pending = await center.pendingNotificationRequests()
        let id = "\(Content.approvalIDPrefix)\(pubkeyHex)"
        guard !pending.contains(where: { $0.identifier == id }) else { return }

        let granted = await requestAuthorization()
        guard granted else { return }

        let content = UNMutableNotificationContent()
        content.title = Content.approvalTitle
        content.body = Content.approvalBody
        content.sound = .default

        let request = UNNotificationRequest(identifier: id, content: content, trigger: nil)
        do {
            try await center.add(request)
        } catch {
            logger.error("notifyPendingApproval failed for pubkey \(pubkeyHex, privacy: .public): \(error, privacy: .public)")
        }
    }

    // MARK: - New-episode notifications

    /// Fires one local notification per `newEpisodes` entry, capped at
    /// ``maxNewEpisodeNotificationsPerRefresh`` per call. Title is the
    /// subscription's display title; body is `"New episode: <title>"`.
    /// The episode UUID rides in `userInfo["episodeID"]` so the
    /// `AppDelegate` tap handler can deep-link straight into the right detail
    /// view.
    ///
    /// The caller is expected to filter by the user's subscription state
    /// and to compute the actual delta — this function trusts both invariants.
    static func notifyNewEpisodes(
        _ newEpisodes: [Episode],
        podcast: Podcast
    ) async {
        guard !newEpisodes.isEmpty else { return }
        let granted = await requestAuthorization()
        guard granted else { return }

        let center = UNUserNotificationCenter.current()
        let capped = newEpisodes.prefix(maxNewEpisodeNotificationsPerRefresh)

        for episode in capped {
            let id = "\(Content.newEpisodeIDPrefix)\(episode.id.uuidString)"
            let content = UNMutableNotificationContent()
            content.title = podcast.title
            content.body = "New episode: \(episode.title)"
            content.sound = .default
            content.userInfo = [Self.episodeIDUserInfoKey: episode.id.uuidString]
            // Threading by podcast so iOS groups multiple new-episode banners
            // from the same show into one stack on the lock screen.
            content.threadIdentifier = "podcast:\(podcast.id.uuidString)"

            let request = UNNotificationRequest(identifier: id, content: content, trigger: nil)
            do {
                try await center.add(request)
            } catch {
                logger.error(
                    "notifyNewEpisode failed for \(episode.id.uuidString, privacy: .public): \(error, privacy: .public)"
                )
            }
        }
    }
}
