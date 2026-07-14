import AVFoundation
import Foundation
import os.log

// MARK: - AgentTTSComposer
//
// Synthesises a sequence of `TTSTurn` values into a single stitched m4a and
// publishes the result as a new episode on the agent-generated virtual podcast.
//
// Turn types:
//   .speech   — text → ElevenLabs TTS → temp mp3 → stitched in
//   .snippet  — existing episode clip → time-trimmed via BriefingAudioStitcher
//
// After stitching, a `Transcript` is built from the turn text and saved to
// `TranscriptStore`. Chapters are synthesised directly from the turn structure
// (consecutive speech turns collapse into a single chapter; each snippet turn
// gets its own chapter with the source episode's artwork and `sourceEpisodeID`).
// `adSegments` is set to `[]` so `AIChapterCompiler` skips re-processing.

final class AgentTTSComposer: TTSPublisherProtocol, @unchecked Sendable {

    // MARK: - Dependencies

    private let ttsClient: ElevenLabsTTSClient
    weak var store: AppStateStore?
    weak var playback: PlaybackState?

    private static let logger = Logger.app("AgentTTSComposer")

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

        // 1. Build BriefingTrack list (one per turn); skips tracks whose audio
        //    fails to load so chapter math stays in sync with tracks.
        let (tracks, trackDurations, survivingTurns) = try await buildTracks(for: turns)

        // 2. Stitch tracks into a single m4a.
        let episodeID = UUID()
        let outputURL = try AgentGeneratedPodcastService.audioFileURL(episodeID: episodeID)
        let durationSeconds = try await BriefingAudioStitcher.stitch(tracks: tracks, outputURL: outputURL)

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
        let episode = await MainActor.run {
            guard let store else { return Optional<Episode>.none }
            return AgentGeneratedPodcastService.publishEpisode(
                title: title,
                description: description ?? "",
                audioURL: outputURL,
                durationSeconds: durationSeconds,
                imageURL: inheritedArtwork,
                generationSource: generationSource,
                targetPodcastID: targetPodcastID,
                in: store
            )
        }

        guard let episode else {
            throw AgentTTSError.storeUnavailable
        }

        // 5. Persist transcript, chapters, and set adSegments = [] so
        //    AIChapterCompiler skips this already-structured episode.
        await MainActor.run {
            guard let store else { return }
            try? TranscriptStore.shared.save(transcript)
            store.setEpisodeTranscriptState(episode.id, state: .ready(source: .other))
            store.setEpisodeChapters(episode.id, chapters: chapters)
            store.setEpisodeAdSegments(episode.id, segments: [])
        }

        // 6. Optionally start playback.
        if playNow {
            await MainActor.run {
                guard let playback else { return }
                playback.setEpisode(episode)
                playback.seek(to: 0)
                playback.play()
            }
        }

        let podcastID: String
        if let targetPodcastID {
            podcastID = targetPodcastID.uuidString
        } else {
            podcastID = await MainActor.run {
                store?.podcast(feedURL: AgentGeneratedPodcastService.sentinelFeedURL)?.id.uuidString ?? ""
            }
        }

        return TTSEpisodeResult(
            episodeID: episode.id.uuidString,
            podcastID: podcastID,
            title: title,
            durationSeconds: durationSeconds,
            publishedToLibrary: true
        )
    }

    // MARK: - Track building

    /// Builds `BriefingTrack` values and returns the per-turn audio durations
    /// plus the surviving turns (turns whose audio loaded successfully).
    ///
    /// A turn is silently skipped — with an error log — when its audio asset
    /// fails to load or reports a zero duration. This prevents fictional
    /// durations from corrupting chapter start-time math. If every turn is
    /// skipped, throws `AgentTTSError.noPlayableContent`.
    private func buildTracks(for turns: [TTSTurn]) async throws -> (
        tracks: [BriefingTrack],
        durations: [Double],
        survivingTurns: [TTSTurn]
    ) {
        var tracks: [BriefingTrack] = []
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
                tracks.append(BriefingTrack(
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
                tracks.append(BriefingTrack(
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

    // MARK: - Chapter + transcript building

    /// Converts the turn sequence into `Episode.Chapter` values and a
    /// `Transcript`. Consecutive speech turns collapse into a single chapter;
    /// each snippet turn gets its own chapter with the source episode's
    /// artwork URL and `sourceEpisodeID` set for the player chip.
    private func buildChaptersAndTranscript(
        turns: [TTSTurn],
        trackDurations: [Double],
        episodeID: UUID
    ) async -> ([Episode.Chapter], Transcript) {
        var chapters: [Episode.Chapter] = []
        var transcriptSegments: [Segment] = []
        var cursor: TimeInterval = 0

        // Accumulator for consecutive speech turns.
        var speechStart: TimeInterval?
        var speechTexts: [String] = []

        func flushSpeechChapter() {
            guard !speechTexts.isEmpty, let start = speechStart else { return }
            let combinedText = speechTexts.joined(separator: " ")
            let preview = String(combinedText.prefix(60))
            let chapterTitle = combinedText.count <= 60 ? combinedText : preview + "…"
            chapters.append(Episode.Chapter(
                startTime: start,
                title: chapterTitle,
                isAIGenerated: true
            ))
            speechStart = nil
            speechTexts = []
        }

        // turns and trackDurations are guaranteed to be parallel and
        // contain only positive durations (buildTracks filters skipped tracks).
        for (index, turn) in turns.enumerated() {
            let duration = index < trackDurations.count ? trackDurations[index] : 0

            switch turn.kind {
            case .speech(let text, _):
                if speechStart == nil { speechStart = cursor }
                speechTexts.append(text)

                // Each speech turn is a transcript segment.
                transcriptSegments.append(Segment(
                    start: cursor,
                    end: cursor + duration,
                    text: text
                ))

            case .snippet(let sourceID, let snippetStart, _, let label):
                // Close any open speech chapter first.
                flushSpeechChapter()

                // Resolve the source episode's artwork for the mid-play swap.
                let artworkURL = await MainActor.run { [weak self] () -> URL? in
                    guard let self, let store = self.store else { return nil }
                    guard let uuid = UUID(uuidString: sourceID),
                          let ep = store.episode(id: uuid) else { return nil }
                    return ep.imageURL ?? store.podcast(id: ep.podcastID)?.imageURL
                }

                let chapterTitle: String
                if let nonEmpty = label, !nonEmpty.isEmpty {
                    chapterTitle = nonEmpty
                } else if let resolved = await resolveEpisodeTitle(episodeID: sourceID) {
                    chapterTitle = resolved
                } else {
                    // Episode not in store — use a time-anchored fallback so
                    // the chapter still has a meaningful label.
                    let minutes = Int(snippetStart) / 60
                    let seconds = Int(snippetStart) % 60
                    chapterTitle = String(format: "Quote at %d:%02d", minutes, seconds)
                }

                chapters.append(Episode.Chapter(
                    startTime: cursor,
                    title: chapterTitle,
                    imageURL: artworkURL,
                    isAIGenerated: true,
                    sourceEpisodeID: sourceID
                ))

                // Snippet text becomes a transcript segment too.
                if let labelText = label, !labelText.isEmpty {
                    transcriptSegments.append(Segment(
                        start: cursor,
                        end: cursor + duration,
                        text: labelText
                    ))
                }
            }

            cursor += duration
        }

        // Flush any trailing speech turns.
        flushSpeechChapter()

        let transcript = Transcript(
            episodeID: episodeID,
            language: "en",
            source: .onDevice,
            segments: transcriptSegments
        )

        return (chapters, transcript)
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

    // MARK: - Episode audio resolution

    /// Returns a local URL for the episode's audio, downloading it first if needed.
    /// Polls up to `timeout` seconds for the download to complete.
    private func resolveEpisodeAudio(episodeID: EpisodeID, timeout: TimeInterval = 300) async throws -> URL {
        guard let uuid = UUID(uuidString: episodeID) else {
            throw AgentTTSError.snippetEpisodeNotFound(episodeID: episodeID)
        }

        // Check current state and trigger download if needed.
        let alreadyReady: URL? = await MainActor.run {
            guard let episode = store?.episode(id: uuid) else { return nil }
            if case .downloaded = episode.downloadState {
                let localURL = EpisodeDownloadStore.shared.localFileURL(for: episode)
                if FileManager.default.fileExists(atPath: localURL.path) {
                    return localURL
                }
            }
            EpisodeDownloadService.shared.download(episodeID: uuid)
            return nil
        }
        if let url = alreadyReady { return url }

        // Episode not in store at all.
        let episodeExists = await MainActor.run { store?.episode(id: uuid) != nil }
        guard episodeExists else {
            throw AgentTTSError.snippetEpisodeNotFound(episodeID: episodeID)
        }

        Self.logger.info("AgentTTSComposer: waiting for download of snippet episode \(episodeID, privacy: .public)")

        // Poll until downloaded, failed, or timed out.
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            try await Task.sleep(for: .seconds(1))

            let result: Result<URL, Error>? = await MainActor.run {
                guard let episode = store?.episode(id: uuid) else { return nil }
                switch episode.downloadState {
                case .downloaded:
                    let localURL = EpisodeDownloadStore.shared.localFileURL(for: episode)
                    return .success(localURL)
                case .failed(let message):
                    return .failure(AgentTTSError.snippetDownloadFailed(episodeID: episodeID, message: message))
                default:
                    return nil
                }
            }
            switch result {
            case .success(let url): return url
            case .failure(let error): throw error
            case nil: continue
            }
        }

        throw AgentTTSError.snippetDownloadTimeout(episodeID: episodeID)
    }

    /// Returns the episode title for the given ID, or `nil` when the episode
    /// cannot be found in the store. `nil` lets the caller compose a more
    /// meaningful fallback than a generic string.
    private func resolveEpisodeTitle(episodeID: String) async -> String? {
        await MainActor.run {
            guard let uuid = UUID(uuidString: episodeID),
                  let episode = store?.episode(id: uuid) else {
                Self.logger.error(
                    "AgentTTSComposer: episode not found for chapter title lookup — episodeID=\(episodeID, privacy: .public)"
                )
                return nil
            }
            return episode.title
        }
    }

    // MARK: - Audio duration helper

    /// Loads the playback duration of an audio asset.
    ///
    /// Throws `AudioDurationError` on load failure or a zero/negative duration.
    /// Callers must skip the track rather than substituting a fictional length,
    /// which would corrupt chapter start-time math for all subsequent tracks.
    private func audioDuration(of url: URL) async throws -> TimeInterval {
        let asset = AVURLAsset(url: url)
        do {
            let duration = try await asset.load(.duration)
            let seconds = CMTimeGetSeconds(duration)
            guard seconds > 0 else {
                throw AudioDurationError.zeroDuration(url)
            }
            return seconds
        } catch let err as AudioDurationError {
            throw err
        } catch {
            throw AudioDurationError.assetLoadFailed(url, underlying: error)
        }
    }
}

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
