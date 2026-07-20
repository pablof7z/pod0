import Foundation
import os.log

// MARK: - AgentTTSComposer
//
// Synthesises a sequence of `TTSTurn` values into a single stitched m4a and
// publishes the result as a new episode on the agent-generated virtual podcast.
//
// Turn types:
//   .speech   — text → ElevenLabs TTS → temp mp3 → stitched in
//   .snippet  — existing episode clip → time-trimmed via ComposedAudioStitcher
//
// After stitching, a `Transcript` observation is committed to the shared core.
// Chapters are synthesised directly from the turn structure
// (consecutive speech turns collapse into a single chapter; each snippet turn
// gets its own chapter with the source episode's artwork and `sourceEpisodeID`).
// The raw composition is qualified and committed by the shared Rust core.

final class AgentTTSComposer: TTSPublisherProtocol, @unchecked Sendable {

    // MARK: - Dependencies

    private let ttsClient: ElevenLabsTTSClient
    weak var store: AppStateStore?
    weak var playback: PlaybackState?

    static let logger = Logger.app("AgentTTSComposer")

    // MARK: - Voice configuration

    private static let defaultVoiceIDKey = "io.f7z.podcast.agent.defaultVoiceID"

    init(store: AppStateStore, playback: PlaybackState) {
        self.store = store
        self.playback = playback
        self.ttsClient = ElevenLabsTTSClient()
    }

    func defaultVoiceID() -> String {
        UserDefaults.standard.string(forKey: Self.defaultVoiceIDKey)
            ?? ElevenLabsTTSClient.defaultVoiceID
    }

    func setDefaultVoiceID(_ voiceID: String) {
        UserDefaults.standard.set(voiceID, forKey: Self.defaultVoiceIDKey)
    }

    // MARK: - TTSPublisherProtocol

    func generateAndPublish(
        title: String,
        description: String?,
        turns: [TTSTurn],
        playNow: Bool,
        generationSource: Episode.GenerationSource? = nil,
        targetPodcastID: UUID? = nil
    ) async throws -> TTSEpisodeResult {
        guard !turns.isEmpty else {
            throw AgentTTSError.emptyTurns
        }
        guard ttsClient.isConfigured else {
            throw AgentTTSError.notConfigured
        }

        // 1. Build AudioComposeTrack list (one per turn); skips tracks whose audio
        //    fails to load so chapter math stays in sync with tracks.
        let (tracks, trackDurations, survivingTurns) = try await buildTracks(for: turns)

        // 2. Stitch tracks into a single m4a.
        let episodeID = UUID()
        let outputURL = try AgentGeneratedPodcastService.audioFileURL(episodeID: episodeID)
        let durationSeconds = try await ComposedAudioStitcher.stitch(tracks: tracks, outputURL: outputURL)

        // 3. Build chapters and transcript from SURVIVING turns + resolved
        //    durations — uses the filtered list so indices stay aligned.
        let (chapters, transcript) = await buildChaptersAndTranscript(
            turns: survivingTurns,
            trackDurations: trackDurations,
            episodeID: episodeID
        )

        // 3b. Inherit artwork from the first snippet chapter that has one —
        // covers the typical case where the TTS-stitched episode includes
        // clips from a real show, so the result carries that show's image
        // even though the synthetic "Agent Generated" podcast itself has
        // none.
        let inheritedArtwork = chapters.first(where: { $0.imageURL != nil })?.imageURL

        // 4. Register the episode and optionally start playback.
        guard let store else { throw AgentTTSError.storeUnavailable }
        let episode = try await AgentGeneratedPodcastService.publishEpisode(
            title: title,
            description: description ?? "",
            audioURL: outputURL,
            durationSeconds: durationSeconds,
            imageURL: inheritedArtwork,
            generationSource: generationSource,
            targetPodcastID: targetPodcastID,
            in: store
        )
        let committedTranscript = transcript.replacingEpisodeID(with: episode.id)

        // 5. Commit both durable observations through the shared core.
        try await commitGeneratedTranscript(committedTranscript, for: episode)
        try await commitGeneratedChapters(
            chapters,
            durationSeconds: durationSeconds,
            for: episode
        )

        // 6. Optionally start playback.
        if playNow {
            await MainActor.run {
                guard let playback else { return }
                playback.setEpisode(store.episode(id: episode.id) ?? episode)
                playback.seek(to: 0)
                playback.play()
            }
        }

        return TTSEpisodeResult(
            episodeID: episode.id.uuidString,
            podcastID: episode.podcastID.uuidString,
            title: title,
            durationSeconds: durationSeconds,
            publishedToLibrary: true
        )
    }

    // MARK: - Track building

    /// Builds `AudioComposeTrack` values and returns the per-turn audio durations
    /// plus the surviving turns (turns whose audio loaded successfully).
    ///
    /// A turn is silently skipped — with an error log — when its audio asset
    /// fails to load or reports a zero duration. This prevents fictional
    /// durations from corrupting chapter start-time math. If every turn is
    /// skipped, throws `AgentTTSError.noPlayableContent`.
    private func buildTracks(for turns: [TTSTurn]) async throws -> (
        tracks: [AudioComposeTrack],
        durations: [Double],
        survivingTurns: [TTSTurn]
    ) {
        var tracks: [AudioComposeTrack] = []
        var durations: [Double] = []
        var survivingTurns: [TTSTurn] = []
        let dummySegmentID = UUID()

        for (index, turn) in turns.enumerated() {
            switch turn.kind {
            case .speech(let text, let voiceIDOverride):
                let voice = voiceIDOverride ?? defaultVoiceID()
                let audioURL = try await synthesizeSpeech(text: text, voiceID: voice, index: index)
                let duration: TimeInterval
                do {
                    duration = try await audioDuration(of: audioURL)
                } catch {
                    Self.logger.error(
                        "AgentTTSComposer: skipping speech turn \(index, privacy: .public) — duration unavailable for \(audioURL.lastPathComponent, privacy: .public): \(error.localizedDescription, privacy: .public)"
                    )
                    continue
                }
                tracks.append(AudioComposeTrack(
                    segmentID: dummySegmentID,
                    indexInSegment: index,
                    kind: .tts,
                    audioURL: audioURL,
                    startInTrackSeconds: 0,
                    endInTrackSeconds: duration,
                    transcriptText: text
                ))
                durations.append(duration)
                survivingTurns.append(turn)

            case .snippet(let episodeID, let start, let end, let label):
                let enclosureURL = try await resolveEpisodeAudio(episodeID: episodeID)
                let duration = end - start
                tracks.append(AudioComposeTrack(
                    segmentID: dummySegmentID,
                    indexInSegment: index,
                    kind: .quote,
                    audioURL: enclosureURL,
                    startInTrackSeconds: start,
                    endInTrackSeconds: end,
                    transcriptText: label ?? ""
                ))
                durations.append(duration)
                survivingTurns.append(turn)
            }
        }

        guard !tracks.isEmpty else {
            throw AgentTTSError.noPlayableContent
        }

        return (tracks, durations, survivingTurns)
    }

    // MARK: - Speech synthesis → temp file

    private func synthesizeSpeech(text: String, voiceID: String, index: Int) async throws -> URL {
        let tmpURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-tts-\(index)-\(UUID().uuidString).mp3")

        var collectedData = Data()
        let stream = ttsClient.synthesizeStream(text: text, voiceID: voiceID)
        for try await chunk in stream {
            collectedData.append(chunk)
        }

        guard !collectedData.isEmpty else {
            throw AgentTTSError.emptyAudioData(index: index)
        }

        try collectedData.write(to: tmpURL, options: .atomic)
        Self.logger.debug("AgentTTSComposer: synthesised turn \(index, privacy: .public) → \(tmpURL.lastPathComponent, privacy: .public)")
        return tmpURL
    }

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
        }
    }
}
