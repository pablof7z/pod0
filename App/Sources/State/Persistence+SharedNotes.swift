import Foundation

extension Persistence {
    /// After the verified notes cutover, Swift metadata becomes a projection
    /// cache only and must never be an alternate durable note writer.
    func activateSharedNoteAuthority() {
        sharedNoteAuthority.withLock { $0 = true }
    }

    func metadataState(from state: AppState) -> AppState {
        var metadata = state
        metadata.episodes = []
        if sharedNoteAuthority.withLock({ $0 }) {
            metadata.notes = []
        }
        return metadata
    }
}
