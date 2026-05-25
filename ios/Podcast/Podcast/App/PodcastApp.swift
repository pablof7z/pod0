import AppIntents
import CoreSpotlight
import SwiftUI
import UIKit

@main
struct PodcastApp: App {
    // `@State` (not `@StateObject`) because KernelModel is now `@Observable`.
    @State private var model = KernelModel()

    // Compat shim — bridges legacy Identity views' `@Environment(UserIdentityStore.self)`
    // injection. Replaced when functional sign-in lands at M1 exit.
    @State private var identityStore = UserIdentityStore()

    // Spotlight (and future deep-link) router. Owned at the app root
    // so both `RootShell` (tab switch) and `LibraryView` (path push)
    // can read from the same one-slot mailbox.
    @State private var deepLinkRouter = SpotlightDeepLinkRouter()


    // UIKit app delegate is the only surface that receives
    // `application(_:handleEventsForBackgroundURLSession:completionHandler:)`,
    // which the OS calls when it relaunches the app to drain a
    // background download. The adaptor forwards that hook into
    // `PodcastCapabilities.shared.download` — see
    // `PodcastAppDelegate` below.
    @UIApplicationDelegateAdaptor(PodcastAppDelegate.self) private var appDelegate

    // T118 / G3 — iOS scenePhase observer. Routes `.active` / `.background`
    // to the kernel; silently drops `.inactive` (app-switcher interstitial —
    // the kernel's transition reducer would debounce it, but suppressing it
    // avoids a pointless FFI hop on every app-switch animation tick).
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootShell()
                .environment(model)
                .environment(identityStore)
                .environment(deepLinkRouter)
                .tint(PodcastColor.accent)
                .task {
                    model.start()
                    // iCloud settings sync attaches *after* the kernel is
                    // up so its inbound dispatches have a live destination.
                    // The capability holds a weak handle to the model, so
                    // an out-of-order shutdown is safe.
                    PodcastCapabilities.shared.startICloudSync(kernel: model)
                    // Re-register App Shortcuts with the system so Siri /
                    // Shortcuts / Spotlight pick up phrase changes after an
                    // install or upgrade. iOS caches the provider's output
                    // until this call (or a fresh install) refreshes it.
                    PodcastAppShortcuts.updateAppShortcutParameters()
                }
                // Spotlight tap → deep-link router. `RootShell` /
                // `LibraryView` watch the router's mailbox to flip
                // tabs and push the corresponding `NavigationStack`
                // destination. The handler is non-throwing — an
                // unrecognised activity is dropped (D6).
                .onContinueUserActivity(CSSearchableItemActionType) { activity in
                    deepLinkRouter.handle(activity)
                }
        }
        .onChange(of: scenePhase) { _, newPhase in
            // D7: Swift reports the fact; the kernel decides what each
            // phase MEANS (reconcile, throttle retries, etc.). No policy here.
            switch newPhase {
            case .active:
                // ADR-0028: pull-side actor-liveness probe before reporting
                // foreground so a dead kernel is not hit with a doomed command.
                model.checkAlive()
                model.lifecycleForeground()
            case .background:
                model.lifecycleBackground()
            case .inactive:
                break // transient — kernel never hears about it.
            @unknown default:
                break
            }
        }
    }
}

// MARK: - Background URLSession handoff

/// Minimal `UIApplicationDelegate` whose sole purpose is to forward the
/// background-`URLSession` relaunch hook into `DownloadCapability`.
///
/// SwiftUI's `App` protocol does not expose
/// `application(_:handleEventsForBackgroundURLSession:completionHandler:)`,
/// so we add a tiny adaptor to receive it. The delegate stays empty
/// otherwise — all other app-lifecycle wiring goes through SwiftUI's
/// `scenePhase` observer above.
final class PodcastAppDelegate: NSObject, UIApplicationDelegate {

    /// Called when iOS relaunches the app in the background to deliver
    /// accrued events for a background `URLSession`. We hand the
    /// completion handler to the capability; it invokes the handler
    /// after the session's
    /// `urlSessionDidFinishEvents(forBackgroundURLSession:)` fires.
    func application(
        _ application: UIApplication,
        handleEventsForBackgroundURLSession identifier: String,
        completionHandler: @escaping () -> Void
    ) {
        // `PodcastCapabilities.shared` is `@MainActor`-isolated. The OS
        // calls this entry point on the main thread; the hop is
        // synchronous via `MainActor.assumeIsolated` so the OS still has
        // the completion handler stashed before any delegate event lands.
        MainActor.assumeIsolated {
            PodcastCapabilities.shared.download.handleEventsForBackgroundURLSession(
                identifier: identifier,
                completionHandler: completionHandler)
        }
    }
}
