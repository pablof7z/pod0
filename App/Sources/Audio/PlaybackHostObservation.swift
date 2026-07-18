import Foundation

/// Coarse native-player state emitted at lifecycle boundaries. This is a
/// screen/diagnostic projection, not a second playback source of truth.
enum PlaybackHostState: String, Codable, Equatable, Sendable {
    case idle
    case loading
    case playing
    case paused
    case buffering
    case failed
}
/// Stable route vocabulary. Platform port names stay inside the native host.
enum PlaybackAudioRoute: String, Codable, Equatable, Sendable {
    case builtIn
    case wired
    case bluetooth
    case airPlay
    case car
    case external
    case unknown
}

enum PlaybackInterruption: String, Codable, Equatable, Sendable {
    case none
    case began
    case endedShouldResume
    case endedShouldRemainPaused
}

/// Typed evidence captured after a host lifecycle event. Durable playback
/// policy may consume an equivalent projection through the future Rust facade;
/// AVFoundation-specific details never cross that boundary.
struct PlaybackObservation: Codable, Equatable, Sendable {
    let episodeID: UUID?
    let hostState: PlaybackHostState
    let positionMilliseconds: Int64
    let durationMilliseconds: Int64
    let route: PlaybackAudioRoute
    let interruption: PlaybackInterruption
    let ended: Bool
    let observedAt: Date
}
