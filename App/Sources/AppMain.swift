import os.log
import SwiftUI

/// The top-level entry point for the app. Sets up global environment objects.
@main
struct PodcastrApp: App {
    @Environment(\.scenePhase) private var scenePhase
    @UIApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @State private var store = AppStateStore(productSignals: ProductSignalStore.shared)
    /// Single global owner-consultation coordinator. Lives here (not on
    /// `AgentChatSession`) so it can pop the same sheet even when the user is
    /// on Home / Library / Clippings — i.e. while no chat session exists.
    /// Mounted on `RootView` via `agentAskPresenter(coordinator:)`.
    @State private var askCoordinator = AgentAskCoordinator()
    @State private var approvalCoordinator = AgentApprovalCoordinator()
    @State private var workflows = WorkflowClient()

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

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(store)
                .environment(askCoordinator)
                .environment(approvalCoordinator)
                .environment(workflows)
                .task { await workflows.startAndReconcile() }
                .onChange(of: scenePhase, initial: true) { _, phase in
                    Task { await ProductSignalStore.shared.setSessionActive(phase == .active) }
                }
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
    }
}

/// Drives the What's New `.sheet(item:)`. Bundling the entries with the
/// trigger guarantees the sheet content closure receives them atomically
/// — see the wiring note above.
private struct WhatsNewPresentation: Identifiable {
    let id = UUID()
    let entries: [WhatsNewEntry]
}
