import Foundation

// MARK: - Audio capability wire vocabulary
//
// Swift mirror of the Rust types in
// `apps/nmp-app-podcast/src/capability/audio.rs`. The Rust enums are
// `#[serde(tag = "type", rename_all = "snake_case")]`; the manual
// `Codable` impls below match that wire shape exactly so a JSON string
// produced on one side decodes on the other.
//
// Split out of `AudioCapability.swift` to keep that file under the
// 500-line hard limit (AGENTS.md).

/// Commands Rust dispatches to the iOS audio executor.
enum AudioCommand: Decodable, Equatable {
    case load(url: String, positionSecs: Double)
    case play
    case pause
    case seek(positionSecs: Double)
    case setVolume(volume: Float)
    case setSpeed(speed: Float)
    case setSleepTimer(secs: UInt64?)
    case stop

    private enum CodingKeys: String, CodingKey {
        case type
        case url
        case positionSecs = "position_secs"
        case volume
        case speed
        case secs
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "load":
            self = .load(
                url: try c.decode(String.self, forKey: .url),
                positionSecs: try c.decode(Double.self, forKey: .positionSecs))
        case "play":
            self = .play
        case "pause":
            self = .pause
        case "seek":
            self = .seek(positionSecs: try c.decode(Double.self, forKey: .positionSecs))
        case "set_volume":
            self = .setVolume(volume: try c.decode(Float.self, forKey: .volume))
        case "set_speed":
            self = .setSpeed(speed: try c.decode(Float.self, forKey: .speed))
        case "set_sleep_timer":
            self = .setSleepTimer(secs: try c.decodeIfPresent(UInt64.self, forKey: .secs))
        case "stop":
            self = .stop
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type, in: c, debugDescription: "unknown audio command: \(type)")
        }
    }
}

/// Events the iOS audio executor pushes back to Rust.
enum AudioReport: Encodable, Equatable {
    case playing(url: String, positionSecs: Double, durationSecs: Double)
    case paused(url: String, positionSecs: Double)
    case stopped
    case failed(url: String, error: String)
    case bufferingProgress(fraction: Float)
    case sleepTimerFired

    private enum CodingKeys: String, CodingKey {
        case type
        case url
        case positionSecs = "position_secs"
        case durationSecs = "duration_secs"
        case error
        case fraction
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .playing(url, position, duration):
            try c.encode("playing", forKey: .type)
            try c.encode(url, forKey: .url)
            try c.encode(position, forKey: .positionSecs)
            try c.encode(duration, forKey: .durationSecs)
        case let .paused(url, position):
            try c.encode("paused", forKey: .type)
            try c.encode(url, forKey: .url)
            try c.encode(position, forKey: .positionSecs)
        case .stopped:
            try c.encode("stopped", forKey: .type)
        case let .failed(url, error):
            try c.encode("failed", forKey: .type)
            try c.encode(url, forKey: .url)
            try c.encode(error, forKey: .error)
        case let .bufferingProgress(fraction):
            try c.encode("buffering_progress", forKey: .type)
            try c.encode(fraction, forKey: .fraction)
        case .sleepTimerFired:
            try c.encode("sleep_timer_fired", forKey: .type)
        }
    }

    /// Encode to a JSON string. Returns `nil` on the (impossible) serde
    /// failure — callers treat `nil` as "no-op" per D6.
    func jsonString() -> String? {
        guard let data = try? JSONEncoder().encode(self) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
