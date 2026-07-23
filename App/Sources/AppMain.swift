import os.log
import SwiftUI

/// The top-level entry point for the app. Sets up global environment objects.
@main
struct PodcastrApp: App {
    @Environment(\.scenePhase) private var scenePhase
    @UIApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @State private var store = AppStateStore(productSignals: ProductSignalStore.shared)
    /// Single global owner-consultation coordinator. Lives here so it can pop
    /// the same sheet even when the user is on Home / Library / Clippings.
    /// Mounted on `RootView` via `agentAskPresenter(coordinator:)`.
    @State private var askCoordinator = AgentAskCoordinator()
    @State private var approvalCoordinator = AgentApprovalCoordinator()
    @State private var workflows = WorkflowClient()
    @State private var suspensionPersistence = AppSuspensionPersistenceCoordinator()

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
                    if phase == .background {
                        suspensionPersistence.persistForSuspension {
                            await store.flushForSuspension()
                        }
                    }
                }
        }
    }
}
