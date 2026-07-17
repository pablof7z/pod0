import Foundation

/// Testable lifecycle wrapper shared by BGAppRefresh and BGProcessing. iOS
/// chooses whether/when to grant a window; this only makes a granted window
/// resubmit promptly, cancel locally on expiry, and complete exactly once.
@MainActor
final class BackgroundOpportunity {
    typealias Operation = @MainActor @Sendable () async -> Bool

    private let complete: @MainActor (Bool) -> Void
    private let cancel: @MainActor @Sendable () async -> Void
    private var operationTask: Task<Void, Never>?
    private var completed = false

    init(
        resubmit: @MainActor () -> Void,
        complete: @escaping @MainActor (Bool) -> Void,
        cancel: @escaping @MainActor @Sendable () async -> Void
    ) {
        self.complete = complete
        self.cancel = cancel
        resubmit()
    }

    func start(_ operation: @escaping Operation) {
        guard operationTask == nil, !completed else { return }
        operationTask = Task { @MainActor [weak self] in
            let success = await operation()
            self?.finish(success: success && !Task.isCancelled)
        }
    }

    func expire() {
        guard !completed else { return }
        operationTask?.cancel()
        finish(success: false)
        Task { @MainActor [cancel] in await cancel() }
    }

    private func finish(success: Bool) {
        guard !completed else { return }
        completed = true
        complete(success)
    }
}
