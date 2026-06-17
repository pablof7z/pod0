import Foundation
import Observation
import os.log

/// Mediates the agent's `ask` tool: when the model wants to consult the
/// owner mid-loop, the dispatcher calls `ask(question:context:kernel:)`, which
/// suspends until the owner answers, declines, or the timeout fires.
///
/// Rust owns FIFO ordering, current-row promotion, timeout expiry, and the final
/// tool-result envelope. Swift owns only modal presentation, continuation
/// parking, and reporting raw owner actions back to Rust.
@MainActor
@Observable
final class AgentAskCoordinator {

    /// Identifies a single pending question. The `CheckedContinuation`
    /// is deliberately stored separately (in `continuations`) — it is
    /// not `Sendable` and must not leak through this `Identifiable`
    /// struct that SwiftUI captures for `.sheet(item:)`.
    struct PendingAsk: Identifiable, Equatable {
        let id: UUID
        let question: String
        let context: String?
        let createdAt: Date
        let timeoutSeconds: UInt64
    }

    /// Drives the presenter's `.sheet(item:)`. Always equals `queue.first`.
    /// Read-only from the view layer; mutated only inside this type.
    private(set) var current: PendingAsk?

    @ObservationIgnored private let logger = Logger.app("AgentAskCoordinator")
    @ObservationIgnored private var continuations: [UUID: CheckedContinuation<String, Never>] = [:]
    @ObservationIgnored private weak var kernel: KernelModel?

    init() {}

    // MARK: - Tool surface

    /// Suspends until the owner answers, declines, or the timeout fires.
    /// Returns Rust's already-serialized tool envelope.
    func ask(question: String, context: String?, kernel: KernelModel?) async -> String {
        guard let kernel else {
            return "{\"error\":\"ask is unavailable in this context — no kernel surface to prompt the owner.\"}"
        }
        self.kernel = kernel
        kernel.onAgentAskEvent = { [weak self] response in
            self?.handleKernelAskEvent(response)
        }
        guard let response = kernel.agentAskEnqueue(question: question, context: context) else {
            return "{\"error\":\"ask is unavailable in this context — kernel ask queue unavailable.\"}"
        }
        if let result = response.result {
            applyCurrent(response.current)
            return result
        }
        guard response.ok, let pending = makePending(response.enqueued) else {
            return response.result ?? "{\"error\":\"ask was rejected by the kernel.\"}"
        }
        return await withCheckedContinuation { (cont: CheckedContinuation<String, Never>) in
            continuations[pending.id] = cont
            applyCurrent(response.current)
            logger.debug("Enqueued ask \(pending.id.uuidString, privacy: .public)")
        }
    }

    func handleKernelAskEvent(_ response: KernelModel.AgentAskResponse) {
        guard let settledID = response.settledId,
              let id = UUID(uuidString: settledID),
              let cont = continuations.removeValue(forKey: id)
        else {
            applyCurrent(response.current)
            return
        }
        applyCurrent(response.current)
        cont.resume(returning: response.result ?? "{\"error\":\"ask did not return a result\"}")
    }

    // MARK: - UI surface

    /// Called by the sheet when the owner finishes with a non-empty
    /// answer. Idempotent: a second call for the same id is a no-op
    /// (covers swipe-dismiss after resolve, double-tap on Send, etc.).
    func resolve(_ id: UUID, with answer: String) {
        finish(id, outcome: "answer", answer: answer, reason: "answered")
    }

    /// Called when the owner taps Decline or swipes the sheet down
    /// without answering. Idempotent for the same reason as `resolve`.
    func decline(_ id: UUID) {
        finish(id, outcome: "decline", answer: nil, reason: "declined")
    }

    // MARK: - Internals

    /// Single resolution path. Resumes the continuation exactly once
    /// (the `removeValue` ensures the second caller finds nothing) and
    /// keeps current / timeout state consistent with Rust.
    private func finish(_ id: UUID, outcome: String, answer: String?, reason: String) {
        guard let response = kernel?.agentAskSettle(id: id.uuidString, outcome: outcome, answer: answer) else {
            return
        }
        guard let cont = continuations.removeValue(forKey: id) else {
            // Already resolved — late timeout, double swipe, etc.
            applyCurrent(response.current)
            return
        }
        applyCurrent(response.current)
        logger.debug("Resolved ask \(id.uuidString, privacy: .public) via \(reason, privacy: .public)")
        cont.resume(returning: response.result ?? "{\"error\":\"ask did not return a result\"}")
    }

    private func applyCurrent(_ pending: KernelModel.AgentAskPending?) {
        current = makePending(pending)
    }

    private func makePending(_ pending: KernelModel.AgentAskPending?) -> PendingAsk? {
        guard let pending,
              let id = UUID(uuidString: pending.id)
        else { return nil }
        return PendingAsk(
            id: id,
            question: pending.question,
            context: pending.context,
            createdAt: Date(timeIntervalSince1970: TimeInterval(pending.createdAt)),
            timeoutSeconds: pending.timeoutSeconds
        )
    }
}
