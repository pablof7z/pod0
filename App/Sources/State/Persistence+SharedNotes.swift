import Foundation

extension Persistence {
    /// After the verified notes cutover, Swift metadata becomes a projection
    /// cache only and must never be an alternate durable note writer.
    func activateSharedNoteAuthority() {
        sharedArtifactAuthority.withLock { $0.notes = true }
    }

    func metadataState(from state: AppState) -> AppState {
        var metadata = state
        metadata.episodes = []
        if sharedArtifactAuthority.withLock({ $0.notes }) {
            metadata.notes = []
        }
        if sharedArtifactAuthority.withLock({ $0.clips }) {
            metadata.clips = []
        }
        if sharedArtifactAuthority.withLock({ $0.scheduledAgents }) {
            metadata.agentScheduledTasks = []
        }
        if sharedArtifactAuthority.withLock({ $0.memories }) {
            metadata.agentMemories = []
            metadata.compiledMemory = nil
        }
        return metadata
    }
}
