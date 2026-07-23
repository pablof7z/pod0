import Observation
import Pod0Core

/// Transient, bounded presentation state for one Rust-fenced model request.
/// Durable messages remain exclusively in the shared-core projection.
@MainActor
@Observable
final class CoreAgentStreamingState {
    private(set) var turnID: AgentTurnId?
    private(set) var content: String?
    private(set) var isActive = false
    @ObservationIgnored private var fenceID: AgentExecutionFenceId?
    @ObservationIgnored private var maximumBytes: UInt64 = 0
    @ObservationIgnored private var pendingContent: String?
    @ObservationIgnored private var flushTask: Task<Void, Never>?

    func begin(
        turnID: AgentTurnId,
        fenceID: AgentExecutionFenceId,
        maximumBytes: UInt64
    ) {
        flushTask?.cancel()
        flushTask = nil
        self.turnID = turnID
        self.fenceID = fenceID
        self.maximumBytes = maximumBytes
        pendingContent = nil
        content = ""
        isActive = true
    }

    func update(
        turnID: AgentTurnId,
        fenceID: AgentExecutionFenceId,
        content: String
    ) {
        guard matches(turnID: turnID, fenceID: fenceID),
              UInt64(content.utf8.count) <= maximumBytes else { return }
        pendingContent = content
        guard flushTask == nil else { return }
        flushTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(50))
            guard !Task.isCancelled else { return }
            self?.flush()
        }
    }

    func finish(turnID: AgentTurnId, fenceID: AgentExecutionFenceId) {
        guard matches(turnID: turnID, fenceID: fenceID) else { return }
        flushTask?.cancel()
        flushTask = nil
        flush()
        isActive = false
    }

    func clear(turnID: AgentTurnId) {
        guard self.turnID == turnID else { return }
        flushTask?.cancel()
        flushTask = nil
        self.turnID = nil
        fenceID = nil
        maximumBytes = 0
        pendingContent = nil
        content = nil
        isActive = false
    }

    private func matches(
        turnID: AgentTurnId,
        fenceID: AgentExecutionFenceId
    ) -> Bool {
        self.turnID == turnID && self.fenceID == fenceID
    }

    private func flush() {
        if let pendingContent { content = pendingContent }
        pendingContent = nil
        flushTask = nil
    }
}
