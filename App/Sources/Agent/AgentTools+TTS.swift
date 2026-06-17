import Foundation

// MARK: - TTS tool handlers
//
// Implements two agent tools:
//
//   generate_tts_episode  — synthesise a multi-turn episode (speech + snippets)
//                           and publish it to the agent-generated podcast feed.
//   configure_agent_voice — set the agent's default ElevenLabs voice ID so all
//                           future speech turns use it when voice_id is omitted.
//
// Both tools dispatch through `AgentTools.dispatchPodcast` via the standard
// `PodcastAgentToolDeps.ttsPublisher` dep.

extension AgentTools {
    private struct TTSToolPlan: Decodable {
        let error: String?
        let title: String?
        let description: String?
        let playNow: Bool
        let targetPodcastID: String?
        let turns: [TTSToolTurnPlan]

        enum CodingKeys: String, CodingKey {
            case error, title, description, turns
            case playNow = "play_now"
            case targetPodcastID = "target_podcast_id"
        }
    }

    private struct TTSToolTurnPlan: Decodable {
        let kind: String
        let text: String?
        let voiceID: String?
        let episodeID: String?
        let startSeconds: Double?
        let endSeconds: Double?
        let label: String?

        enum CodingKeys: String, CodingKey {
            case kind, text, label
            case voiceID = "voice_id"
            case episodeID = "episode_id"
            case startSeconds = "start_seconds"
            case endSeconds = "end_seconds"
        }
    }

    private struct VoiceConfigurePlan: Decodable {
        let error: String?
        let voiceID: String?

        enum CodingKeys: String, CodingKey {
            case error
            case voiceID = "voice_id"
        }
    }

    private struct TTSPublishPlan: Decodable {
        let shouldPublishToNostr: Bool

        enum CodingKeys: String, CodingKey {
            case shouldPublishToNostr = "should_publish_to_nostr"
        }
    }

    // MARK: - generate_tts_episode

    static func generateTTSEpisodeTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        let ownedPodcasts = await deps.ownedPodcasts.listOwnedPodcasts()
        let ownedPodcastIDs = ownedPodcasts.map(\.podcastID)
        guard let plan = await ttsToolPlan(args: args, ownedPodcastIDs: ownedPodcastIDs) else {
            return toolError("TTS episode planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let title = plan.title else {
            return toolError("TTS episode plan was incomplete")
        }
        let targetPodcastID = plan.targetPodcastID.flatMap(UUID.init(uuidString:))
        if plan.targetPodcastID != nil, targetPodcastID == nil {
            return toolError("TTS episode plan returned an invalid target podcast ID")
        }
        let turns = plan.turns.compactMap(ttsTurn)
        guard turns.count == plan.turns.count else {
            return toolError("TTS episode plan returned an invalid turn")
        }

        // Build the generation source so the player can link back to the
        // originating conversation (Nostr peer or in-app chat).
        let generationSource: Episode.GenerationSource?
        if let ctx = deps.peerContext {
            generationSource = .nostr(
                rootEventID: ctx.rootEventID,
                peerPubkeyHex: ctx.peerPubkeyHex
            )
        } else if let convID = deps.chatConversationID {
            generationSource = .inAppChat(conversationID: convID)
        } else {
            generationSource = nil
        }

        do {
            let result = try await deps.ttsPublisher.generateAndPublish(
                title: title,
                description: plan.description,
                turns: turns,
                playNow: plan.playNow,
                generationSource: generationSource,
                targetPodcastID: targetPodcastID
            )
            if await ttsPublishPlan(
                targetPodcastID: plan.targetPodcastID,
                ownedPodcasts: ownedPodcasts,
                deps: deps
            )?.shouldPublishToNostr == true {
                _ = try? await deps.ownedPodcasts.publishEpisodeToNostr(episodeID: result.episodeID)
            }
            return await ttsToolResult(
                result: result,
                turnCount: turns.count,
                playNow: plan.playNow
            ) ?? toolError("TTS episode result shaping is unavailable")
        } catch {
            return toolError("generate_tts_episode failed: \(error.localizedDescription)")
        }
    }

    // MARK: - configure_agent_voice

    static func configureAgentVoiceTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await voiceConfigurePlan(args: args) else {
            return toolError("Voice configuration planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let voiceID = plan.voiceID else {
            return toolError("Voice configuration plan was incomplete")
        }
        let previousVoiceID = await deps.ttsPublisher.defaultVoiceID()
        await deps.ttsPublisher.setDefaultVoiceID(voiceID)
        return await voiceConfigureResult(voiceID: voiceID, previousVoiceID: previousVoiceID)
            ?? toolError("Voice configuration result shaping is unavailable")
    }

    private static func ttsTurn(_ plan: TTSToolTurnPlan) -> TTSTurn? {
        switch plan.kind {
        case "speech":
            guard let text = plan.text else { return nil }
            return TTSTurn(kind: .speech(text: text, voiceID: plan.voiceID))
        case "snippet":
            guard let episodeID = plan.episodeID,
                  let start = plan.startSeconds,
                  let end = plan.endSeconds
            else { return nil }
            return TTSTurn(kind: .snippet(
                episodeID: episodeID,
                startSeconds: start,
                endSeconds: end,
                label: plan.label
            ))
        default:
            return nil
        }
    }

    private static func ttsToolPlan(args: [String: Any], ownedPodcastIDs: [String]) async -> TTSToolPlan? {
        guard let envelope = await ttsToolFFI(
            payload: [
                "args": args,
                "owned_podcast_ids": ownedPodcastIDs,
            ],
            op: "plan"
        ),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(TTSToolPlan.self, from: data)
    }

    private static func ttsPublishPlan(
        targetPodcastID: String?,
        ownedPodcasts: [AgentOwnedPodcastInfo],
        deps: PodcastAgentToolDeps
    ) async -> TTSPublishPlan? {
        let rows = ownedPodcasts.map { podcast -> [String: Any] in
            [
                "podcast_id": podcast.podcastID,
                "visibility": podcast.visibility,
            ]
        }
        var payload: [String: Any] = [
            "target_podcast_id": targetPodcastID ?? "",
            "owned_podcasts": rows,
        ]
        guard let envelope = await actionTool(op: "tts_publish_plan", payload: payload),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(TTSPublishPlan.self, from: data)
    }

    private static func ttsToolResult(
        result: TTSEpisodeResult,
        turnCount: Int,
        playNow: Bool
    ) async -> String? {
        var payload: [String: Any] = [
            "episode_id": result.episodeID,
            "podcast_id": result.podcastID,
            "title": result.title,
            "published_to_library": result.publishedToLibrary,
            "turn_count": turnCount,
            "play_now": playNow,
        ]
        if let duration = result.durationSeconds { payload["duration_seconds"] = duration }
        return await ttsToolFFI(payload: payload, op: "result")
    }

    private static func voiceConfigurePlan(args: [String: Any]) async -> VoiceConfigurePlan? {
        guard let envelope = await ttsToolFFI(payload: ["args": args], op: "voice_plan"),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(VoiceConfigurePlan.self, from: data)
    }

    private static func voiceConfigureResult(voiceID: String, previousVoiceID: String?) async -> String? {
        var payload: [String: Any] = ["voice_id": voiceID]
        if let previousVoiceID { payload["previous_voice_id"] = previousVoiceID }
        return await ttsToolFFI(payload: payload, op: "voice_result")
    }

    private static func ttsToolFFI(payload: [String: Any], op: String) async -> String? {
        let handleBits = await MainActor.run {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }
        guard let handleBits,
              let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return nil
            }
            return json.withCString { ptr in
                let result: UnsafeMutablePointer<CChar>?
                switch op {
                case "plan":
                    result = nmp_app_podcast_agent_tts_tool_plan(handle, ptr)
                case "result":
                    result = nmp_app_podcast_agent_tts_tool_result(handle, ptr)
                case "voice_plan":
                    result = nmp_app_podcast_agent_voice_configure_plan(handle, ptr)
                case "voice_result":
                    result = nmp_app_podcast_agent_voice_configure_result(handle, ptr)
                default:
                    result = nil
                }
                guard let result else { return nil }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
    }
}
