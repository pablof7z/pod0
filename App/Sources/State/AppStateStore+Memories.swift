import Foundation

// MARK: - Agent Memories

extension AppStateStore {

    @discardableResult
    func addAgentMemory(content: String) async -> AgentMemory? {
        do {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            return try await sharedLibrary.createMemory(content: content)
        } catch {
            Self.logger.error(
                "Shared memory creation failed: \(error.localizedDescription, privacy: .public)"
            )
            return nil
        }
    }

    @discardableResult
    func updateAgentMemory(_ id: UUID, content: String) async -> Bool {
        do {
            guard let memory = state.agentMemories.first(where: { $0.id == id }),
                  let sharedLibrary
            else { throw SharedLibraryError.notFound }
            try await sharedLibrary.updateMemory(memory, content: content)
            return true
        } catch {
            Self.logger.error(
                "Shared memory update failed: \(error.localizedDescription, privacy: .public)"
            )
            return false
        }
    }

    @discardableResult
    func deleteAgentMemory(_ id: UUID) async -> Bool {
        await setAgentMemoryDeleted(id, deleted: true)
    }

    @discardableResult
    func restoreAgentMemory(_ id: UUID) async -> Bool {
        await setAgentMemoryDeleted(id, deleted: false)
    }

    @discardableResult
    func clearAllAgentMemories() async -> Bool {
        do {
            guard let sharedLibrary else { throw SharedLibraryError.unavailable }
            try await sharedLibrary.clearMemories()
            return true
        } catch {
            Self.logger.error(
                "Shared memory clear failed: \(error.localizedDescription, privacy: .public)"
            )
            return false
        }
    }

    var activeMemories: [AgentMemory] {
        state.agentMemories.filter { !$0.deleted }
    }

    private func setAgentMemoryDeleted(_ id: UUID, deleted: Bool) async -> Bool {
        do {
            guard let memory = state.agentMemories.first(where: { $0.id == id }),
                  let sharedLibrary
            else { throw SharedLibraryError.notFound }
            try await sharedLibrary.setMemoryDeleted(memory, deleted: deleted)
            return true
        } catch {
            Self.logger.error(
                "Shared memory deletion change failed: \(error.localizedDescription, privacy: .public)"
            )
            return false
        }
    }
}
