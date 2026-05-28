import Foundation

// MARK: - Audio + Kernel bridge callbacks

extension PlaybackState {

    /// Wire the `AudioEngine` event callbacks that drive the UI and forward
    /// real playback events to the Rust kernel via `AudioCapability.emitReport`.
    ///
    /// Also installs the `AudioCapability.commandHandler` bridge so Rust-
    /// originated `AudioCommand`s (auto-advance, remote-control) reach
    /// `AudioEngine` instead of AudioCapability's idle own `AVPlayer`.
    func configureAudioEngineCallbacks() {
        // ── NowPlaying / lock-screen remote commands ─────────────────────
        var callbacks = NowPlayingCenter.Callbacks()
        callbacks.play = { [weak self] in self?.play() }
        callbacks.pause = { [weak self] in self?.pause() }
        callbacks.toggle = { [weak self] in self?.togglePlayPause() }
        callbacks.skipForward = { [weak self] in self?.skipForward() }
        callbacks.skipBackward = { [weak self] in self?.skipBackward() }
        callbacks.seek = { [weak self] time in self?.seek(to: time) }
        callbacks.changeRate = { [weak self] rate in self?.setRate(rate) }
        callbacks.nextTrack = { [weak self] in
            guard let self else { return }
            self.performHeadphoneGesture(self.headphoneDoubleTapAction)
        }
        callbacks.previousTrack = { [weak self] in
            guard let self else { return }
            self.performHeadphoneGesture(self.headphoneTripleTapAction)
        }
        engine.setNowPlayingCallbacks(callbacks)
        engine.onSleepTimerFire = { [weak self] in self?.pause() }

        // ── Kernel bridge: AudioEngine → AudioCapability → Rust ──────────
        let audio = PodcastCapabilities.shared.audio
        engine.onPlayingTick = { [weak audio] url, position, duration in
            audio?.emitReport(.playing(url: url, positionSecs: position, durationSecs: duration))
        }
        engine.onPauseEvent = { [weak audio] url, position in
            audio?.emitReport(.paused(url: url, positionSecs: position))
        }
        engine.onItemEnd = { [weak audio] url in
            audio?.emitReport(.itemEnd(url: url))
        }

        // ── Kernel bridge: Rust AudioCommand → AudioEngine ───────────────
        // Commands from Rust (auto-advance, Siri, CarPlay) route here so
        // AudioEngine — the real player — acts on them, not AudioCapability's
        // idle own AVPlayer.
        audio.commandHandler = { [weak self] command in
            guard let self else { return }
            switch command {
            case let .load(urlString, positionSecs, episodeID):
                if let idStr = episodeID,
                   let id = UUID(uuidString: idStr),
                   var episode = self.store?.episode(id: id) {
                    // The store's enclosureURL is a placeholder for streaming
                    // episodes (Rust projects only the local download path).
                    // Use Rust's resolved URL for non-downloaded episodes.
                    if case .notDownloaded = episode.downloadState,
                       let url = URL(string: urlString) {
                        episode.enclosureURL = url
                    }
                    self.setEpisode(episode, playAfterLoad: false)
                    if positionSecs > 0 { self.engine.seek(to: positionSecs) }
                }
            case .play:
                self.engine.play()
            case .pause:
                self.engine.pause()
            case let .seek(positionSecs):
                self.engine.seek(to: positionSecs)
            case .stop:
                self.engine.pause()
            case let .setSpeed(speed):
                self.engine.setRate(Double(speed))
            case let .setSleepTimer(secs):
                if let secs, secs > 0 {
                    self.engine.setSleepTimer(.duration(TimeInterval(secs)))
                } else {
                    self.engine.setSleepTimer(.off)
                }
            case .setVolume:
                break // AudioEngine has no volume API
            }
        }
    }

    func setRate(_ newRate: Double) {
        engine.setRate(newRate)
    }
}
