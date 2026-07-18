import Foundation

extension PlaybackState {
    var playbackFailure: UserFacingFailure? {
        guard case .failed(let error) = engine.state else { return nil }
        return UserFacingFailurePresenter.make(failure: error.failure, canRetry: true)
    }

    func handleAudioSessionEvent(
        _ event: PlaybackAudioSessionEvent,
        observedAt: Date = Date()
    ) {
        let action = sessionPolicy.handle(
            event,
            episodeID: episode?.id,
            playbackRequested: playbackRequested,
            didReachNaturalEnd: engine.didReachNaturalEnd
        )

        switch action {
        case .none:
            if case .interruptionEnded = event {
                playbackRequested = false
            }
            break
        case .pauseAndPersist:
            if case .routeChanged(reason: .oldDeviceUnavailable, previous: _, current: _) = event {
                playbackRequested = false
            }
            pauseForSystemBoundary()
        case .rebuildAndResume:
            rebuildAndResumeAfterMediaServicesReset()
        case .resume:
            resumeAfterSystemBoundary()
        }

        let observation = makeHostObservation(observedAt: observedAt)
        lastHostObservation = observation
        onHostObservation(observation)
    }

    func makeHostObservation(observedAt: Date = Date()) -> PlaybackObservation {
        PlaybackObservation(
            episodeID: episode?.id,
            hostState: engine.state.hostState,
            positionMilliseconds: Self.milliseconds(engine.currentTime),
            durationMilliseconds: Self.milliseconds(duration),
            route: sessionPolicy.route,
            interruption: sessionPolicy.interruption,
            ended: engine.didReachNaturalEnd,
            observedAt: observedAt
        )
    }

    /// System-originated pause boundary. It deliberately omits haptics and
    /// persists the latest AVPlayer playhead before draining the store cache.
    func pauseForSystemBoundary() {
        if let episode, engine.currentTime > 0 {
            onPersistPosition(episode.id, engine.currentTime)
        }
        if case .failed = engine.state {
            onFlushPositions()
            return
        }
        engine.pause()
        persistenceTask?.cancel()
        persistenceTask = nil
        onFlushPositions()
        writeNowPlayingSnapshot(force: true)
    }

    /// Resumes only after PlaybackSessionPolicy has matched the interruption
    /// to the same active episode and the OS supplied `shouldResume`.
    func resumeAfterSystemBoundary() {
        guard let episode, !engine.didReachNaturalEnd else { return }
        if case .failed = engine.state {
            let resume = max(engine.currentTime, episode.playbackPosition)
            engine.load(episode)
            if resume > 0 { engine.seek(to: resume) }
        }
        playbackRequested = true
        engine.play()
        if case .failed = engine.state {
            playbackRequested = false
            return
        }
        startPersistenceLoop()
        writeNowPlayingSnapshot(force: true)
    }

    private func rebuildAndResumeAfterMediaServicesReset() {
        guard let episode else { return }
        let resume = max(engine.currentTime, episode.playbackPosition)
        pauseForSystemBoundary()
        AudioSessionCoordinator.shared.invalidateAfterMediaServicesReset()
        engine.load(episode)
        if resume > 0 { engine.seek(to: resume) }
        resumeAfterSystemBoundary()
    }

    private static func milliseconds(_ seconds: TimeInterval) -> Int64 {
        guard seconds.isFinite else { return 0 }
        return Int64((max(0, seconds) * 1_000).rounded())
    }
}

private extension AudioEngine.State {
    var hostState: PlaybackHostState {
        switch self {
        case .idle: .idle
        case .loading: .loading
        case .playing: .playing
        case .paused: .paused
        case .buffering: .buffering
        case .failed: .failed
        }
    }
}
