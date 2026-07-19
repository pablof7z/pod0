import UIKit

/// Owns scene-scoped home-screen quick-action delivery. SwiftUI still owns
/// scene presentation and navigation; this adapter only turns UIKit's raw
/// shortcut item into the app's existing typed deep-link handoff.
final class AppSceneDelegate: NSObject, UIWindowSceneDelegate {
    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let shortcut = connectionOptions.shortcutItem,
              let appDelegate = UIApplication.shared.delegate as? AppDelegate
        else { return }
        appDelegate.handleShortcut(shortcut, delivery: .pending)
    }

    func windowScene(
        _ windowScene: UIWindowScene,
        performActionFor shortcutItem: UIApplicationShortcutItem,
        completionHandler: @escaping (Bool) -> Void
    ) {
        guard let appDelegate = UIApplication.shared.delegate as? AppDelegate else {
            completionHandler(false)
            return
        }
        completionHandler(appDelegate.handleShortcut(shortcutItem, delivery: .notification))
    }
}
