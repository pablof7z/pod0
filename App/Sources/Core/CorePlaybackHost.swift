import AVFoundation
import Foundation
import Pod0Core

@MainActor
protocol CorePlaybackHosting: AnyObject {
    func execute(_ request: HostRequest) -> HostObservation
    func installObservationSink(_ sink: @escaping (PlaybackLifecycleObservation) -> Void)
}

/// Executes AVFoundation primitives requested by the shared core. It does not
/// choose queue order, resume policy, completion, or timer semantics.
@MainActor
final class CorePlaybackHost: CorePlaybackHosting {
    typealias EpisodeResolver = (UUID) -> Episode?

    private let engine: AudioEngine
    private let resolveEpisode: EpisodeResolver
    private var observationSink: (PlaybackLifecycleObservation) -> Void = { _ in }
    private var route: Pod0Core.PlaybackAudioRoute = .unknown
    private var interruption: Pod0Core.PlaybackInterruption = .none
    private var hasIssuedPlay = false
    private var transitionTask: Task<Void, Never>?

    init(engine: AudioEngine, resolveEpisode: @escaping EpisodeResolver) {
        self.engine = engine
        self.resolveEpisode = resolveEpisode
        route = Self.currentRoute()
        engine.onHostStateChanged = { [weak self] in self?.emitObservation() }
        engine.onHostAudioSessionEvent = { [weak self] event in
            self?.record(event)
        }
    }

    func installObservationSink(
        _ sink: @escaping (PlaybackLifecycleObservation) -> Void
    ) {
        observationSink = sink
    }

    func execute(_ request: HostRequest) -> HostObservation {
        switch request {
        case .loadMedia(let episodeID, let audioURL, let startPositionMilliseconds):
            return load(
                episodeID: episodeID,
                audioURL: audioURL,
                startPositionMilliseconds: startPositionMilliseconds
            )
        case .play(let episodeID, let transitionCue):
            guard matchesLoadedEpisode(episodeID) else { return unavailable() }
            guard play(transitionCue: transitionCue) else {
                return .failed(code: .invalidResponse, safeDetail: "Unsupported transition cue")
            }
        case .pause(let episodeID), .stopPlayback(let episodeID):
            guard matchesLoadedEpisode(episodeID) else { return unavailable() }
            cancelTransition()
            engine.pause()
        case .seek(let episodeID, let positionMilliseconds, _, _):
            guard matchesLoadedEpisode(episodeID) else { return unavailable() }
            engine.seek(to: Self.seconds(positionMilliseconds))
        case .setRate(let episodeID, let rate):
            guard matchesLoadedEpisode(episodeID) else { return unavailable() }
            engine.setRate(Double(rate.value) / 1_000)
        case .armNativeTimer(let episodeID, let mode):
            guard matchesLoadedEpisode(episodeID) else { return unavailable() }
            guard let mode = Self.timerMode(mode) else {
                return .failed(code: .invalidResponse, safeDetail: "Unsupported timer mode")
            }
            engine.setSleepTimer(mode)
        case .cancelNativeTimer(let episodeID):
            guard matchesLoadedEpisode(episodeID) else { return unavailable() }
            engine.setSleepTimer(.off)
        case .observePlayback:
            break
        case .fetchFeed, .fetchPublisherChapters:
            return .failed(code: .invalidResponse, safeDetail: "HTTP request sent to player")
        case .executeChapterModel, .recoverChapterModelOperation,
             .executeTranscriptCapability, .executeScheduledAgentTurn,
             .executeAgentModelTurn, .presentAgentApproval,
             .executeAgentCapability, .provisionNostrSignerCredential,
             .restoreNostrSignerCredential, .signNostrEvent,
             .deleteNostrSignerCredential, .scheduleCoreWake:
            return .failed(code: .invalidResponse, safeDetail: "Core request sent to player")
        case .startEpisodeDownload, .cancelEpisodeDownload,
             .removeEpisodeDownloadArtifact:
            return .failed(code: .invalidResponse, safeDetail: "Download request sent to player")
        case .embedRecallQuery, .embedRecallSpans, .rerankRecallCandidates,
             .removeLegacyRecallIndexArtifacts:
            return .failed(code: .invalidResponse, safeDetail: "Recall request sent to player")
        case .unsupported(let wireCode):
            return .unsupported(wireCode: wireCode)
        }
        return .playbackObserved(value: currentObservation())
    }

    private func load(
        episodeID: EpisodeId,
        audioURL: String,
        startPositionMilliseconds: UInt64
    ) -> HostObservation {
        guard let id = episodeID.uuid,
              let episode = resolveEpisode(id),
              let requestedURL = URL(string: audioURL),
              Self.isAllowedMediaURL(requestedURL)
        else { return unavailable() }

        cancelTransition()
        hasIssuedPlay = false
        engine.load(
            episode,
            requestedURL: requestedURL,
            initialPosition: Self.seconds(startPositionMilliseconds)
        )
        return .playbackObserved(value: currentObservation())
    }

    private func play(transitionCue: PlaybackTransitionCue) -> Bool {
        cancelTransition()
        hasIssuedPlay = true
        switch transitionCue {
        case .immediate:
            engine.play()
        case .fadeIn(let durationMilliseconds):
            engine.fadeBaseVolume = 0
            engine.applyEffectiveVolume()
            engine.play()
            startFadeIn(durationMilliseconds: durationMilliseconds)
        case .unsupported:
            hasIssuedPlay = false
            return false
        }
        return true
    }

    private func startFadeIn(durationMilliseconds: UInt32) {
        let duration = max(1, min(durationMilliseconds, 5_000))
        transitionTask = Task { @MainActor [weak self] in
            guard let self else { return }
            let steps: UInt32 = 20
            let stepDuration = duration / steps
            for step in 1...steps {
                guard !Task.isCancelled else { return }
                try? await Task.sleep(for: .milliseconds(stepDuration))
                guard !Task.isCancelled else { return }
                self.engine.fadeBaseVolume = Float(step) / Float(steps)
                self.engine.applyEffectiveVolume()
            }
        }
    }

    private func cancelTransition() {
        transitionTask?.cancel()
        transitionTask = nil
        engine.fadeBaseVolume = 1
        engine.applyEffectiveVolume()
    }

    private func matchesLoadedEpisode(_ episodeID: EpisodeId) -> Bool {
        episodeID.uuid == engine.episode?.id
    }

    private func unavailable() -> HostObservation {
        .failed(code: .mediaUnavailable, safeDetail: "Playback media is unavailable")
    }

    private func emitObservation() {
        let observation = currentObservation()
        interruption = .none
        observationSink(observation)
    }

    private func currentObservation() -> PlaybackLifecycleObservation {
        PlaybackLifecycleObservation(
            episodeId: engine.episode.map { EpisodeId(uuid: $0.id) },
            state: hostState,
            positionMilliseconds: Self.milliseconds(engine.currentTime),
            durationMilliseconds: Self.milliseconds(engine.duration),
            route: route,
            interruption: interruption,
            ended: engine.didReachNaturalEnd
        )
    }

    private var hostState: Pod0Core.PlaybackHostState {
        switch engine.state {
        case .idle: .idle
        case .loading: .loading
        case .playing: .playing
        case .paused: hasIssuedPlay ? .paused : .prepared
        case .buffering: .buffering
        case .failed: .failed
        }
    }

    private func record(_ event: PlaybackAudioSessionEvent) {
        switch event {
        case .interruptionBegan(let route):
            self.route = route.coreRoute
            interruption = .began
        case .interruptionEnded(let shouldResume, let route):
            self.route = route.coreRoute
            interruption = shouldResume ? .endedShouldResume : .endedShouldRemainPaused
        case .routeChanged(let reason, _, let current):
            route = current.coreRoute
            interruption = reason == .oldDeviceUnavailable ? .routeLost : .none
        case .mediaServicesWereReset(let route):
            self.route = route.coreRoute
            AudioSessionCoordinator.shared.invalidateAfterMediaServicesReset()
            interruption = .mediaServicesReset
        }
        emitObservation()
    }

    private static func timerMode(_ mode: NativeTimerMode) -> SleepTimer.Mode? {
        switch mode {
        case .duration(let durationMilliseconds):
            .duration(seconds(durationMilliseconds))
        case .endOfEpisode:
            .endOfEpisode
        case .unsupported:
            nil
        }
    }

    private static func currentRoute() -> Pod0Core.PlaybackAudioRoute {
        PlaybackAudioSessionObserver.route(
            for: AVAudioSession.sharedInstance().currentRoute.outputs.map(\.portType)
        ).coreRoute
    }

    private static func isAllowedMediaURL(_ url: URL) -> Bool {
        guard let scheme = url.scheme?.lowercased() else { return false }
        return scheme == "https" || scheme == "http" || scheme == "file"
    }

    private static func seconds(_ milliseconds: UInt64) -> TimeInterval {
        Double(milliseconds) / 1_000
    }

    private static func milliseconds(_ seconds: TimeInterval) -> UInt64 {
        guard seconds.isFinite, seconds > 0 else { return 0 }
        return UInt64(min(seconds * 1_000, Double(UInt64.max)).rounded())
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
