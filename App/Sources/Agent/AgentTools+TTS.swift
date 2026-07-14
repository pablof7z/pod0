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

    // MARK: - generate_tts_episode

    static func generateTTSEpisodeTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let title = (args["title"] as? String)?.trimmed, !title.isEmpty else {
            return toolError("Missing or empty 'title'")
        }
        guard let rawTurns = args["turns"] as? [[String: Any]], !rawTurns.isEmpty else {
            return toolError("'turns' must be a non-empty array")
        }
        let description = (args["description"] as? String)?.trimmed.nilIfEmpty
        let playNow = args["play_now"] as? Bool ?? false

        // Validate optional podcast_id against owned shows.
        let targetPodcastID: UUID?
        if let rawPodcastID = (args["podcast_id"] as? String)?.trimmed.nilIfEmpty {
            guard let uuid = UUID(uuidString: rawPodcastID) else {
                return toolError("podcast_id '\(rawPodcastID)' is not a valid UUID")
            }
            let owned = await deps.ownedPodcasts.listOwnedPodcasts()
            guard owned.contains(where: { $0.podcastID == rawPodcastID }) else {
                return toolError("podcast_id '\(rawPodcastID)' is not an agent-owned podcast — use list_my_podcasts to find valid IDs")
            }
            targetPodcastID = uuid
        } else {
            targetPodcastID = nil
        }

        // Parse turns
        var turns: [TTSTurn] = []
        for (i, raw) in rawTurns.enumerated() {
            guard let kind = (raw["kind"] as? String)?.trimmed.lowercased() else {
                return toolError("Turn \(i): missing 'kind' (speech | snippet)")
            }
            switch kind {
            case "speech":
                guard let text = (raw["text"] as? String)?.trimmed, !text.isEmpty else {
                    return toolError("Turn \(i): speech turn requires non-empty 'text'")
                }
                let voiceID = (raw["voice_id"] as? String)?.trimmed.nilIfEmpty
                turns.append(TTSTurn(kind: .speech(text: text, voiceID: voiceID)))

            case "snippet":
                guard let episodeID = (raw["episode_id"] as? String)?.trimmed, !episodeID.isEmpty else {
                    return toolError("Turn \(i): snippet turn requires 'episode_id'")
                }
                guard let start = numericArg(raw["start_seconds"]),
                      let end = numericArg(raw["end_seconds"]),
                      end > start else {
                    return toolError("Turn \(i): snippet turn requires valid 'start_seconds' < 'end_seconds'")
                }
                let label = (raw["label"] as? String)?.trimmed.nilIfEmpty
                turns.append(TTSTurn(kind: .snippet(
                    episodeID: episodeID,
                    startSeconds: start,
                    endSeconds: end,
                    label: label
                )))

            default:
                return toolError("Turn \(i): unknown kind '\(kind)' — must be 'speech' or 'snippet'")
            }
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
                description: description,
                turns: turns,
                playNow: playNow,
                generationSource: generationSource,
                targetPodcastID: targetPodcastID
            )
            // Publish to Nostr when the target is an owned public podcast.
            if targetPodcastID != nil {
                _ = try? await deps.ownedPodcasts.publishEpisodeToNostr(episodeID: result.episodeID)
            }
            var payload: [String: Any] = [
                "episode_id": result.episodeID,
                "podcast_id": result.podcastID,
                "title": result.title,
                "published_to_library": result.publishedToLibrary,
                "turn_count": turns.count,
            ]
            if let dur = result.durationSeconds {
                payload["duration_seconds"] = Int(dur)
            }
            if playNow { payload["play_now"] = true }
            return toolSuccess(payload)
        } catch {
            return toolError("generate_tts_episode failed: \(error.localizedDescription)")
        }
    }

    // MARK: - configure_agent_voice

    static func configureAgentVoiceTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let voiceID = (args["voice_id"] as? String)?.trimmed, !voiceID.isEmpty else {
            return toolError("Missing or empty 'voice_id'")
        }
        let previousVoiceID = deps.ttsPublisher.defaultVoiceID()
        deps.ttsPublisher.setDefaultVoiceID(voiceID)
        return toolSuccess([
            "voice_id": voiceID,
            "previous_voice_id": previousVoiceID,
        ])
    }
}
