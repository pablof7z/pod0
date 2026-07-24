import Foundation

extension Persistence {
    /// Fences a startup retirement snapshot before later native mutations can
    /// reserve a newer revision. Production persistence only enqueues here;
    /// immediate test persistence commits synchronously.
    @discardableResult
    func commitStartupRetirement(_ state: AppState) -> UInt64 {
        save(state)
    }

    func reset() {
        writeLock.withLock {
            try? FileManager.default.removeItem(at: fileURL)
            episodeStore.reset()
            removeSharedCoreArtifacts()
            episodeSnapshot.withLock { $0 = nil }
            revision.withLock { $0 = 0 }
            lastWrittenRevision.withLock { $0 = 0 }
            sharedArtifactAuthority.withLock { $0 = .init() }
            resetEpisodeWriteSummary()
        }
    }
}
