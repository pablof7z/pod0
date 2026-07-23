import Foundation
import Observation
import Pod0Core

/// Native presentation queue for exact Rust-authored proposals. It reports
/// only approve or deny; it never edits arguments or decides authorization.
@MainActor
@Observable
final class AgentApprovalCoordinator: CoreAgentApprovalPresenting {
    struct PendingApproval: Identifiable, Equatable {
        let id: UUID
        let request: AgentApprovalRequest
    }

    private(set) var current: PendingApproval?
    @ObservationIgnored private var queue: [PendingApproval] = []
    @ObservationIgnored private var continuations: [UUID: CheckedContinuation<Bool, Never>] = [:]

    func requestApproval(_ request: AgentApprovalRequest) async -> Bool {
        let id = UUID()
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                continuations[id] = continuation
                queue.append(PendingApproval(id: id, request: request))
                if current == nil { current = queue.first }
            }
        } onCancel: {
            Task { @MainActor [weak self] in self?.resolve(id, approved: false) }
        }
    }

    func approve(_ id: UUID) {
        resolve(id, approved: true)
    }

    func deny(_ id: UUID) {
        resolve(id, approved: false)
    }

    private func resolve(_ id: UUID, approved: Bool) {
        guard let continuation = continuations.removeValue(forKey: id) else { return }
        queue.removeAll { $0.id == id }
        if current?.id == id { current = queue.first }
        continuation.resume(returning: approved)
    }
}
