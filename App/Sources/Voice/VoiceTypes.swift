import Foundation

// MARK: - VoiceError

/// Equatable error surface for `AudioConversationManager.state`.
///
/// We use an enum rather than the raw `Error` protocol so the state machine
/// case `.error(VoiceError)` stays Equatable for SwiftUI `.onChange(of:)` and
/// animation triggers.
enum VoiceError: Error, Equatable, Sendable {
    case permissionDenied
    case recognizerUnavailable
    case ttsFailed(String)
    case agentFailed(String)
    case audioRouteFailed(String)
    case unknown(String)

    init(from error: Error) {
        switch error {
        case let speech as SpeechRecognizerError:
            switch speech {
            case .permissionDenied: self = .permissionDenied
            case .recognizerUnavailable: self = .recognizerUnavailable
            case .audioEngineFailed(let msg): self = .audioRouteFailed(msg)
            case .sessionAlreadyRunning: self = .unknown("STT session already running")
            }
        case let tts as ElevenLabsTTSError:
            switch tts {
            case .missingAPIKey: self = .ttsFailed("Missing API key")
            case .webSocketFailed(let msg): self = .ttsFailed(msg)
            case .restFailed(let code): self = .ttsFailed("HTTP \(code)")
            case .decodeFailed(let msg): self = .ttsFailed(msg)
            }
        default:
            self = .unknown(error.localizedDescription)
        }
    }
}

// MARK: - AudioConversationState

/// State machine for the voice conversation.
///
/// Transitions:
/// ```
///                      ┌──────── interrupt ─────────┐
///                      ▼                            │
/// idle ── PTT/ambient ──▶ listening ──▶ thinking ──▶ speaking
///   ▲           barge-in │              │            │
///   │                    │              │            │ briefing handoff
///   │                    │              ▼            ▼
///   └──── exit ──────────┴──── error ◀──────── duckedWhileBriefing
/// ```
enum AudioConversationState: Equatable, Sendable {
    case idle
    case listening
    case thinking
    case speaking
    case duckedWhileBriefing
    case error(VoiceError)
}

// MARK: - VoiceBriefingHandle

/// Opaque handle Lane 9 (Briefings) hands us so we know when to resume.
/// `waitUntilFinished` returns when the briefing's audio has stopped.
@MainActor
protocol VoiceBriefingHandle: AnyObject {
    func waitUntilFinished() async
}
