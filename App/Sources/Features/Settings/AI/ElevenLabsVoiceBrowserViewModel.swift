import AVFoundation
import Observation
import SwiftUI
import os.log

@MainActor
@Observable
final class ElevenLabsVoiceBrowserViewModel {
    private let logger = Logger.app("ElevenLabsVoiceBrowserViewModel")

    enum Phase: Equatable {
        case idle
        case loading
        case loaded
        case error(String)
        case needsAPIKey
    }

    private(set) var phase: Phase = .idle
    private(set) var voices: [ElevenLabsVoice] = []
    private(set) var playingVoiceID: String?
    private(set) var loadingPreviewVoiceID: String?

    private let service = ElevenLabsVoicesService()
    private let player = PreviewPlayer()

    init() {
        Task { @MainActor [weak self] in
            let stream = NotificationCenter.default.notifications(named: .AVPlayerItemDidPlayToEndTime)
            for await _ in stream {
                guard let self else { return }
                self.handlePlaybackEnded()
            }
        }
    }

    func loadIfNeeded() async {
        guard voices.isEmpty, phase != .loading else { return }
        await reload()
    }

    func reload() async {
        let apiKey: String?
        do {
            apiKey = try ElevenLabsCredentialStore.apiKey()
        } catch {
            logger.error("ElevenLabsVoiceBrowserViewModel: Keychain read failed — \(error, privacy: .public)")
            apiKey = nil
        }
        guard let apiKey, !apiKey.isEmpty else {
            phase = .needsAPIKey
            voices = []
            return
        }

        phase = .loading
        do {
            let result = try await service.fetchVoices(apiKey: apiKey)
            voices = result
            phase = .loaded
        } catch ElevenLabsVoicesError.unauthorized {
            phase = .needsAPIKey
            voices = []
        } catch {
            phase = .error(error.localizedDescription)
        }
    }

    func togglePreview(for voice: ElevenLabsVoice) {
        if playingVoiceID == voice.voiceID {
            stopPreview()
            return
        }
        guard let url = voice.previewURL else { return }
        player.play(url: url)
        playingVoiceID = voice.voiceID
        loadingPreviewVoiceID = nil
        Haptics.light()
    }

    func stopPreview() {
        player.stop()
        playingVoiceID = nil
        loadingPreviewVoiceID = nil
    }

    private func handlePlaybackEnded() {
        playingVoiceID = nil
        loadingPreviewVoiceID = nil
    }

    // MARK: - PreviewPlayer

    private final class PreviewPlayer {
        private var player: AVPlayer?

        func play(url: URL) {
            player?.pause()
            configureElevenLabsAudioPlaybackSession()
            let item = AVPlayerItem(url: url)
            let newPlayer = AVPlayer(playerItem: item)
            newPlayer.automaticallyWaitsToMinimizeStalling = false
            player = newPlayer
            newPlayer.play()
        }

        func stop() {
            player?.pause()
            player = nil
        }
    }
}
