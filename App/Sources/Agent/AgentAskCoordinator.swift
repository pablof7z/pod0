import Foundation
import Observation
import os.log

/// Mediates the agent's `ask` tool: when the model wants to consult the
/// owner mid-loop, the dispatcher calls `ask(question:context:)`, which
/// suspends until the owner answers, declines, or the 5-minute timeout
/// fires.
///
/// One sheet at a time. Concurrent asks (e.g. two peer-agent conversations
/// landing simultaneously) FIFO-queue; the head is published as `current`
/// and `RootView` presents it via `.sheet(item:)` through
/// `AgentAskPresenter`. Each ask resolves through its
/// `CheckedContinuation` exactly once — `resolve` and `decline` are no-ops
/// if the id is already settled, so a late timeout (or a swipe-dismiss
/// after a resolve) cannot double-resume.
///
/// Ported from the win-the-day app's `AgentAskCoordinator` and adapted
/// to podcast's `@Observable` / Swift Concurrency style. Keeps the same
/// invariants: FIFO queue, 5-minute timeout, late-event dedup, single
/// continuation resume per ask.
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
    }

    /// Drives the presenter's `.sheet(item:)`. Always equals `queue.first`.
    /// Read-only from the view layer; mutated only inside this type.
    private(set) var current: PendingAsk?

    /// 5-minute hard timeout. Matches the spec contract returned to the
    /// agent ("user did not respond within 5 minutes"). Static so callers
    /// can reference it in tests without instantiating the coordinator.
    static let timeoutSeconds: TimeInterval = 5 * 60

    @ObservationIgnored private let logger = Logger.app("AgentAskCoordinator")
    @ObservationIgnored private var queue: [PendingAsk] = []
    @ObservationIgnored private var continuations: [UUID: CheckedContinuation<String, Never>] = [:]
    @ObservationIgnored private var timeoutTasks: [UUID: Task<Void, Never>] = [:]

    init() {}

    // MARK: - Tool surface

    /// Suspends until the owner answers, declines, or the 5-minute
    /// timeout fires. Always returns a string (never throws) so the
    /// dispatcher can wrap the result in a small JSON envelope. The
    /// caller observes one of three sentinel outcomes:
    ///   • the owner's transcribed / typed answer,
    ///   • `"user declined to answer"` on explicit decline / dismiss,
    ///   • `"user did not respond within 5 minutes"` on timeout.
    func ask(question: String, context: String?) async -> String {
        let trimmedQuestion = question.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedContext = context?.trimmingCharacters(in: .whitespacesAndNewlines)
        let id = UUID()
        let pending = PendingAsk(
            id: id,
            question: trimmedQuestion.isEmpty ? "(no question)" : trimmedQuestion,
            context: (trimmedContext?.isEmpty ?? true) ? nil : trimmedContext,
            createdAt: Date()
        )

        return await withCheckedContinuation { (cont: CheckedContinuation<String, Never>) in
            continuations[id] = cont
            queue.append(pending)
            promoteHeadIfNeeded()
            armTimeout(for: id)
            logger.debug("Enqueued ask \(id.uuidString, privacy: .public); queue depth \(self.queue.count)")
        }
    }

    // MARK: - UI surface

    /// Called by the sheet when the owner finishes with a non-empty
    /// answer. Idempotent: a second call for the same id is a no-op
    /// (covers swipe-dismiss after resolve, double-tap on Send, etc.).
    func resolve(_ id: UUID, with answer: String) {
        finish(id, with: answer, reason: "answered")
    }

    /// Called when the owner taps Decline or swipes the sheet down
    /// without answering. Idempotent for the same reason as `resolve`.
    func decline(_ id: UUID) {
        finish(id, with: "user declined to answer", reason: "declined")
    }

    // MARK: - Internals

    private func promoteHeadIfNeeded() {
        if current == nil {
            current = queue.first
        }
    }

    private func armTimeout(for id: UUID) {
        // The Task inherits this type's `@MainActor` isolation, so
        // `finish` is a direct (non-suspending) call — the only real
        // suspend point is `Task.sleep`.
        let task = Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(Self.timeoutSeconds * 1_000_000_000))
            guard !Task.isCancelled else { return }
            self?.finish(id, with: "user did not respond within 5 minutes", reason: "timeout")
        }
        timeoutTasks[id] = task
    }

    /// Single resolution path. Resumes the continuation exactly once
    /// (the `removeValue` ensures the second caller finds nothing) and
    /// keeps queue / current / timeout state consistent.
    private func finish(_ id: UUID, with answer: String, reason: String) {
        guard let cont = continuations.removeValue(forKey: id) else {
            // Already resolved — late timeout, double swipe, etc.
            return
        }
        timeoutTasks.removeValue(forKey: id)?.cancel()
        queue.removeAll { $0.id == id }
        if current?.id == id {
            current = queue.first
        }
        logger.debug("Resolved ask \(id.uuidString, privacy: .public) via \(reason, privacy: .public)")
        cont.resume(returning: answer)
    }
}
