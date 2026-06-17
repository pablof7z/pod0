import Foundation

// MARK: - Audio duration errors

enum AudioDurationError: Error {
    case zeroDuration(URL)
    case assetLoadFailed(URL, underlying: Error)
}

// MARK: - Errors

enum AgentTTSError: LocalizedError {
    case emptyTurns
    case notConfigured
    case emptyAudioData(index: Int)
    case storeUnavailable
    case snippetEpisodeNotFound(episodeID: String)
    case snippetDownloadFailed(episodeID: String, message: String)
    case snippetDownloadTimeout(episodeID: String)
    case noPlayableContent
    case plannerFailed(String)

    var errorDescription: String? {
        switch self {
        case .emptyTurns:
            return "generate_tts_episode requires at least one turn."
        case .notConfigured:
            return "ElevenLabs API key is not configured. Add it in Settings → AI."
        case .emptyAudioData(let index):
            return "TTS synthesis returned no audio for turn \(index)."
        case .storeUnavailable:
            return "AppStateStore is unavailable; cannot publish episode."
        case .snippetEpisodeNotFound(let episodeID):
            return "Snippet episode \(episodeID) was not found in the library."
        case .snippetDownloadFailed(let episodeID, let message):
            return "Download failed for snippet episode \(episodeID): \(message)"
        case .snippetDownloadTimeout(let episodeID):
            return "Timed out waiting for snippet episode \(episodeID) to download (5 min limit)."
        case .noPlayableContent:
            return "All TTS tracks failed audio loading; nothing to stitch."
        case .plannerFailed(let message):
            return "Generated episode planner failed: \(message)"
        }
    }
}
