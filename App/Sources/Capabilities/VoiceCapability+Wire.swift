import Foundation

// MARK: - Voice capability wire vocabulary
//
// Swift mirror of the Rust types in
// `apps/nmp-app-podcast/src/capability/voice.rs`. The Rust enums are
// `#[serde(tag = "type", rename_all = "snake_case")]`; the manual
// `Codable` impls below match that wire shape exactly so a JSON string
// produced on one side decodes on the other.
//
// Split out of `VoiceCapability.swift` to keep that file under the
// 300-line soft limit (AGENTS.md).

/// Commands Rust dispatches to the iOS voice executor. Mirrors
/// `crate::capability::VoiceCommand`.
enum VoiceCommand: Decodable, Equatable {
    case startListening
    case stopListening
    case speak(text: String, voiceID: String?, requestID: String)
    case stop
    case setVoice(voiceID: String)

    private enum CodingKeys: String, CodingKey {
        case type
        case text
        case voiceID = "voice_id"
        case requestID = "request_id"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "start_listening":
            self = .startListening
        case "stop_listening":
            self = .stopListening
        case "speak":
            self = .speak(
                text: try c.decode(String.self, forKey: .text),
                voiceID: try c.decodeIfPresent(String.self, forKey: .voiceID),
                requestID: try c.decode(String.self, forKey: .requestID))
        case "stop":
            self = .stop
        case "set_voice":
            self = .setVoice(voiceID: try c.decode(String.self, forKey: .voiceID))
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type, in: c, debugDescription: "unknown voice command: \(type)")
        }
    }
}

/// Reports the iOS voice executor pushes back to Rust. Mirrors
/// `crate::capability::VoiceReport`.
enum VoiceReport: Encodable, Equatable {
    case started(requestID: String)
    case finished(requestID: String)
    case failed(requestID: String, error: String)
    case stopped
    case listeningStarted
    case listeningStopped
    case transcriptPartial(text: String)
    case transcriptFinal(text: String)
    case error(message: String)

    private enum CodingKeys: String, CodingKey {
        case type
        case text
        case requestID = "request_id"
        case error
        case message
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .started(id):
            try c.encode("started", forKey: .type)
            try c.encode(id, forKey: .requestID)
        case let .finished(id):
            try c.encode("finished", forKey: .type)
            try c.encode(id, forKey: .requestID)
        case let .failed(id, err):
            try c.encode("failed", forKey: .type)
            try c.encode(id, forKey: .requestID)
            try c.encode(err, forKey: .error)
        case .stopped:
            try c.encode("stopped", forKey: .type)
        case .listeningStarted:
            try c.encode("listening_started", forKey: .type)
        case .listeningStopped:
            try c.encode("listening_stopped", forKey: .type)
        case let .transcriptPartial(text):
            try c.encode("transcript_partial", forKey: .type)
            try c.encode(text, forKey: .text)
        case let .transcriptFinal(text):
            try c.encode("transcript_final", forKey: .type)
            try c.encode(text, forKey: .text)
        case let .error(message):
            try c.encode("error", forKey: .type)
            try c.encode(message, forKey: .message)
        }
    }

    /// Encode to a JSON string. Returns `nil` on encoder failure
    /// (treated by callers as "no-op" per D6).
    func jsonString() -> String? {
        guard let data = try? JSONEncoder().encode(self) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
