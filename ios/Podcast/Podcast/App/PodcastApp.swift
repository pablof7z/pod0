import SwiftUI
import UIKit

@main
struct PodcastApp: App {
    // `@State` (not `@StateObject`) because KernelModel is now `@Observable`.
    @State private var model = KernelModel()

    // Compat shim — bridges legacy Identity views' `@Environment(UserIdentityStore.self)`
    // injection. Replaced when functional sign-in lands at M1 exit.
    @State private var identityStore = UserIdentityStore()

    // Compat store — holds podcast/episode state surfaced by Library views.
    // Replaced when Rust snapshot projection drives AppStateStore directly.
    @State private var appStore = AppStateStore()

    // Shared playback state for the audio layer.
    @State private var playback = PlaybackState()

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
                .environment(appStore)
                .environment(playback)
                .tint(PodcastColor.accent)
                .task { model.start() }
                .onChange(of: model.library) { _, newLibrary in
                    appStore.allPodcasts = newLibrary.map { summary in
                        var p = Podcast()
                        p.id = UUID(uuidString: summary.id) ?? UUID()
                        p.title = summary.title
                        if let urlStr = summary.artworkUrl { p.imageURL = URL(string: urlStr) }
                        return p
                    }
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
