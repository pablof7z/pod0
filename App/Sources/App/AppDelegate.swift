import UIKit
import UserNotifications
import os.log

// MARK: - App Delegate

/// Handles UIKit lifecycle events that pure SwiftUI cannot receive:
/// - App-icon quick-action (home-screen shortcut) selection.
/// - Foreground notification presentation.
///
/// Wired in via `@UIApplicationDelegateAdaptor` in `AppMain`.
final class AppDelegate: NSObject, UIApplicationDelegate {
    private let logger = Logger.app("AppDelegate")

    // MARK: - Pending shortcut

    /// Shortcut selected while a scene was connecting (cold-launch path).
    /// `RootView` reads this on `.onAppear` and clears it after routing.
    var pendingShortcutURL: URL?

    // MARK: - UIApplicationDelegate

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        UNUserNotificationCenter.current().delegate = self
#if DEBUG
        if WorkflowProcessReconstructionHarness.runIfRequested() { return true }
#endif
        BackgroundWorkScheduler.shared.register()
        // Bound Kingfisher's memory + disk caches so artwork doesn't grow
        // unchecked. See KingfisherConfiguration for the rationale.
        KingfisherConfiguration.configure()
        return true
    }

    func application(
        _ application: UIApplication,
        configurationForConnecting connectingSceneSession: UISceneSession,
        options: UIScene.ConnectionOptions
    ) -> UISceneConfiguration {
        let configuration = UISceneConfiguration(
            name: nil,
            sessionRole: connectingSceneSession.role
        )
        if connectingSceneSession.role == .windowApplication {
            configuration.delegateClass = AppSceneDelegate.self
        }
        return configuration
    }

    func application(
        _ application: UIApplication,
        handleEventsForBackgroundURLSession identifier: String,
        completionHandler: @escaping () -> Void
    ) {
        Task { @MainActor in
            if identifier == CoreDownloadHost.backgroundSessionIdentifier {
                CoreDownloadHost.shared.handleEventsForBackgroundURLSession(
                    identifier: identifier,
                    completionHandler: completionHandler
                )
                return
            }
            EpisodeDownloadService.shared.handleEventsForBackgroundURLSession(
                identifier: identifier,
                completionHandler: completionHandler
            )
        }
    }

    /// Maps an `UIApplicationShortcutItem.type` to a `podcastr://` deep-link
    /// the rest of the app already knows how to route via `DeepLinkHandler`.
    /// The bundle-id prefix is stripped so the suffix alone identifies the
    /// destination — keeps this in sync with whatever bundle ID `Project.swift`
    /// resolves to today.
    static func deepLinkURL(
        for shortcut: UIApplicationShortcutItem,
        bundleIdentifier: String = Bundle.main.bundleIdentifier ?? ""
    ) -> URL? {
        let bundleID = bundleIdentifier
        let prefix = bundleID + "."
        let suffix = shortcut.type.hasPrefix(prefix)
            ? String(shortcut.type.dropFirst(prefix.count))
            : shortcut.type
        switch suffix {
        case "open-agent": return URL(string: "podcastr://agent")
        case "settings":   return URL(string: "podcastr://settings")
        default:           return nil
        }
    }

    @discardableResult
    func handleShortcut(
        _ shortcut: UIApplicationShortcutItem,
        delivery: ShortcutDelivery,
        bundleIdentifier: String = Bundle.main.bundleIdentifier ?? "",
        notificationCenter: NotificationCenter = .default
    ) -> Bool {
        guard let url = Self.deepLinkURL(
            for: shortcut,
            bundleIdentifier: bundleIdentifier
        ) else {
            logger.warning("Unhandled quick action: \(shortcut.type, privacy: .public)")
            return false
        }
        switch delivery {
        case .pending:
            pendingShortcutURL = url
        case .notification:
            notificationCenter.post(name: Self.shortcutURLNotification, object: url)
        }
        return true
    }
}

extension AppDelegate {
    enum ShortcutDelivery {
        case pending
        case notification
    }
}

// MARK: - UNUserNotificationCenterDelegate

extension AppDelegate: UNUserNotificationCenterDelegate {

    /// Shows banners even when the app is in the foreground (e.g. during testing).
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound, .badge])
    }

    /// Routes notification taps. Only new-episode notifications carry an
    /// `episodeID` payload — for those we synthesize a `podcastr://episode/<uuid>`
    /// deep-link and post it through `shortcutURLNotification`, which `RootView`
    /// already observes and routes via `handleDeepLink(_:)`.
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        defer { completionHandler() }
        let userInfo = response.notification.request.content.userInfo
        guard let episodeID = userInfo[NotificationService.episodeIDUserInfoKey] as? String,
              UUID(uuidString: episodeID) != nil,
              let url = URL(string: "podcastr://episode/\(episodeID)")
        else { return }
        // Hop onto the main actor to post — RootView listens on the main queue.
        Task { @MainActor in
            NotificationCenter.default.post(
                name: AppDelegate.shortcutURLNotification,
                object: url
            )
        }
    }
}

// MARK: - Notification names

extension AppDelegate {
    /// Posted when a quick-action URL is ready to route.
    static let shortcutURLNotification = Notification.Name("AppDelegate.shortcutURL")
}
