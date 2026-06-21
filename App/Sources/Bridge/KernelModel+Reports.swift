import Foundation

// MARK: - KernelModel download and audio report handlers
//
// These two methods handle the narrow-snapshot push channels (download-queue
// and audio-report). Extracted from KernelModel.swift to keep that file under
// the AGENTS.md 500-line hard limit.

extension KernelModel {

    /// Apply a download-report response (from `attachDownloadReportChannel`).
    ///
    /// Progress ticks (~1 Hz per active download) land here and update only the
    /// always-fresh `downloadSnapshot` — the source `AppStateStore`'s row
    /// overlay reads. They do NOT bump the global `rev` in Rust, so they never
    /// pull or JSON-decode the full library snapshot (the empirical CPU/heat
    /// hot path). Only a durable change (completion/cancellation, which flips
    /// `Episode.downloadState`) sets `durableChanged`; then we pull the full
    /// snapshot so the library projection reprojects the affected episode.
    @MainActor
    func applyDownloadReport(downloads: DownloadQueueSnapshot?, durableChanged: Bool) {
        if downloads != downloadSnapshot {
            downloadSnapshot = downloads
        }
        if durableChanged {
            pullPodcastSnapshotIfChanged()
        }
    }

    /// Apply one audio report's inline player state. The hot path: `Playing`
    /// (≤4 Hz playhead) and `BufferingProgress` ticks arrive here with
    /// `durableChanged == false`, so they refresh ONLY the live surfaces
    /// (`nowPlaying` scrubber + Dynamic Island + lock-screen elapsed) using the
    /// already-decoded `library` — never re-decoding the 3k-episode snapshot. A
    /// structural report (play/pause/stop, track end, sleep-timer) additionally
    /// pulls the full snapshot so list-view state stays correct.
    ///
    /// Mirrors the `nowPlaying`/reconcile block of `applyPodcastUpdate` (the
    /// path durable reports still take), minus the library decode + hashing.
    func applyAudioReport(nowPlaying newNowPlaying: PlayerState?, durableChanged: Bool) {
        let previous = nowPlaying
        nowPlaying = newNowPlaying
        // Forward the live position to AppStateStore for render-only surfaces
        // (scrubber, in-progress carousel). The kernel persists position itself
        // (audio_report.rs::apply_writeback); this forward never writes to disk.
        // Covers Playing, BufferingProgress (which advances positionSecs with
        // isPlaying=false), and the final Paused frame (capturing the last
        // playhead before a force-quit). Guard only on positionSecs > 0 and
        // episodeId being present; skips stopped/reset states automatically.
        if let np = newNowPlaying, np.positionSecs > 0, let id = np.episodeId,
           !np.didReachNaturalEnd {
            onPositionTick?(id, np.positionSecs)
        }
        // Live media surfaces, off the library-decode path. `reconcileLiveActivity`
        // coalesces same-episode position updates; `reconcileNowPlayingMetadata`
        // is a no-op unless the episode changed — both cheap, and `library` is
        // the current cached value (unchanged by a position tick).
        reconcileLiveActivity(previous: previous, next: newNowPlaying, library: library)
        reconcileNowPlayingMetadata(previous: previous, next: newNowPlaying, library: library)
        // Always probe — but `pullPodcastSnapshotIfChanged` is rev-gated, and
        // since `Playing`/buffering ticks no longer bump the global `rev`, a tick
        // with no other activity costs only one atomic read (no decode, no
        // rebuild). This intentionally preserves the reactive side-channel the
        // per-tick pull used to provide: background actor-thread work that bumps
        // `rev` off the kernel emit path (inbox triage, categorization, and any
        // tokio-spawned projection update) still reaches the UI during a long
        // listen. A real change — a durable audio event OR a background bump —
        // advances `rev` and triggers exactly one full rebuild; `durableChanged`
        // is informational (the rev gate, not the flag, decides the pull).
        pullPodcastSnapshotIfChanged(synchronous: false)
    }
}
