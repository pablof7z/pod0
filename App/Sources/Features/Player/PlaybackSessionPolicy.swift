import Foundation

enum PlaybackSessionPolicyAction: Equatable, Sendable {
    case none
    case pauseAndPersist
    case rebuildAndResume
    case resume
}

/// Deterministic native lifecycle policy. AVFoundation events are inputs;
/// PlaybackState executes the resulting host action and persistence boundary.
struct PlaybackSessionPolicy: Equatable, Sendable {
    private(set) var interruptedEpisodeID: UUID?
    private(set) var interruption: PlaybackInterruption = .none
    private(set) var route: PlaybackAudioRoute = .unknown

    mutating func handle(
        _ event: PlaybackAudioSessionEvent,
        episodeID: UUID?,
        playbackRequested: Bool,
        didReachNaturalEnd: Bool
    ) -> PlaybackSessionPolicyAction {
        switch event {
        case .interruptionBegan(let route):
            self.route = route
            interruption = .began
            interruptedEpisodeID = playbackRequested ? episodeID : nil
            return episodeID == nil ? .none : .pauseAndPersist

        case .interruptionEnded(let shouldResume, let route):
            self.route = route
            interruption = shouldResume ? .endedShouldResume : .endedShouldRemainPaused
            let mayResume = shouldResume
                && playbackRequested
                && interruptedEpisodeID == episodeID
                && !didReachNaturalEnd
            interruptedEpisodeID = nil
            return mayResume ? .resume : .none

        case .routeChanged(let reason, _, let current):
            route = current
            guard reason == .oldDeviceUnavailable else { return .none }
            interruptedEpisodeID = nil
            interruption = .none
            return episodeID == nil ? .none : .pauseAndPersist

        case .mediaServicesWereReset(let route):
            self.route = route
            interruptedEpisodeID = nil
            interruption = .none
            guard episodeID != nil else { return .none }
            return playbackRequested && !didReachNaturalEnd
                ? .rebuildAndResume
                : .pauseAndPersist
        }
    }

    mutating func invalidateResumeIntent() {
        interruptedEpisodeID = nil
    }
}
