import Foundation
import MediaPlayer
import UIKit

// MARK: - MPNowPlayingInfoCenter integration
//
// The lock screen, Control Center, and CarPlay Now Playing template all
// read from `MPNowPlayingInfoCenter.default()`. The executor updates the
// dictionary every time a report is emitted so the system surfaces
// follow the in-app playhead within a single render tick.
//
// D7 caveats:
//   - Title / artist / artwork resolution lives elsewhere (Rust will
//     populate it via a future `SetMetadata` command per the canonical
//     `nmp.audio.capability` §5.1 spec). Until that lands the title
//     shows the URL basename as a placeholder so the lock screen isn't
//     blank during testing.
//   - The executor never *decides* what to display; it just mirrors
//     the report it just emitted.

@MainActor
extension AudioCapability {

    /// Fold the just-emitted report into the Now Playing dictionary.
    /// Called from `AudioCapability.emit(_:)` for every report.
    func updateNowPlayingForReport(_ report: AudioReport) {
        let center = MPNowPlayingInfoCenter.default()
        switch report {
        case let .playing(url, position, duration):
            var info = center.nowPlayingInfo ?? [:]
            // Preserve episode/podcast title set by updateNowPlayingMetadata;
            // fall back to URL stem only when none has been applied yet.
            if info[MPMediaItemPropertyTitle] == nil {
                info[MPMediaItemPropertyTitle] = placeholderTitle(for: url)
            }
            if duration > 0 {
                info[MPMediaItemPropertyPlaybackDuration] = duration
            }
            info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = position
            info[MPNowPlayingInfoPropertyPlaybackRate] = Double(avPlayer.rate)
            center.nowPlayingInfo = info
        case let .paused(url, position):
            var info = center.nowPlayingInfo ?? [:]
            if info[MPMediaItemPropertyTitle] == nil {
                info[MPMediaItemPropertyTitle] = placeholderTitle(for: url)
            }
            info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = position
            info[MPNowPlayingInfoPropertyPlaybackRate] = 0.0
            center.nowPlayingInfo = info
        case .stopped, .failed:
            center.nowPlayingInfo = nil
        case .bufferingProgress, .sleepTimerFired:
            break
        }
    }

    /// Apply episode/podcast metadata from the kernel snapshot to the
    /// lock screen / Control Center. Call this whenever `nowPlaying`
    /// transitions to a new episode. Metadata persists in the dictionary
    /// until the next episode switch or `stopped`/`failed` clears it.
    func updateNowPlayingMetadata(
        episodeTitle: String,
        podcastTitle: String,
        artworkURL: URL?
    ) {
        let center = MPNowPlayingInfoCenter.default()
        var info = center.nowPlayingInfo ?? [:]
        info[MPMediaItemPropertyTitle] = episodeTitle
        info[MPMediaItemPropertyArtist] = podcastTitle
        center.nowPlayingInfo = info
        if let url = artworkURL {
            Task {
                guard let (data, _) = try? await URLSession.shared.data(from: url),
                      let uiImage = UIImage(data: data) else { return }
                await MainActor.run {
                    var current = MPNowPlayingInfoCenter.default().nowPlayingInfo ?? [:]
                    let artwork = MPMediaItemArtwork(boundsSize: uiImage.size) { _ in uiImage }
                    current[MPMediaItemPropertyArtwork] = artwork
                    MPNowPlayingInfoCenter.default().nowPlayingInfo = current
                }
            }
        }
    }

    /// Lightweight refresh on every periodic tick — keeps elapsed time
    /// in sync without rebuilding the whole dictionary.
    func updateNowPlayingElapsed() {
        let center = MPNowPlayingInfoCenter.default()
        guard var info = center.nowPlayingInfo else { return }
        info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = currentPosition()
        info[MPNowPlayingInfoPropertyPlaybackRate] = Double(avPlayer.rate)
        center.nowPlayingInfo = info
    }

    /// Mirror `Pause` into the lock-screen dictionary so the play
    /// button flips state immediately. Called from `playerPause()`.
    func updateNowPlayingPaused() {
        let center = MPNowPlayingInfoCenter.default()
        guard var info = center.nowPlayingInfo else { return }
        info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = currentPosition()
        info[MPNowPlayingInfoPropertyPlaybackRate] = 0.0
        center.nowPlayingInfo = info
    }

    // MARK: - Helpers

    /// Cheap title derived from the URL's last path component. A real
    /// title arrives via a future `SetMetadata` command (canonical
    /// spec §5.1).
    private func placeholderTitle(for url: String) -> String {
        guard let parsed = URL(string: url) else { return url }
        return parsed.deletingPathExtension().lastPathComponent
    }
}
