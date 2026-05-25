import SwiftUI

@main
struct PodcastApp: App {
    @StateObject private var model = KernelModel()

    // T118 / G3 — iOS scenePhase observer. Routes `.active` / `.background`
    // to the kernel; silently drops `.inactive` (app-switcher interstitial —
    // the kernel's transition reducer would debounce it, but suppressing it
    // avoids a pointless FFI hop on every app-switch animation tick).
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootShell()
                .environmentObject(model)
                .tint(PodcastColor.accent)
                .task { model.start() }
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
