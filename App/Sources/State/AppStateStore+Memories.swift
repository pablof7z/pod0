import Foundation

// MARK: - Agent Memories

extension AppStateStore {

    @discardableResult
    func addAgentMemory(content: String) -> AgentMemory {
        let memory = AgentMemory(content: content)
        mutateState { $0.agentMemories.append(memory) }
        return memory
    }

    func updateAgentMemory(_ id: UUID, content: String) {
        guard let idx = state.agentMemories.firstIndex(where: { $0.id == id }) else { return }
        mutateState { $0.agentMemories[idx].content = content }
    }

    func deleteAgentMemory(_ id: UUID) {
        guard let idx = state.agentMemories.firstIndex(where: { $0.id == id }) else { return }
        mutateState { $0.agentMemories[idx].deleted = true }
    }

    func restoreAgentMemory(_ id: UUID) {
        guard let idx = state.agentMemories.firstIndex(where: { $0.id == id }) else { return }
        mutateState { $0.agentMemories[idx].deleted = false }
    }

    func clearAllAgentMemories() {
        var updated = state.agentMemories
        for idx in updated.indices where !updated[idx].deleted {
            updated[idx].deleted = true
        }
        mutateState { $0.agentMemories = updated }
    }

    var activeMemories: [AgentMemory] {
        state.agentMemories.filter { !$0.deleted }
    }

    /// Replaces the compiled-memory snapshot. Called by `AgentMemoryCompiler`
    /// after a successful LLM compile, or with `nil` when the active memory
    /// set has been emptied. The existing `state.didSet` persistence path
    /// handles save.
    func setCompiledMemory(_ compiled: CompiledAgentMemory?) {
        mutateState { $0.compiledMemory = compiled }
    }
}
