import AVFoundation
import Foundation
import os.log

// MARK: - AgentTTSComposer
//
// Synthesises speech/snippet turns into one stitched m4a, adds chapters and a
// transcript, then publishes the result as an agent-generated episode.

final class AgentTTSComposer: TTSPublisherProtocol, @unchecked Sendable {

    // MARK: - Dependencies

    private let ttsClient = ElevenLabsTTSBackendClient()
    weak var store: AppStateStore?

    private static let logger = Logger.app("AgentTTSComposer")

    init(store: AppStateStore) {
        self.store = store
    }

    func defaultVoiceID() async -> String {
        await MainActor.run { [weak self] in
            guard let response = self?.store?.kernel?.agentTTSDefaultVoiceEnvelope(),
                  let data = response.data(using: .utf8),
                  let envelope = try? KernelDecoding.makeDecoder().decode(DefaultVoiceEnvelope.self, from: data),
                  envelope.error == nil
            else {
                return ""
            }
            return envelope.result?.voiceID ?? ""
        }
    }

    func setDefaultVoiceID(_ voiceID: String) async {
        await MainActor.run { [weak self] in
            guard let store = self?.store else { return }
            _ = store.kernel?.dispatch(namespace: "podcast.settings", body: [
                "op": "set_eleven_labs_voice",
                "voice_id": voiceID,
                "voice_name": "",
            ])
        }
    }

    private struct DefaultVoiceEnvelope: Decodable {
        let result: DefaultVoiceResult?
        let error: String?
    }

    private struct DefaultVoiceResult: Decodable {
        let voiceID: String

        enum CodingKeys: String, CodingKey {
            case voiceID = "voice_id"
        }
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
        // 1. Build NarrationTrack list (one per turn); skips tracks whose audio
        //    fails to load so chapter math stays in sync with tracks.
        let (tracks, trackDurations, survivingTurns) = try await buildTracks(for: turns)

        // 2. Stitch tracks into a single m4a.
        let episodeID = UUID()
        let outputURL = try AgentGeneratedPodcastService.audioFileURL(episodeID: episodeID)
        let durationSeconds = try await NarrationAudioStitcher.stitch(tracks: tracks, outputURL: outputURL)

        // 3. Ask Rust to plan generated-episode metadata from raw host facts.
        //    Swift executed the capabilities above (TTS, durations, stitching);
        //    Rust owns chapter grouping, fallback labels, transcript segments,
        //    flat transcript text, and inherited-artwork selection.
        let plan = try await buildGeneratedEpisodePlan(
            turns: survivingTurns,
            trackDurations: trackDurations,
            episodeID: episodeID
        )

        // 4. Add the episode to the Rust kernel store (the source of truth) so
        //    it survives the `applyKernelState` full-replace tick and
        //    `publish_episode` can later resolve it by id. The kernel owns the
        //    episode lifecycle and generated metadata now; Swift only writes
        //    the audio file and reports raw source facts to the planner.
        let podcastID: String = await MainActor.run {
            guard let store else { return "" }
            guard let resolvedPodcastID = targetPodcastID
                ?? AgentGeneratedPodcastService.ensurePodcastID(in: store) else {
                return ""
            }
            store.kernelAddEpisode(
                podcastId: resolvedPodcastID.uuidString,
                episodeId: episodeID.uuidString,
                title: title,
                enclosureUrl: outputURL.absoluteString,
                description: description ?? "",
                durationSecs: durationSeconds,
                imageUrl: plan.inheritedArtworkURL,
                chapters: plan.chapters,
                transcript: plan.transcriptText.nilIfEmpty
            )
            // Persist the timed transcript to the Swift TranscriptStore (file
            // I/O stays in Swift). The kernel holds the flat text for the
            // projection; the timed segments back the iOS transcript view.
            try? TranscriptStore.shared.save(plan.transcript(episodeID: episodeID))
            return resolvedPodcastID.uuidString
        }

        guard !podcastID.isEmpty else {
            throw AgentTTSError.storeUnavailable
        }

        // 5. Optionally start playback. Rust owns the generated episode row
        //    after `kernelAddEpisode`, so play through the player action path
        //    instead of constructing a local Swift-only `Episode`.
        if playNow {
            await MainActor.run {
                store?.kernelPlay(episodeID: episodeID, startSeconds: 0)
            }
        }

        return TTSEpisodeResult(
            episodeID: episodeID.uuidString,
            podcastID: podcastID,
            title: title,
            durationSeconds: durationSeconds,
            publishedToLibrary: true
        )
    }

    // MARK: - Track building

    /// Builds `NarrationTrack` values and returns the per-turn audio durations
    /// plus the surviving turns (turns whose audio loaded successfully).
    ///
    /// A turn is silently skipped — with an error log — when its audio asset
    /// fails to load or reports a zero duration. This prevents fictional
    /// durations from corrupting chapter start-time math. If every turn is
    /// skipped, throws `AgentTTSError.noPlayableContent`.
    private func buildTracks(for turns: [TTSTurn]) async throws -> (
        tracks: [NarrationTrack],
        durations: [Double],
        survivingTurns: [TTSTurn]
    ) {
        var tracks: [NarrationTrack] = []
        var durations: [Double] = []
        var survivingTurns: [TTSTurn] = []
        let dummySegmentID = UUID()

        for (index, turn) in turns.enumerated() {
            switch turn.kind {
            case .speech(let text, let voiceIDOverride):
                let voice = voiceIDOverride ?? await defaultVoiceID()
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
                tracks.append(NarrationTrack(
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
                tracks.append(NarrationTrack(
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

    // MARK: - Rust metadata planning

    /// Build the Rust planner request from raw host facts. Swift reads source
    /// episode title/artwork because those are already-rendered snapshot facts;
    /// Rust decides how to turn them into generated chapters/transcript.
    private func buildGeneratedEpisodePlan(
        turns: [TTSTurn],
        trackDurations: [Double],
        episodeID: UUID
    ) async throws -> GeneratedTTSEpisodePlan {
        var plannedTurns: [[String: Any]] = []
        for (index, turn) in turns.enumerated() {
            let duration = index < trackDurations.count ? trackDurations[index] : 0
            switch turn.kind {
            case .speech(let text, _):
                plannedTurns.append([
                    "kind": "speech",
                    "text": text,
                    "duration_secs": duration,
                ])

            case .snippet(let sourceID, let snippetStart, _, let label):
                var row: [String: Any] = [
                    "kind": "snippet",
                    "episode_id": sourceID,
                    "start_seconds": snippetStart,
                    "duration_secs": duration,
                ]
                if let label { row["label"] = label }
                let facts = await sourceEpisodeFacts(episodeID: sourceID)
                if let title = facts.title { row["source_episode_title"] = title }
                if let imageURL = facts.imageURL { row["image_url"] = imageURL }
                plannedTurns.append(row)
            }
        }

        let request: [String: Any] = ["turns": plannedTurns]
        let response = await MainActor.run { [weak self] in
            self?.store?.kernel?.agentTTSEpisodePlanEnvelope(request: request)
        }
        guard let response else {
            throw AgentTTSError.storeUnavailable
        }
        guard let data = response.data(using: .utf8) else {
            throw AgentTTSError.plannerFailed("invalid UTF-8 response")
        }
        do {
            let envelope = try KernelDecoding.makeDecoder().decode(GeneratedTTSPlanEnvelope.self, from: data)
            if let error = envelope.error {
                throw AgentTTSError.plannerFailed(error)
            }
            guard let result = envelope.result else {
                throw AgentTTSError.plannerFailed("missing result")
            }
            return result
        } catch let error as AgentTTSError {
            throw error
        } catch {
            throw AgentTTSError.plannerFailed(error.localizedDescription)
        }
    }

    private func sourceEpisodeFacts(episodeID: String) async -> (title: String?, imageURL: String?) {
        await MainActor.run { [weak self] in
            guard let self,
                  let store = self.store,
                  let uuid = UUID(uuidString: episodeID),
                  let episode = store.episode(id: uuid)
            else {
                Self.logger.error(
                    "AgentTTSComposer: episode not found for source fact lookup — episodeID=\(episodeID, privacy: .public)"
                )
                return (nil, nil)
            }
            let imageURL = episode.imageURL ?? store.podcast(id: episode.podcastID)?.imageURL
            return (episode.title, imageURL?.absoluteString)
        }
    }

    private struct GeneratedTTSPlanEnvelope: Decodable {
        let result: GeneratedTTSEpisodePlan?
        let error: String?
    }

    private struct GeneratedTTSEpisodePlan: Decodable {
        let chapters: [KernelEpisodeChapterPayload]
        let transcriptSegments: [GeneratedTranscriptSegment]
        let transcriptText: String
        let inheritedArtworkURL: String?

        enum CodingKeys: String, CodingKey {
            case chapters
            case transcriptSegments = "transcript_segments"
            case transcriptText = "transcript_text"
            case inheritedArtworkURL = "inherited_artwork_url"
        }

        func transcript(episodeID: UUID) -> Transcript {
            Transcript(
                episodeID: episodeID,
                language: "en",
                source: .onDevice,
                segments: transcriptSegments.map {
                    Segment(start: $0.start, end: $0.end, text: $0.text)
                }
            )
        }
    }

    private struct GeneratedTranscriptSegment: Decodable {
        let start: Double
        let end: Double
        let text: String
    }

    // MARK: - Speech synthesis → temp file

    private func synthesizeSpeech(text: String, voiceID: String, index: Int) async throws -> URL {
        let tmpURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-tts-\(index)-\(UUID().uuidString).mp3")

        let collectedData: Data
        do {
            collectedData = try await ttsClient.synthesize(
                text: text,
                voiceID: voiceID,
                model: nil
            ).data
        } catch ElevenLabsTTSBackendError.missingAPIKey {
            throw AgentTTSError.notConfigured
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
    /// Waits up to `timeout` seconds for the download to complete, driven by
    /// reactive `@Observable` store updates (no polling) via `awaitState`.
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
            store?.kernelDownload(uuid)
            return nil
        }
        if let url = alreadyReady { return url }

        // Episode not in store at all.
        let episodeExists = await MainActor.run { store?.episode(id: uuid) != nil }
        guard episodeExists else {
            throw AgentTTSError.snippetEpisodeNotFound(episodeID: episodeID)
        }

        Self.logger.info("AgentTTSComposer: waiting for download of snippet episode \(episodeID, privacy: .public)")

        // Wait reactively (no polling) for the download to resolve. The actual
        // observation wait runs on the store's MainActor (see `waitForDownload`).
        let outcome = await waitForDownload(uuid: uuid, episodeID: episodeID, timeout: timeout)
        switch outcome {
        case .success(let url): return url
        case .failure(let error): throw error
        case nil: throw AgentTTSError.snippetDownloadTimeout(episodeID: episodeID)
        }
    }

    /// Suspends on the store's `@Observable` episode state until the download
    /// settles, driven by `AppStateStore.awaitState` — resumes on the next
    /// projection write that flips `downloadState`, with no timer/poll wakeups
    /// in between (false wakeups just re-check the predicate).
    ///
    /// Runs on the `@MainActor` so the `awaitState` predicate can read the
    /// `@MainActor`-isolated store directly. Returns:
    /// - `.success(url)` once `downloadState == .downloaded`,
    /// - `.failure(AgentTTSError.snippetDownloadFailed)` on `.failed`,
    /// - `nil` when the `timeout` deadline elapses first (caller maps to
    ///   `snippetDownloadTimeout`).
    @MainActor
    private func waitForDownload(
        uuid: UUID,
        episodeID: EpisodeID,
        timeout: TimeInterval
    ) async -> Result<URL, Error>? {
        guard let store else { return nil }
        return await store.awaitState(timeout: .seconds(timeout)) {
            guard let episode = store.episode(id: uuid) else { return nil }
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
