import SwiftUI

/// The top-level entry point for the app. Sets up global environment objects
/// and wires the Nostr relay service to relevant settings changes.
@main
struct PodcastrApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @State private var kernelModel = KernelModel()
    @State private var store = AppStateStore()
    /// Single global owner-consultation coordinator. Lives here (not on
    /// `AgentChatSession`) so an inbound peer-agent reply flowing through
    /// `AgentRelayBridge` can pop the same sheet even when the user is on
    /// Home / Library — i.e. while no chat session exists. Mounted
    /// on `RootView` via `agentAskPresenter(coordinator:)`.
    @State private var askCoordinator = AgentAskCoordinator()

    // MARK: - What's-new sheet wiring
    //
    // Evaluated once on cold launch (`.task` below). Stays here in
    // `AppMain.swift` rather than `RootView.swift` so the "what changed
    // since you last opened the app" check fires before any tab-level
    // view has a chance to short-circuit it.
    //
    // Single optional `@State` + `.sheet(item:)` rather than the more
    // common pair of `entries: [...]` and `isPresented: Bool`. The
    // `OnboardingView` fullScreenCover sits on top of RootView during
    // first launch, and SwiftUI re-evaluates the queued sheet's content
    // closure once the cover dismisses. With the two-state pattern the
    // closure was reading a stale `entries = []` from across that
    // render boundary, so the sheet rendered empty. `.sheet(item:)`
    // passes the entries through the trigger itself, eliminating the
    // race.
    @State private var whatsNewPresentation: WhatsNewPresentation?

    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(kernelModel)
                .environment(store)
                .environment(askCoordinator)
                .task {
                    kernelModel.start()
                    let platform = PodcastCapabilities.shared.platform
                    store.onNowPlayingSnapshot = { [platform] snap in
                        platform.applyWidgetSnapshot(snap)
                    }
                    store.onPositionTick = { [platform] pos in
                        platform.applyPositionTick(pos)
                    }
                    store.attachKernel(kernelModel)
                    PodcastCapabilities.shared.startICloudSync(kernel: kernelModel, appStore: store)
                }
                .task { store.identity.start() }
                .task { CarPlayController.shared.attach(store: store) }
                .task {
                    // Seed a fresh install silently so the first launch
                    // doesn't dump the entire changelog as "new."
                    WhatsNewService.seedIfNeeded()
                    let unseen = WhatsNewService.unseenEntries(
                        lastSeenAt: WhatsNewService.lastSeenAt
                    )
                    if !unseen.isEmpty {
                        whatsNewPresentation = WhatsNewPresentation(entries: unseen)
                    }
                }
                .sheet(item: $whatsNewPresentation) { presentation in
                    WhatsNewSheet(entries: presentation.entries)
                }
        }
        .onChange(of: scenePhase) { _, newPhase in
            switch newPhase {
            case .active:
                kernelModel.checkAlive()
                kernelModel.lifecycleForeground()
                DiagnosticLog.shared.append(
                    level: .info, category: "lifecycle", message: "app foreground")
            case .background:
                kernelModel.lifecycleBackground()
                DiagnosticLog.shared.append(
                    level: .info, category: "lifecycle", message: "app background")
            case .inactive:
                break
            @unknown default:
                break
            }
        }
    }
}

/// Drives the What's New `.sheet(item:)`. Bundling the entries with the
/// trigger guarantees the sheet content closure receives them atomically
/// — see the wiring note above.
private struct WhatsNewPresentation: Identifiable {
    let id = UUID()
    let entries: [WhatsNewEntry]
}
