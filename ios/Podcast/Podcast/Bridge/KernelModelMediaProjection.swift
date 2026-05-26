// KernelModelMediaProjection.swift
// Live Activity + MPNowPlayingInfoCenter reconciliation helpers.
// Extracted from KernelModel.swift to keep that file under the 500-line limit.

import Foundation

// MARK: - KernelModel + media-projection reconciliation

extension KernelModel {

    /// Translate `PlayerState` transitions into Live Activity lifecycle
    /// calls. Called from `pullPodcastSnapshotIfChanged` — every kernel
    /// snapshot advance is the single funnel that mirrors player state out
    /// to ActivityKit (D7 — kernel is the source of truth, executor only
    /// translates).
    ///
    /// Transitions:
    ///   - nil → non-nil: `start(...)` with metadata from the embedded library.
    ///   - non-nil → non-nil (same episode): `update(positionSecs:isPlaying:)`.
    ///   - non-nil → non-nil (different episode): `start(...)` to swap.
    ///   - non-nil → nil: `stop()`.
    func reconcileLiveActivity(
        previous: PlayerState?, next: PlayerState?, library: [PodcastSummary]
    ) {
        switch (previous, next) {
        case (nil, nil):
            return
        case (_, nil):
            LiveActivityManager.shared.stop()
        case let (nil, .some(state)):
            startLiveActivity(for: state, library: library)
        case let (.some(prev), .some(state)):
            if prev.episodeId != state.episodeId {
                startLiveActivity(for: state, library: library)
            } else {
                LiveActivityManager.shared.update(
                    positionSecs: state.positionSecs, isPlaying: state.isPlaying)
            }
        }
    }

    /// Resolve episode/podcast metadata from the library snapshot and
    /// hand the manager a fully-populated start payload.
    func startLiveActivity(for state: PlayerState, library: [PodcastSummary]) {
        guard let episodeId = state.episodeId else { return }
        var episodeTitle = ""
        var podcastTitle = ""
        var artworkURL: URL?

        outer: for show in library {
            for episode in show.episodes where episode.id == episodeId {
                episodeTitle = episode.title
                podcastTitle = episode.podcastTitle ?? show.title
                let artworkString = episode.artworkUrl ?? show.artworkUrl
                if let artworkString { artworkURL = URL(string: artworkString) }
                break outer
            }
        }
        if episodeTitle.isEmpty { episodeTitle = "Now Playing" }

        LiveActivityManager.shared.start(
            episodeID: episodeId,
            episodeTitle: episodeTitle,
            podcastTitle: podcastTitle,
            positionSecs: state.positionSecs,
            durationSecs: state.durationSecs ?? 0,
            isPlaying: state.isPlaying,
            artworkURL: artworkURL)
    }

    /// Mirror episode/podcast metadata into `MPNowPlayingInfoCenter` when the
    /// playing episode changes. Fires only on episode transitions so the
    /// lock-screen title reflects the kernel snapshot rather than the URL stem.
    func reconcileNowPlayingMetadata(
        previous: PlayerState?, next: PlayerState?, library: [PodcastSummary]
    ) {
        guard let next, let episodeId = next.episodeId else { return }
        let previousId = previous?.episodeId
        guard previousId != episodeId else { return }

        var episodeTitle = "Now Playing"
        var podcastTitle = ""
        var artworkURL: URL?
        outer: for show in library {
            for ep in show.episodes where ep.id == episodeId {
                episodeTitle = ep.title
                podcastTitle = ep.podcastTitle ?? show.title
                let urlStr = ep.artworkUrl ?? show.artworkUrl
                if let urlStr { artworkURL = URL(string: urlStr) }
                break outer
            }
        }
        PodcastCapabilities.shared.audio.updateNowPlayingMetadata(
            episodeTitle: episodeTitle,
            podcastTitle: podcastTitle,
            artworkURL: artworkURL
        )
    }
}
