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
        engine.onPlayingTick = { [weak self, weak audio] url, position, duration in
            audio?.emitReport(.playing(url: url, positionSecs: position, durationSecs: duration))
            guard let self else { return }
            // Persist position directly — the Rust kernel's nowPlaying.episodeId
            // may be nil when the episode was loaded without a prior kernelLoad
            // acknowledgment (e.g. cold-start restore path), in which case the
            // normal Rust→onPositionTick path never fires. Writing here is
            // idempotent: AppStateStore guards against duplicate values.
            if let episodeID = self.episode?.id {
                self.store?.setEpisodePlaybackPosition(episodeID, position: position)
            }
            // Advance bounded-segment queue items (clips, agent segments) that
            // are not in the Rust queue. Rust handles whole-episode auto-advance
            // via maybe_auto_advance; this path covers start/end-bounded items.
            if let end = self.currentSegmentEndTime, position >= end {
                self.currentSegmentEndTime = nil
                let store = self.store
                if !self.queue.isEmpty {
                    _ = self.playNext(resolve: { store?.episode(id: $0) })
                } else {
                    self.engine.pause()
                }
            }
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
        engine.onSleepTimerEpisodeEnd = { [weak self] in
            // Sleep timer stopped at end of episode: position was already flushed
            // via onPauseEvent. This path deliberately skips emitting `itemEnd`
            // so Rust's `maybe_auto_advance` doesn't fire.
            guard let self, let episodeID = self.episode?.id else { return }
            // UNLIKE `onItemEnd`, this path cannot delegate mark-played to the
            // kernel: suppressing `itemEnd` (to avoid auto-advance) also means
            // Rust's `apply_writeback` ItemEnd branch — the only kernel path
            // that honours `auto_mark_played_at_end` — never runs. A bare
            // `kernelMarkPlayed` dispatch (`inbox/mark_listened`) is
            // unconditional and would ignore the user's setting. So the Swift
            // `markEpisodePlayed` stays the marker here, gated on the setting to
            // match the natural-end semantics. It also routes the gated
            // delete-after-played policy. The position rewind for this completed
            // episode is handled in `AudioEngine.handleEndOfItem`, which reports
            // the final paused position as 0 on the ordered audio-report channel.
            if self.store?.state.settings.autoMarkPlayedAtEnd == true {
                self.store?.markEpisodePlayed(episodeID)
            }
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
                    // Rust popped this whole-episode item from its queue during
                    // auto-advance; mirror the removal in the iOS queue so the
                    // Up Next sheet doesn't show a stale entry.
                    if self.queue.first.map({ $0.episodeID == id && $0.startSeconds == nil }) == true {
                        self.queue.removeFirst()
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
                if (stagedEpisodeID?.isEmpty ?? true), let episodeID = self.episode?.id {
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
