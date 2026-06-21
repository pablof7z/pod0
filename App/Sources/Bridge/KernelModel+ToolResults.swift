import Foundation

// MARK: - KernelModel tool-result types and FFI call wrappers
//
// Decodable result types for agent-facing tool calls, plus the FFI wrappers
// that serialise Swift arguments → JSON → C string and decode the response.
// Extracted from KernelModel.swift to keep that file under the AGENTS.md
// 500-line hard limit.

extension KernelModel {

    // ── Result types ────────────────────────────────────────────────────────

    struct TranscriptIngestPlan: Decodable {
        var ok: Bool
        var status: String
        var reason: String?
        var publisherUrl: String?
        var mimeHint: String?
        var provider: String?
        var requiresLocalFile: Bool
    }

    private struct TranscriptAutoIngestCandidates: Decodable {
        var ok: Bool
        var episodeIds: [String]
    }

    struct TranscriptToolResult: Decodable {
        var ok: Bool
        var status: String
        var source: String?
        var message: String?
    }

    struct EpisodeMutationToolResult: Decodable {
        var ok: Bool
        var episodeId: String
        var podcastId: String?
        var episodeTitle: String
        var podcastTitle: String?
        var state: String
        var message: String?
    }

    struct PlaybackToolResult: Decodable {
        var ok: Bool
        var episodeId: String
        var queuePosition: String
        var startedPlaying: Bool
        var episodeTitle: String?
        var podcastTitle: String?
        var durationSeconds: Int?
        var message: String?
    }

    struct NowPlayingToolResult: Decodable {
        var ok: Bool
        var episodeId: String?
        var episodeTitle: String?
        var podcastId: String?
        var podcastTitle: String?
        var positionSeconds: Double
        var durationSeconds: Double?
        var isPlaying: Bool
        var rate: Double
        var message: String?
    }

    struct ExternalPlayPlan: Decodable {
        var ok: Bool
        var podcastId: String
        var shouldCreatePlaceholder: Bool
        var shouldHydrateMetadata: Bool
        var feedUrl: String?
        var placeholderTitle: String?
        var visibility: String?
        var titleIsPlaceholder: Bool
        var reason: String?
    }

    struct AgentAskPending: Decodable, Equatable {
        var id: String
        var question: String
        var context: String?
        var createdAt: Int64
        var timeoutSeconds: UInt64
    }

    struct AgentAskResponse: Decodable {
        var ok: Bool
        var current: AgentAskPending?
        var enqueued: AgentAskPending?
        var settledId: String?
        var result: String?
        var message: String?
    }

    struct RememberTextMemoryResponse: Decodable {
        var ok: Bool
        var id: String?
        var key: String?
        var value: String?
        var source: String?
        var message: String?
    }

    // ── Transcript tools ────────────────────────────────────────────────────

    /// Ask Rust what the transcript-ingest pipeline should do next.
    ///
    /// Swift supplies raw host capability facts (`localAudioAvailable`) and then
    /// executes the returned capability branch. Rust owns the policy decision.
    func transcriptIngestPlan(
        episodeID: UUID,
        forceProvider: STTProvider?,
        localAudioAvailable: Bool,
        allowPublisher: Bool,
        autoIngest: Bool = false
    ) -> TranscriptIngestPlan? {
        guard let handle = kernel.podcastHandle else { return nil }
        var payload: [String: Any] = [
            "episode_id": episodeID.uuidString,
            "local_audio_available": localAudioAvailable,
            "allow_publisher": allowPublisher,
            "auto_ingest": autoIngest,
        ]
        if let forceProvider {
            payload["force_provider"] = forceProvider.rawValue
        }
        guard let json = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return nil }
        return jsonStr.withCString { ptr -> TranscriptIngestPlan? in
            guard let result = nmp_app_podcast_transcript_ingest_plan(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            let response = String(cString: result)
            guard let data = response.data(using: .utf8) else { return nil }
            return try? KernelDecoding.makeDecoder().decode(TranscriptIngestPlan.self, from: data)
        }
    }

    /// Ask Rust which episodes should be auto-ingested next.
    ///
    /// Swift supplies native file-availability facts; Rust owns eligibility,
    /// newest-first ordering, optional new-episode scoping, and max count.
    func transcriptAutoIngestCandidates(
        maxCount: Int,
        episodeIDs: [UUID]? = nil,
        localAudioAvailable: [UUID: Bool]
    ) -> [UUID] {
        guard let handle = kernel.podcastHandle else { return [] }
        var payload: [String: Any] = [
            "max_count": maxCount,
            "local_audio_available": localAudioAvailable.map { id, available in
                [
                    "episode_id": id.uuidString,
                    "available": available,
                ] as [String: Any]
            },
        ]
        if let episodeIDs {
            payload["episode_ids"] = episodeIDs.map(\.uuidString)
        }
        guard let json = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return [] }
        return jsonStr.withCString { ptr -> [UUID] in
            guard let result = nmp_app_podcast_transcript_auto_ingest_candidates(handle, ptr) else {
                return []
            }
            defer { nmp_free_string(result) }
            let response = String(cString: result)
            guard let data = response.data(using: .utf8),
                  let decoded = try? KernelDecoding.makeDecoder().decode(TranscriptAutoIngestCandidates.self, from: data),
                  decoded.ok
            else { return [] }
            return decoded.episodeIds.compactMap(UUID.init(uuidString:))
        }
    }

    /// Ask Rust how an agent-facing transcript tool result should be reported
    /// for the current episode state.
    func transcriptToolResult(episodeID: UUID) -> TranscriptToolResult? {
        guard let handle = kernel.podcastHandle else { return nil }
        let payload: [String: Any] = ["episode_id": episodeID.uuidString]
        guard let json = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return nil }
        return jsonStr.withCString { ptr -> TranscriptToolResult? in
            guard let result = nmp_app_podcast_transcript_tool_result(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            let response = String(cString: result)
            guard let data = response.data(using: .utf8) else { return nil }
            return try? KernelDecoding.makeDecoder().decode(TranscriptToolResult.self, from: data)
        }
    }

    // ── Episode / playback tools ────────────────────────────────────────────

    /// Ask Rust to build the agent-facing result envelope for an episode
    /// mutation tool after the mutation action has been accepted.
    func episodeMutationToolResult(episodeID: String, state: String) -> EpisodeMutationToolResult? {
        guard let handle = kernel.podcastHandle else { return nil }
        let payload: [String: Any] = [
            "episode_id": episodeID,
            "state": state
        ]
        guard let json = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return nil }
        return jsonStr.withCString { ptr -> EpisodeMutationToolResult? in
            guard let result = nmp_app_podcast_episode_mutation_tool_result(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            let response = String(cString: result)
            guard let data = response.data(using: .utf8) else { return nil }
            return try? KernelDecoding.makeDecoder().decode(EpisodeMutationToolResult.self, from: data)
        }
    }

    func playbackToolResult(
        episodeID: String,
        queuePosition: QueuePosition,
        startedPlaying: Bool
    ) -> PlaybackToolResult? {
        guard let handle = kernel.podcastHandle else { return nil }
        let payload: [String: Any] = [
            "episode_id": episodeID,
            "queue_position": queuePosition.rawValue,
            "started_playing": startedPlaying
        ]
        guard let json = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return nil }
        return jsonStr.withCString { ptr -> PlaybackToolResult? in
            guard let result = nmp_app_podcast_playback_tool_result(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            let response = String(cString: result)
            guard let data = response.data(using: .utf8) else { return nil }
            return try? KernelDecoding.makeDecoder().decode(PlaybackToolResult.self, from: data)
        }
    }

    func nowPlayingToolResult() -> NowPlayingToolResult? {
        guard let handle = kernel.podcastHandle else { return nil }
        guard let result = nmp_app_podcast_now_playing_tool_result(handle) else {
            return nil
        }
        defer { nmp_free_string(result) }
        let response = String(cString: result)
        guard let data = response.data(using: .utf8) else { return nil }
        return try? KernelDecoding.makeDecoder().decode(NowPlayingToolResult.self, from: data)
    }

    func externalPlayPlan(feedURLString: String?) -> ExternalPlayPlan? {
        guard let handle = kernel.podcastHandle else { return nil }
        var payload: [String: Any] = [:]
        if let feedURLString {
            payload["feed_url"] = feedURLString
        }
        guard let json = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return nil }
        return jsonStr.withCString { ptr -> ExternalPlayPlan? in
            guard let result = nmp_app_podcast_external_play_plan(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            let response = String(cString: result)
            guard let data = response.data(using: .utf8) else { return nil }
            return try? KernelDecoding.makeDecoder().decode(ExternalPlayPlan.self, from: data)
        }
    }

    // ── Agent-ask tools ─────────────────────────────────────────────────────

    func agentAskEnqueue(question: String, context: String?) -> AgentAskResponse? {
        guard let handle = kernel.podcastHandle else { return nil }
        var payload: [String: Any] = ["question": question]
        if let context {
            payload["context"] = context
        }
        guard let json = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return nil }
        return jsonStr.withCString { ptr -> AgentAskResponse? in
            guard let result = nmp_app_podcast_agent_ask_enqueue(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            let response = String(cString: result)
            guard let data = response.data(using: .utf8) else { return nil }
            return try? KernelDecoding.makeDecoder().decode(AgentAskResponse.self, from: data)
        }
    }

    func agentAskSettle(id: String, outcome: String, answer: String? = nil) -> AgentAskResponse? {
        guard let handle = kernel.podcastHandle else { return nil }
        var payload: [String: Any] = [
            "id": id,
            "outcome": outcome
        ]
        if let answer {
            payload["answer"] = answer
        }
        guard let json = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return nil }
        return jsonStr.withCString { ptr -> AgentAskResponse? in
            guard let result = nmp_app_podcast_agent_ask_settle(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            let response = String(cString: result)
            guard let data = response.data(using: .utf8) else { return nil }
            return try? KernelDecoding.makeDecoder().decode(AgentAskResponse.self, from: data)
        }
    }

    func rememberTextMemory(content: String, source: String = "agent") -> RememberTextMemoryResponse? {
        guard let handle = kernel.podcastHandle else { return nil }
        let payload: [String: Any] = [
            "content": content,
            "source": source
        ]
        guard let json = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return nil }
        let response = jsonStr.withCString { ptr -> RememberTextMemoryResponse? in
            guard let result = nmp_app_podcast_memory_remember_text(handle, ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            let response = String(cString: result)
            guard let data = response.data(using: .utf8) else { return nil }
            return try? KernelDecoding.makeDecoder().decode(RememberTextMemoryResponse.self, from: data)
        }
        pullPodcastSnapshotIfChanged()
        return response
    }

    // ── Episode pipeline event log (Diagnostics) ────────────────────────────

    /// Report a completed transcript to the Rust kernel so AI features
    /// can access it without going through Swift's TranscriptStore.
    ///
    /// Slice 5a: sends the full timed segment list as `"entries"` so the
    /// kernel's `index_episode` can produce RAG chunks with real
    /// `start_secs` / `end_secs` (enables seek-to-timestamp in search).
    /// `source` names the service (e.g. "ElevenLabs Scribe"); the kernel
    /// surfaces it on the `transcript.ready` Diagnostics event.
    func sendTranscriptReport(episodeID: UUID, transcript: Transcript, source: String? = nil) {
        guard let handle = kernel.podcastHandle else { return }

        // Build the timed-entries payload (slice 5a).  Each segment maps to a
        // Rust `TimedEntryPayload` { start_secs, end_secs, text, speaker? }.
        // Speaker labels are resolved via the Transcript's speaker lookup so
        // the kernel can surface them in future transcript-search UI.
        let entries: [[String: Any]] = transcript.segments.map { seg in
            var entry: [String: Any] = [
                "start_secs": seg.start,
                "end_secs": seg.end,
                "text": seg.text
            ]
            if let speakerID = seg.speakerID,
               let speaker = transcript.speaker(for: speakerID) {
                entry["speaker"] = speaker.label
            }
            return entry
        }

        var payload: [String: Any] = [
            "episode_id": episodeID.uuidString,
            "entries": entries
        ]
        if let source { payload["source"] = source }
        guard let json = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return }
        jsonStr.withCString { ptr in
            let result = nmp_app_podcast_transcript_report(handle, ptr)
            if let result { nmp_free_string(result) }
        }
    }

    /// Record one host-authored pipeline event onto an episode's Diagnostics
    /// log via the generic record-event FFI. Used for stages that run in the
    /// iOS capability layer and carry detail the kernel can't see — STT with a
    /// named provider, RAG indexing outcome, clip export/share. Fire-and-forget.
    func recordEpisodeEvent(
        episodeID: UUID,
        kind: String,
        severity: String,
        summary: String,
        details: [(String, String)] = []
    ) {
        guard let handle = kernel.podcastHandle else { return }
        let detailObjs = details.map { ["label": $0.0, "value": $0.1] }
        let payload: [String: Any] = [
            "episode_id": episodeID.uuidString,
            "kind": kind,
            "severity": severity,
            "summary": summary,
            "details": detailObjs
        ]
        guard let json = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: json, encoding: .utf8)
        else { return }
        jsonStr.withCString { ptr in
            let result = nmp_app_podcast_record_episode_event(handle, ptr)
            if let result { nmp_free_string(result) }
        }
    }

    /// Fetch the kernel's per-episode pipeline event log (download / transcript
    /// / identify lifecycle). A small, synchronous single-episode FFI read —
    /// the events deliberately do NOT ride the library snapshot, so the
    /// Diagnostics sheet pulls them lazily on appear and on the snapshot
    /// generation changes it already observes. Returns `[]` when the kernel is
    /// unregistered, the episode has no log, or the payload fails to decode.
    func fetchEpisodeEvents(episodeID: UUID) -> [EpisodeAuditEvent] {
        guard let handle = kernel.podcastHandle else { return [] }
        return episodeID.uuidString.withCString { ptr -> [EpisodeAuditEvent] in
            guard let result = nmp_app_podcast_episode_events(handle, ptr) else { return [] }
            defer { nmp_free_string(result) }
            guard let data = String(cString: result).data(using: .utf8) else { return [] }
            let decoder = JSONDecoder()
            decoder.dateDecodingStrategy = .iso8601
            return (try? decoder.decode([EpisodeAuditEvent].self, from: data)) ?? []
        }
    }
}
