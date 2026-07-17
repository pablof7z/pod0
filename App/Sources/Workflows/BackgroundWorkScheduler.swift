import BackgroundTasks
import Foundation
import os.log

@MainActor
final class BackgroundWorkScheduler {
    static let shared = BackgroundWorkScheduler()
    nonisolated static let refreshIdentifier = "io.f7z.podcast.workflow.refresh"
    nonisolated static let processingIdentifier = "io.f7z.podcast.workflow.processing"

    private static let logger = Logger.app("BackgroundWorkScheduler")
    private weak var store: AppStateStore?
    private var isRegistered = false

    private init() {}

    func register() {
        guard !isRegistered else { return }
        isRegistered = true
        BGTaskScheduler.shared.register(
            forTaskWithIdentifier: Self.refreshIdentifier,
            using: .main
        ) { task in
            guard let task = task as? BGAppRefreshTask else {
                task.setTaskCompleted(success: false)
                return
            }
            MainActor.assumeIsolated { self.handleRefresh(task) }
        }
        BGTaskScheduler.shared.register(
            forTaskWithIdentifier: Self.processingIdentifier,
            using: .main
        ) { task in
            guard let task = task as? BGProcessingTask else {
                task.setTaskCompleted(success: false)
                return
            }
            MainActor.assumeIsolated { self.handleProcessing(task) }
        }
        schedule()
    }

    func attach(store: AppStateStore) {
        self.store = store
    }

    func schedule() {
        let refresh = BGAppRefreshTaskRequest(identifier: Self.refreshIdentifier)
        refresh.earliestBeginDate = Date().addingTimeInterval(15 * 60)
        let processing = BGProcessingTaskRequest(identifier: Self.processingIdentifier)
        processing.earliestBeginDate = Date().addingTimeInterval(20 * 60)
        processing.requiresNetworkConnectivity = true
        processing.requiresExternalPower = false
        submit(refresh, label: "refresh")
        submit(processing, label: "processing")
    }

    private func submit(_ request: BGTaskRequest, label: String) {
        do {
            try BGTaskScheduler.shared.submit(request)
        } catch {
            // The sibling request may already be pending. Submit each one
            // independently so that error cannot suppress re-arming the
            // opportunity that just ran.
            Self.logger.notice(
                "Unable to schedule background \(label, privacy: .public) work: \(error, privacy: .public)"
            )
        }
    }

    private func handleRefresh(_ task: BGAppRefreshTask) {
        guard let store else {
            schedule()
            task.setTaskCompleted(success: false)
            return
        }
        let opportunity = BackgroundOpportunity(
            resubmit: { [weak self] in self?.schedule() },
            complete: { success in task.setTaskCompleted(success: success) },
            cancel: { await WorkflowRuntime.shared.cancelActive() }
        )
        opportunity.start {
            await SubscriptionRefreshService.shared.refreshAll(store: store)
            await WorkflowRuntime.shared.reconcileAndDrain()
            return !Task.isCancelled
        }
        task.expirationHandler = {
            Task { @MainActor in opportunity.expire() }
        }
    }

    private func handleProcessing(_ task: BGProcessingTask) {
        let opportunity = BackgroundOpportunity(
            resubmit: { [weak self] in self?.schedule() },
            complete: { success in task.setTaskCompleted(success: success) },
            cancel: { await WorkflowRuntime.shared.cancelActive() }
        )
        opportunity.start {
            await WorkflowRuntime.shared.reconcileAndDrain()
            return !Task.isCancelled
        }
        task.expirationHandler = {
            Task { @MainActor in opportunity.expire() }
        }
    }
}
