import AVFoundation
import Foundation
import os.log

private let audioLogger = Logger.app("ElevenLabsAudio")

// MARK: - Shared audio-session helper

/// Activates the shared `AVAudioSession` for `.playback` mode.
///
/// Called by both `ElevenLabsTTSPreviewService` and `ElevenLabsPreviewPlayer`
/// before starting audio so that a single policy controls all ElevenLabs audio output.
func configureElevenLabsAudioPlaybackSession() {
    let session = AVAudioSession.sharedInstance()
    do {
        try session.setCategory(.playback, mode: .default, options: [])
        try session.setActive(true, options: [])
    } catch {
        audioLogger.error("AVAudioSession configuration failed: \(error, privacy: .public)")
    }
}

enum ElevenLabsTTSPreviewError: LocalizedError {
    case missingAPIKey
    case missingVoiceID
    case server(Int)
    case transport(String)
    case playback(String)

    var errorDescription: String? {
        switch self {
        case .missingAPIKey:       return "No ElevenLabs API key. Connect first."
        case .missingVoiceID:      return "No voice selected. Pick a voice first."
        case .server(let code):    return "ElevenLabs error (HTTP \(code))."
        case .transport(let m):    return m
        case .playback(let m):     return "Playback failed: \(m)"
        }
    }
}

@MainActor
final class ElevenLabsTTSPreviewService {
    private let backend = ElevenLabsTTSBackendClient()
    private var audioPlayer: AVAudioPlayer?

    // MARK: - Constants

    static let samplePhrase = "Hello! This is a preview of the selected ElevenLabs voice."

    private enum API {
        static let defaultModel = "eleven_turbo_v2_5"
    }

    func speak(voiceID: String, model: String) async throws {
        guard !voiceID.isEmpty else { throw ElevenLabsTTSPreviewError.missingVoiceID }

        let trimmedModel = model.trimmed
        let effectiveModel = trimmedModel.isEmpty ? API.defaultModel : trimmedModel
        let audio: ElevenLabsSynthesizedAudio
        do {
            audio = try await backend.synthesize(
                text: Self.samplePhrase,
                voiceID: voiceID,
                model: effectiveModel
            )
        } catch let error as ElevenLabsTTSBackendError {
            throw Self.previewError(from: error)
        }

        configureElevenLabsAudioPlaybackSession()
        do {
            let player = try AVAudioPlayer(data: audio.data, fileTypeHint: "mp3")
            player.prepareToPlay()
            audioPlayer = player
            player.play()
        } catch {
            throw ElevenLabsTTSPreviewError.playback(error.localizedDescription)
        }
    }

    func stop() {
        audioPlayer?.stop()
        audioPlayer = nil
    }

    private static func previewError(from error: ElevenLabsTTSBackendError) -> ElevenLabsTTSPreviewError {
        switch error {
        case .missingAPIKey:
            return .missingAPIKey
        case .missingVoiceID:
            return .missingVoiceID
        case .http(let status, _):
            return .server(status)
        case .network(let message), .decoding(let message), .invalidRequest(let message):
            return .transport(message)
        case .emptyAudio:
            return .transport("ElevenLabs returned no audio.")
        case .kernelUnavailable:
            return .transport(error.localizedDescription)
        }
    }
}
