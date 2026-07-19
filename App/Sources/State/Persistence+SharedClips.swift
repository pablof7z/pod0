import Foundation

extension Persistence {
    /// Prevents a legacy metadata write from racing a staged shared-core
    /// verification and its single-writer cutover marker.
    func withSharedArtifactMigrationLock<T>(_ body: () throws -> T) rethrows -> T {
        try writeLock.withLock(body)
    }

    /// After verified cutover, AppState clips are an in-memory projection only.
    func activateSharedClipAuthority() {
        sharedArtifactAuthority.withLock { $0.clips = true }
    }
}
