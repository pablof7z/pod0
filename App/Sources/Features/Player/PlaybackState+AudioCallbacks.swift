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

        // ── Kernel bridge: AudioEngine → AudioCapability → Rust ──────────
        let audio = PodcastCapabilities.shared.audio
        engine.onPlayingTick = { [weak audio] url, position, duration in
            // The Rust kernel's apply_writeback (audio_report.rs) is the sole
            // owner of ep.position_secs persistence. Swift is render-only —
            // the emitReport below drives the kernel path; the resulting
            // nowPlaying tick propagates position to the UI via
            // KernelModel.onPositionTick → AppStateStore.onPositionTick.
            audio?.emitReport(.playing(url: url, positionSecs: position, durationSecs: duration))
        }
        engine.onPauseEvent = { [weak audio] url, position in
            audio?.emitReport(.paused(url: url, positionSecs: position))
        }
        engine.onItemEnd = { [weak audio] url in
            audio?.emitReport(.itemEnd(url: url))
            // Both mark-played-at-end AND delete-after-played are KERNEL policy
            // on this path. The `itemEnd` report above drives Rust's
            // `apply_writeback` ItemEnd branch, which (gated on
            // `auto_mark_played_at_end`) flips `played`, rewinds the stored
            // position to 0, and — when `auto_delete_downloads_after_played` is
            // on — removes the local download itself. The resulting frame
            // round-trips `played`/`position`/download state back through the
            // projection. So no Swift reaction is needed here; doing it would
            // duplicate kernel-owned decisions (D0).
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
                // Cold-restart restore case: RootView re-seeds the last-played
                // episode into the engine (paused), but Rust's `PlayerActor`
                // was never sent a `Load` for it — so its `nowPlaying` is still
                // empty. If we just `engine.play()` here, audio starts but Rust
                // attributes the resulting `Playing` reports to no episode:
                // position never persists and the episode never marks played.
                //
                // So before starting audio, if Rust has no staged episode
                // (`nowPlaying.episodeId` nil/empty) but we have a restored one,
                // stage it in Rust via `kernelLoad` first. Rust replies with a
                // `Load` echo that lands on the `.load` case below, which only
                // calls `setEpisode(playAfterLoad: false)` — it never re-issues
                // `play()` or `kernelLoad`, so this cannot loop. (Rust-originated
                // auto-advance plays already carry a populated `nowPlaying`, so
                // the guard is a no-op there.)
                let stagedEpisodeID = self.store?.kernel?.podcastSnapshot?.nowPlaying?.episodeId
                if let episodeID = Self.restoredEpisodeIDToStageBeforeRemotePlay(
                    kernelNowPlayingEpisodeID: stagedEpisodeID,
                    restoredEpisodeID: self.episode?.id
                ) {
                    self.store?.kernelLoad(episodeID: episodeID)
                }
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
                _ = secs
            case .setVolume:
                break // AudioEngine has no volume API
            }
        }
    }

    func setRate(_ newRate: Double) {
        transport?.kernelSetSpeed(newRate)
        engine.setRate(newRate)
    }

    static func restoredEpisodeIDToStageBeforeRemotePlay(
        kernelNowPlayingEpisodeID: String?,
        restoredEpisodeID: UUID?
    ) -> UUID? {
        let stagedEpisodeID = kernelNowPlayingEpisodeID?.trimmingCharacters(in: .whitespacesAndNewlines)
        guard stagedEpisodeID?.isEmpty ?? true else { return nil }
        return restoredEpisodeID
    }
}
