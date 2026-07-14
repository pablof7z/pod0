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
    case bodyEncoding(String)
    case server(Int)
    case transport(String)
    case playback(String)

    var errorDescription: String? {
        switch self {
        case .missingAPIKey:       return "No ElevenLabs API key. Connect first."
        case .missingVoiceID:      return "No voice selected. Pick a voice first."
        case .bodyEncoding(let m): return "Could not encode request body: \(m)"
        case .server(let code):    return "ElevenLabs error (HTTP \(code))."
        case .transport(let m):    return m
        case .playback(let m):     return "Playback failed: \(m)"
        }
    }
}

@MainActor
final class ElevenLabsTTSPreviewService {
    private let logger = Logger.app("ElevenLabsTTSPreviewService")
    private var audioPlayer: AVAudioPlayer?

    // MARK: - Constants

    static let samplePhrase = "Hello! This is a preview of the selected ElevenLabs voice."

    private enum API {
        static let defaultModel = "eleven_turbo_v2_5"
        static let voiceStability: Double = 0.5
        static let voiceSimilarityBoost: Double = 0.75
        static let timeoutInterval: TimeInterval = 20
        static func endpointURL(voiceID: String) -> URL? {
            URL(string: "https://api.elevenlabs.io/v1/text-to-speech/\(voiceID)")
        }
    }

    func speak(voiceID: String, model: String) async throws {
        guard !voiceID.isEmpty else { throw ElevenLabsTTSPreviewError.missingVoiceID }

        let apiKey: String
        do {
            guard let key = try ElevenLabsCredentialStore.apiKey(), !key.isEmpty else {
                throw ElevenLabsTTSPreviewError.missingAPIKey
            }
            apiKey = key
        } catch let e as ElevenLabsTTSPreviewError {
            throw e
        } catch {
            throw ElevenLabsTTSPreviewError.missingAPIKey
        }

        let trimmedModel = model.trimmed
        let effectiveModel = trimmedModel.isEmpty ? API.defaultModel : trimmedModel

        guard let url = API.endpointURL(voiceID: voiceID) else {
            throw ElevenLabsTTSPreviewError.transport("Invalid voice URL.")
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue(apiKey, forHTTPHeaderField: "xi-api-key")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("audio/mpeg", forHTTPHeaderField: "Accept")
        request.timeoutInterval = API.timeoutInterval

        let body: [String: Any] = [
            "text": Self.samplePhrase,
            "model_id": effectiveModel,
            "voice_settings": [
                "stability": API.voiceStability,
                "similarity_boost": API.voiceSimilarityBoost
            ]
        ]
        do {
            request.httpBody = try JSONSerialization.data(withJSONObject: body)
        } catch {
            logger.error("Failed to encode TTS request body: \(error, privacy: .public)")
            throw ElevenLabsTTSPreviewError.bodyEncoding(error.localizedDescription)
        }

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await URLSession.shared.data(for: request)
        } catch {
            throw ElevenLabsTTSPreviewError.transport(error.localizedDescription)
        }

        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            throw ElevenLabsTTSPreviewError.server(http.statusCode)
        }

        configureElevenLabsAudioPlaybackSession()
        do {
            let player = try AVAudioPlayer(data: data, fileTypeHint: "mp3")
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
}
