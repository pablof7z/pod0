import Foundation

extension Persistence {
    /// Activates only after the verified listening import and every required
    /// bootstrap cutover succeeds. Until then the native rows remain migration
    /// evidence and must be preserved.
    func activateSharedListeningAuthority() {
        sharedArtifactAuthority.withLock { $0.listening = true }
    }

    /// After the verified notes cutover, Swift metadata becomes a projection
    /// cache only and must never be an alternate durable note writer.
    func activateSharedNoteAuthority() {
        sharedArtifactAuthority.withLock { $0.notes = true }
    }

    func metadataState(from state: AppState) -> AppState {
        var metadata = state
        metadata.episodes = []
        if sharedArtifactAuthority.withLock({ $0.listening }) {
            metadata.podcasts = []
            metadata.subscriptions = []
            metadata.lastPlayedEpisodeID = nil
        }
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

    func episodesForNativePersistence(from state: AppState) -> [Episode] {
        sharedArtifactAuthority.withLock { $0.listening } ? [] : state.episodes
    }
}
