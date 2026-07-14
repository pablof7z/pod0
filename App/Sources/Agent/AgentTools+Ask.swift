import Foundation

// MARK: - Owner-consultation tool

extension AgentTools {

    /// Pops the agent-ask sheet in front of the owner, waits for a typed
    /// answer (or decline / timeout), and returns it. The `coordinator`
    /// handles queueing — concurrent asks from parallel peer-agent
    /// conversations are serialized one sheet at a time. Returns a
    /// JSON error envelope when no coordinator is wired (headless or
    /// background contexts where there is no UI surface to prompt the
    /// owner), so the LLM loop keeps running rather than crashing.
    ///
    /// Takes primitive `String` / `String?` rather than the raw `args`
    /// dictionary so nothing non-Sendable crosses the `await` boundary —
    /// the caller in `AgentTools.dispatch` extracts the fields
    /// synchronously before invoking.
    static func askOwnerTool(
        question: String,
        context: String?,
        coordinator: AgentAskCoordinator?
    ) async -> String {
        guard let coordinator else {
            return toolError("ask is unavailable in this context — no UI surface to prompt the owner.")
        }
        let trimmedQuestion = question.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedQuestion.isEmpty else {
            return toolError("Missing or empty 'question'")
        }
        let trimmedContext = context?.trimmingCharacters(in: .whitespacesAndNewlines)
        let normalizedContext = (trimmedContext?.isEmpty ?? true) ? nil : trimmedContext

        let answer = await coordinator.ask(question: trimmedQuestion, context: normalizedContext)
        return toolSuccess(["answer": answer])
    }
}
