import Foundation

// MARK: - AppStateStore + EpisodeProjections
//
// Historical compatibility shim. Episode-derived product facts used to be
// precomputed in Swift caches here. Those facts are now Rust-owned and exposed
// through narrow projections in `AppStateStore+RustLibraryProjection.swift`.
// Mutation paths still call `invalidateEpisodeProjections()`; keeping it as a
// no-op avoids broad churn while making the ownership boundary explicit.

extension AppStateStore {

    func recomputeEpisodeProjections() {}

    func invalidateEpisodeProjections() {}

    /// Cheap inequality check used by `state.didSet` to decide whether episode
    /// membership changed. The cache rebuild is gone, but this helper remains
    /// for state-change paths that still use the fingerprint as a guard.
    static func episodesFingerprintChanged(_ lhs: [Episode], _ rhs: [Episode]) -> Bool {
        if lhs.count != rhs.count { return true }
        guard !lhs.isEmpty else { return false }
        if lhs.first?.id != rhs.first?.id { return true }
        if lhs.last?.id != rhs.last?.id { return true }
        return false
    }
}
