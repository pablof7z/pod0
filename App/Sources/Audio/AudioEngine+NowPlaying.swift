import Kingfisher
import MediaPlayer
import UIKit

@MainActor
extension AudioEngine {
    // MARK: - Now Playing wiring

    func configureNowPlayingCallbacks() {
        var callbacks = NowPlayingCenter.Callbacks()
        callbacks.play = { [weak self] in self?.play() }
        callbacks.pause = { [weak self] in self?.pause() }
        callbacks.toggle = { [weak self] in self?.toggle() }
        callbacks.skipForward = { [weak self] in self?.skip(forward: nil) }
        callbacks.skipBackward = { [weak self] in self?.skip(back: nil) }
        callbacks.seek = { [weak self] time in self?.seek(to: time) }
        callbacks.changeRate = { [weak self] rate in self?.setRate(rate) }
        // Engine-default mappings for headphone double/triple-tap. PlaybackState
        // replaces these with the user's configured gesture actions.
        callbacks.nextTrack = { [weak self] in self?.skip(forward: nil) }
        callbacks.previousTrack = { [weak self] in self?.skip(back: nil) }
        nowPlaying.setCallbacks(callbacks)
    }

    func configureSleepTimerHooks() {
        sleepTimer.onFadeTick = { [weak self] multiplier in
            guard let self else { return }
            sleepFadeMultiplier = multiplier
            applyEffectiveVolume()
        }
        sleepTimer.onFire = { [weak self] in
            guard let self else { return }
            onSleepTimerFire()
            sleepFadeMultiplier = 1.0
            applyEffectiveVolume()
        }
    }

    func applyEffectiveVolume() {
        player.volume = fadeBaseVolume * sleepFadeMultiplier
    }

    // MARK: - Internal Now Playing helpers

    func publishNowPlaying() {
        let chapterTitle = episode.flatMap { resolveActiveChapterTitle($0, currentTime) }
        let artworkURL = episode.flatMap { resolveArtworkURL($0, currentTime) }
        if artworkURL != lastPublishedArtworkURL {
            lastPublishedArtworkImage = nil
        }
        nowPlaying.update(
            title: episode?.title,
            artist: episode.flatMap { resolveShowName($0) },
            albumTitle: chapterTitle,
            duration: duration > 0 ? duration : nil,
            elapsed: currentTime,
            rate: state == .playing ? rate : 0,
            artwork: makeMediaItemArtwork()
        )
        lastPublishedChapterTitle = chapterTitle
        fetchArtworkIfNeeded(url: artworkURL)
    }

    /// The request handler runs on MediaPlayer's workloop, so it may only
    /// capture the sendable image value and call the nonisolated resizer.
    private func makeMediaItemArtwork() -> MPMediaItemArtwork? {
        guard let image = lastPublishedArtworkImage else { return nil }
        return MPMediaItemArtwork(boundsSize: image.size) { @Sendable requested in
            if requested == image.size { return image }
            return Self.resize(image, to: requested) ?? image
        }
    }

    private func fetchArtworkIfNeeded(url: URL?) {
        guard url != lastPublishedArtworkURL else { return }
        lastPublishedArtworkURL = url
        guard let url else {
            lastPublishedArtworkImage = nil
            return
        }
        KingfisherManager.shared.retrieveImage(with: url) { [weak self] result in
            Task { @MainActor [weak self] in
                guard let self, lastPublishedArtworkURL == url else { return }
                if case .success(let value) = result {
                    lastPublishedArtworkImage = value.image
                    publishNowPlaying()
                }
            }
        }
    }

    nonisolated private static func resize(_ image: UIImage, to size: CGSize) -> UIImage? {
        let renderer = UIGraphicsImageRenderer(size: size)
        return renderer.image { @Sendable _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
    }

    func publishNowPlayingElapsed() {
        nowPlaying.updateElapsed(currentTime, rate: state == .playing ? rate : 0)
    }
}
