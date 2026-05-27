import Foundation
import os.log

// MARK: - ChaptersHydrationService

/// Routes Podcasting 2.0 chapter fetches through the Rust kernel.
///
/// UI surfaces (`PlayerView`, `EpisodeDetailView`) call `hydrateIfNeeded(_:)`
/// from `.task`. The method dispatches `{"op":"fetch_chapters"}` to the kernel
/// which owns the HTTP fetch, JSON parse, and persistence. The next snapshot
/// tick projects the chapters onto `Episode.chapters` via `applyKernelState`.
///
/// Deduplication: a per-session `Set<UUID>` prevents duplicate dispatches for
/// the same episode. Errors are handled by Rust (logged internally) and
/// surface as a no-op to the caller — chapters are nice-to-have.
@MainActor
final class ChaptersHydrationService {

    static let shared = ChaptersHydrationService()

    private static let logger = Logger.app("ChaptersHydration")

    /// Episode IDs whose chapter fetch has been dispatched this session.
    private var dispatched: Set<UUID> = []

    /// Dispatch a chapter fetch for `episode` if it has a `chaptersURL`,
    /// doesn't already have inline chapters, and hasn't been dispatched yet
    /// this session. Idempotent — safe to call on every view appear.
    func hydrateIfNeeded(episode: Episode, store: AppStateStore) {
        guard episode.chaptersURL != nil else { return }
        if let existing = episode.chapters, !existing.isEmpty { return }
        guard !dispatched.contains(episode.id) else { return }
        dispatched.insert(episode.id)
        store.kernelFetchChapters(episodeID: episode.id)
        Self.logger.info("Dispatched fetch_chapters for episode \(episode.id, privacy: .public)")
    }

    /// Test hook: clears the per-session dedup cache so a fresh dispatch can
    /// be observed. Production code never needs this.
    func resetForTesting() {
        dispatched.removeAll()
    }
}
