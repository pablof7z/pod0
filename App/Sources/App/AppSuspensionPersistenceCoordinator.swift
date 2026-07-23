import UIKit

@MainActor
protocol AppBackgroundExecutionHosting: AnyObject {
    func begin(expirationHandler: @escaping @MainActor @Sendable () -> Void) -> Int?
    func end(_ token: Int)
}

@MainActor
final class UIApplicationBackgroundExecutionHost: AppBackgroundExecutionHosting {
    func begin(expirationHandler: @escaping @MainActor @Sendable () -> Void) -> Int? {
        var identifier = UIBackgroundTaskIdentifier.invalid
        identifier = UIApplication.shared.beginBackgroundTask(
            withName: "Persist Pod0 state"
        ) {
            Task { @MainActor in expirationHandler() }
        }
        return identifier == .invalid ? nil : identifier.rawValue
    }

    func end(_ token: Int) {
        UIApplication.shared.endBackgroundTask(UIBackgroundTaskIdentifier(rawValue: token))
    }
}

/// Owns only the iOS background-execution lease. Persistence ordering and
/// durability remain inside `Persistence`.
@MainActor
final class AppSuspensionPersistenceCoordinator {
    private let backgroundExecution: any AppBackgroundExecutionHosting
    private var backgroundToken: Int?
    private var flushTask: Task<Void, Never>?

    init(
        backgroundExecution: any AppBackgroundExecutionHosting =
            UIApplicationBackgroundExecutionHost()
    ) {
        self.backgroundExecution = backgroundExecution
    }

    func persistForSuspension(
        flush: @escaping @MainActor @Sendable () async -> Bool
    ) {
        guard flushTask == nil else { return }
        backgroundToken = backgroundExecution.begin { [weak self] in
            self?.expire()
        }
        flushTask = Task { @MainActor [weak self] in
            _ = await flush()
            self?.finish()
        }
    }

    func waitUntilIdle() async {
        let task = flushTask
        await task?.value
    }

    private func expire() {
        flushTask?.cancel()
        endBackgroundExecution()
    }

    private func finish() {
        endBackgroundExecution()
        flushTask = nil
    }

    private func endBackgroundExecution() {
        guard let backgroundToken else { return }
        self.backgroundToken = nil
        backgroundExecution.end(backgroundToken)
    }
}
