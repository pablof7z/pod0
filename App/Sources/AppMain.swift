import os.log
import SwiftUI

/// The top-level entry point for the app. Sets up global environment objects.
@main
struct PodcastrApp: App {
    @Environment(\.scenePhase) private var scenePhase
    @UIApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    @State private var store: AppStateStore?
    /// Single global owner-consultation coordinator. Lives here so it can pop
    /// the same sheet even when the user is on Home / Library / Clippings.
    /// Mounted on `RootView` via `agentAskPresenter(coordinator:)`.
    @State private var askCoordinator = AgentAskCoordinator()
    /// Owns the only strong reference to the agent approval decider, which
    /// `CoreAgentHost` holds weakly. It presents nothing — see the type.
    @State private var approvalCoordinator = AgentApprovalCoordinator()
    @State private var workflows = WorkflowClient()
    @State private var suspensionPersistence = AppSuspensionPersistenceCoordinator()

    var body: some Scene {
        WindowGroup {
            if let store {
                RootView(approvalCoordinator: approvalCoordinator)
                    .environment(store)
                    .environment(askCoordinator)
                    .environment(workflows)
                    .task { await workflows.startAndReconcile() }
                    .onChange(of: scenePhase, initial: true) { _, phase in
                        Task {
                            await ProductSignalStore.shared.setSessionActive(phase == .active)
                        }
                        if phase == .background {
                            suspensionPersistence.persistForSuspension {
                                await store.flushForSuspension()
                            }
                        }
                    }
            } else {
                Pod0LaunchView()
                    .task {
                        guard AppLaunchEnvironment.shouldLoadProductionState() else {
                            return
                        }
                        store = await AppStateStore.production(
                            productSignals: ProductSignalStore.shared
                        )
                    }
            }
        }
    }
}

enum AppLaunchEnvironment {
    static func shouldLoadProductionState(
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> Bool {
        environment["XCTestConfigurationFilePath"] == nil
    }
}

private struct Pod0LaunchView: View {
    var body: some View {
        VStack(spacing: 12) {
            ProgressView()
            Text("Opening your library…")
                .font(.system(.subheadline, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(uiColor: .systemBackground))
        .accessibilityElement(children: .combine)
    }
}
