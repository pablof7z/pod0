import Foundation
import UserNotifications
import os.log

/// Native notification authorization and routing constants.
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
    nonisolated static let occurrenceIDUserInfoKey = "occurrenceID"

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
}
