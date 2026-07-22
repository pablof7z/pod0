import Foundation
import Pod0Core

protocol CoreRecallHosting: Sendable {
    func execute(_ request: HostRequest) async -> HostObservation
}

struct UnavailableCoreRecallHost: CoreRecallHosting {
    func execute(_ request: HostRequest) async -> HostObservation {
        .failed(code: .indexUnavailable, safeDetail: "Recall capabilities are not attached")
    }
}

actor DeferredRecallHost: CoreRecallHosting {
    private var host: (any CoreRecallHosting)?
    private var waiters: [UUID: CheckedContinuation<(any CoreRecallHosting)?, Never>] = [:]

    func attach(_ host: any CoreRecallHosting) {
        self.host = host
        let pending = waiters.values
        waiters.removeAll()
        for continuation in pending { continuation.resume(returning: host) }
    }

    func execute(_ request: HostRequest) async -> HostObservation {
        guard let host = await resolvedHost() else { return .cancelled }
        return await host.execute(request)
    }

    private func resolvedHost() async -> (any CoreRecallHosting)? {
        if let host { return host }
        let id = UUID()
        return await withTaskCancellationHandler {
            await withCheckedContinuation {
                (continuation: CheckedContinuation<(any CoreRecallHosting)?, Never>) in
                if let host {
                    continuation.resume(returning: host)
                } else if Task.isCancelled {
                    continuation.resume(returning: nil)
                } else {
                    waiters[id] = continuation
                }
            }
        } onCancel: {
            Task { await self.cancelWaiter(id) }
        }
    }

    private func cancelWaiter(_ id: UUID) {
        waiters.removeValue(forKey: id)?.resume(returning: nil)
    }
}
