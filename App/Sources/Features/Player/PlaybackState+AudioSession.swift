import Foundation
import Pod0Core

extension PlaybackState {
    var playbackFailure: UserFacingFailure? {
        guard case .failed(let error) = engine.state else { return nil }
        return UserFacingFailurePresenter.make(failure: error.failure, canRetry: true)
    }

    func handleAudioSessionEvent(
        _ event: PlaybackAudioSessionEvent,
        observedAt _: Date = Date()
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

        let observation = makeHostObservation()
        lastHostObservation = observation
        onHostObservation(observation)
    }

    func makeHostObservation() -> PlaybackLifecycleObservation {
        PlaybackLifecycleObservation(
            episodeId: episode.map { EpisodeId(uuid: $0.id) },
            state: engine.state.hostState,
            positionMilliseconds: Self.milliseconds(engine.currentTime),
            durationMilliseconds: Self.milliseconds(duration),
            route: sessionPolicy.route.coreRoute,
            interruption: sessionPolicy.interruption.coreInterruption,
            ended: engine.didReachNaturalEnd
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

    private static func milliseconds(_ seconds: TimeInterval) -> UInt64 {
        guard seconds.isFinite else { return 0 }
        return UInt64((max(0, seconds) * 1_000).rounded())
    }
}

private extension AudioEngine.State {
    var hostState: Pod0Core.PlaybackHostState {
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

private extension NativePlaybackAudioRoute {
    var coreRoute: Pod0Core.PlaybackAudioRoute {
        switch self {
        case .builtIn: .builtIn
        case .wired: .wired
        case .bluetooth: .bluetooth
        case .airPlay: .airPlay
        case .car: .car
        case .external: .external
        case .unknown: .unknown
        }
    }
}

private extension NativePlaybackInterruption {
    var coreInterruption: Pod0Core.PlaybackInterruption {
        switch self {
        case .none: .none
        case .began: .began
        case .endedShouldResume: .endedShouldResume
        case .endedShouldRemainPaused: .endedShouldRemainPaused
        }
    }
}
