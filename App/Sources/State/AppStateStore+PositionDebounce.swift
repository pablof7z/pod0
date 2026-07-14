import Foundation
import UIKit

// MARK: - AppStateStore + PositionDebounce
//
// **Why this exists.** The audio engine ticks once per second while playback
// runs and forwards the playhead through `setEpisodePlaybackPosition`. Before
// this file landed, every one of those ticks mutated `state.episodes`, which
// fires `state.didSet`, which atomically rewrites the entire ~8 MB JSON blob
// at `<App Group>/Library/Application Support/podcastr-state.v1.json`. That's
// 480 MB of disk I/O per minute of playback — battery, NAND wear, and a main-
// actor stall every second.
//
// **The fix (Option A — debounce position writes only).** Position updates
// flow through a side cache (`positionCache`) instead of straight into
// `state.episodes`. The cache is folded back into reads (`episode(id:)`,
// `inProgressEpisodes`, `recentEpisodes`) so UI surfaces always see the
// latest playhead. Disk writes happen only when we need them:
//
//   - **Eager-first:** the very first position update after a flush is
//     written immediately. Rationale: a crash 0.5 sec after playback starts
//     mustn't lose all progress. It also keeps the existing single-call
//     `setEpisodePlaybackPosition` semantics — the position lands in
//     `state.episodes` straight away when the loop hasn't started yet.
//   - **Trailing debounce:** once the eager save fires, subsequent rapid
//     updates queue in the cache. A `Task` schedules a flush 5 sec after
//     the last update — covering the "user paused mid-episode" case where
//     no other natural flush event will fire.
//   - **Max-interval cap:** if updates keep streaming faster than the
//     debounce can settle (continuous playback), the eager-first gate
//     re-opens after `maxInterval` (30 sec). So the worst case is one
//     write per 30 sec during continuous playback — meeting the "≤ 30 sec
//     of position lost on crash" constraint without hammering the file.
//
// **Hard flush events.** Some transitions need the cache on disk *now*:
//
//   - `markEpisodePlayed(_:)` — the played-true mutation resets the
//     position to 0; the cache must drain *before* that or we'd silently
//     overwrite the user's actual end-position.
//   - `UIApplication.didEnterBackgroundNotification` — force-quit window.
//   - `clearAllData()` — the cache holds positions for episodes that are
//     about to be wiped; flush would attempt to mutate gone records.
//
// All other state mutations (subscribe, settings change, etc.) are rare and
// the user expects them durable, so they stay on the existing
// `state.didSet → save` path.

extension AppStateStore {

    // MARK: - Tunables

    /// Trailing debounce window. After the last `setEpisodePlaybackPosition`
    /// call, wait this long before writing the cache to disk. Long enough
    /// that a tight 1 Hz loop never fires it; short enough that pausing
    /// mid-episode lands the position on disk before the user puts the phone
    /// down.
    static var positionDebounceInterval: TimeInterval { 5 }

    /// Maximum time the cache may hold an unwritten position during
    /// continuous playback. Once this elapses, the next
    /// `setEpisodePlaybackPosition` re-opens the eager-first gate and writes
    /// straight to disk. 30 sec matches the task's "≤ 30 sec lost on crash"
    /// guarantee.
    static var positionMaxInterval: TimeInterval { 30 }

    // MARK: - Public entry points

    /// Records a playback-position update. Cheap: writes to an in-memory
    /// cache and either fires an eager save (first update / max-interval
    /// elapsed) or schedules a trailing-debounce flush.
    ///
    /// Idempotent on no-op: if the cached value already equals `position`
    /// we skip the bookkeeping entirely so the engine's coalesced ticks
    /// don't double-touch the cache.
    func setEpisodePlaybackPosition(_ id: UUID, position: TimeInterval) {
        guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else {
            // Episode is gone (rare: removed mid-tick). Drop the cached entry
            // so it can't resurrect the record on the next flush.
            positionCache.removeValue(forKey: id)
            return
        }

        // Effective current position is whichever is more recent: cache
        // wins if it's been touched since the last flush, else fall back
        // to the persisted record.
        let liveCurrent = positionCache[id] ?? state.episodes[idx].playbackPosition
        guard liveCurrent != position else { return }

        positionCache[id] = position

        let now = Date()
        let dueForEager: Bool
        if let last = lastPositionFlush {
            dueForEager = now.timeIntervalSince(last) >= Self.positionMaxInterval
        } else {
            // No save has happened yet for any episode — write immediately
            // so a crash 200 ms into playback still preserves *some*
            // progress.
            dueForEager = true
        }

        if dueForEager {
            flushPendingPositions()
        } else {
            schedulePositionDebounce()
        }
    }

    /// Drains the position cache into `state.episodes` and lets the existing
    /// `state.didSet` save the file. Safe to call from any path that needs
    /// the cache on disk synchronously (background notification, mark-played,
    /// clearAllData, episode-end). Idempotent on empty cache.
    func flushPendingPositions() {
        positionFlushTask?.cancel()
        positionFlushTask = nil

        guard !positionCache.isEmpty else {
            // Even with nothing pending, refresh `lastPositionFlush` so the
            // eager-first gate stays on its 30s cadence after an explicit
            // flush event (background, mark-played).
            lastPositionFlush = Date()
            return
        }

        // Build the mutation in one pass against a working copy so we hit
        // `state.didSet` exactly once — N cached entries become a single
        // 8 MB save, not N saves.
        var working = state.episodes
        var mutated = false
        for (id, position) in positionCache {
            guard let idx = working.firstIndex(where: { $0.id == id }) else { continue }
            if working[idx].playbackPosition != position {
                working[idx].playbackPosition = position
                mutated = true
            }
        }
        positionCache.removeAll(keepingCapacity: true)
        lastPositionFlush = Date()

        if mutated {
            performMutationBatch {
                state.episodes = working
                // Newly-non-zero playback positions need to land in
                // `inProgressEpisodesCached`; count-only fingerprinting misses this.
                invalidateEpisodeProjections()
            }
        }
    }

    // MARK: - Cache-fold reads
    //
    // Folded into the existing `episode(id:)` / `inProgressEpisodes` /
    // `recentEpisodes` getters in `AppStateStore+Episodes.swift` via these
    // helpers. Kept here next to the cache so the read/write contract lives
    // in one place — anyone changing the cache shape will see the readers
    // immediately below it.

    /// Returns the cached position for `id` if one is pending, else `nil`.
    /// Callers fall back to the value from `state.episodes`.
    func cachedPosition(for id: UUID) -> TimeInterval? {
        positionCache[id]
    }

    /// Folds the position cache into a list of episodes. Used by the
    /// in-progress / recent feeds so a freshly-started episode shows up
    /// without waiting for the first 30s flush.
    func applyingPositionCache(_ episodes: [Episode]) -> [Episode] {
        guard !positionCache.isEmpty else { return episodes }
        return episodes.map { episode in
            guard let cached = positionCache[episode.id] else { return episode }
            var copy = episode
            copy.playbackPosition = cached
            return copy
        }
    }

    // MARK: - Lifecycle hooks (called from AppStateStore.init)

    /// Subscribes the store to `UIApplication.didEnterBackgroundNotification`
    /// so the cache is flushed when the user backgrounds the app — covering
    /// the force-quit window where neither pause nor episode-end fires.
    /// Returns the observer token; callers retain it on the store so removal
    /// happens at deinit.
    func registerBackgroundFlushObserver() -> NSObjectProtocol {
        NotificationCenter.default.addObserver(
            forName: UIApplication.didEnterBackgroundNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            // Mirror the iCloud observer pattern already in `init`: the
            // Notification queue delivers on .main so the closure is
            // already on the main thread; `assumeIsolated` lets us cross
            // back into the actor-isolated method without an `await`.
            MainActor.assumeIsolated {
                self?.flushPendingPositions()
            }
        }
    }

    // MARK: - Private — debounce machinery

    /// Schedules a flush task to fire `positionDebounceInterval` seconds
    /// from now. Cancels any prior pending task so successive calls within
    /// the window keep extending the deadline (true trailing debounce).
    private func schedulePositionDebounce() {
        positionFlushTask?.cancel()
        positionFlushTask = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .seconds(Self.positionDebounceInterval))
            guard let self, !Task.isCancelled else { return }
            self.flushPendingPositions()
        }
    }
}
