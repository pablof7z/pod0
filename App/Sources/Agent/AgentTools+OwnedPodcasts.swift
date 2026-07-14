import Foundation

// MARK: - Agent-owned podcast tool handlers
//
// Implements five tools backed by `PodcastAgentToolDeps.ownedPodcasts`:
//
//   create_podcast           — create a new agent-owned show
//   update_podcast           — update metadata on an existing agent-owned show
//   delete_my_podcast        — delete an agent-owned show and all its episodes
//   list_my_podcasts         — list all agent-owned shows
//   generate_podcast_artwork — generate artwork via image-gen + Blossom upload
//
// `generate_tts_episode` in AgentTools+TTS.swift accepts an optional
// `podcast_id` parameter; if supplied and the podcast is public and Nostr is
// enabled, the episode is published to Nostr via `publishEpisodeToNostr`.

extension AgentTools {

    // MARK: - create_podcast

    static func createPodcastTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let title = (args["title"] as? String)?.trimmed, !title.isEmpty else {
            return toolError("Missing or empty 'title'")
        }
        let description = (args["description"] as? String)?.trimmed ?? ""
        let author      = (args["author"] as? String)?.trimmed ?? ""
        let language    = (args["language"] as? String)?.trimmed.nilIfEmpty
        let categories  = (args["categories"] as? [String]) ?? []
        let imageURLStr = (args["image_url"] as? String)?.trimmed.nilIfEmpty
        let imageURL    = imageURLStr.flatMap { URL(string: $0) }
        let visibilityRaw = (args["visibility"] as? String)?.trimmed ?? "private"
        let visibility  = Podcast.NostrVisibility(rawValue: visibilityRaw) ?? .private

        do {
            let info = try await deps.ownedPodcasts.createPodcast(
                title: title,
                description: description,
                author: author,
                imageURL: imageURL,
                language: language,
                categories: categories,
                visibility: visibility
            )
            return toolSuccess(podcastInfoPayload(info))
        } catch {
            return toolError("create_podcast failed: \(error.localizedDescription)")
        }
    }

    // MARK: - update_podcast

    static func updatePodcastTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let podcastID = (args["podcast_id"] as? String)?.trimmed, !podcastID.isEmpty else {
            return toolError("Missing or empty 'podcast_id'")
        }
        let title       = (args["title"] as? String)?.trimmed.nilIfEmpty
        let description = (args["description"] as? String)?.trimmed.nilIfEmpty
        let author      = (args["author"] as? String)?.trimmed.nilIfEmpty
        let imageURLStr = (args["image_url"] as? String)?.trimmed.nilIfEmpty
        let imageURL    = imageURLStr.flatMap { URL(string: $0) }
        let visibilityRaw = (args["visibility"] as? String)?.trimmed.nilIfEmpty
        let visibility  = visibilityRaw.flatMap { Podcast.NostrVisibility(rawValue: $0) }

        do {
            let info = try await deps.ownedPodcasts.updatePodcast(
                podcastID: podcastID,
                title: title,
                description: description,
                author: author,
                imageURL: imageURL,
                visibility: visibility
            )
            return toolSuccess(podcastInfoPayload(info))
        } catch {
            return toolError("update_podcast failed: \(error.localizedDescription)")
        }
    }

    // MARK: - delete_my_podcast

    static func deleteMyPodcastTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let podcastID = (args["podcast_id"] as? String)?.trimmed, !podcastID.isEmpty else {
            return toolError("Missing or empty 'podcast_id'")
        }
        do {
            try await deps.ownedPodcasts.deletePodcast(podcastID: podcastID)
            return toolSuccess(["podcast_id": podcastID, "deleted": true])
        } catch {
            return toolError("delete_my_podcast failed: \(error.localizedDescription)")
        }
    }

    // MARK: - list_my_podcasts

    static func listMyPodcastsTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        let podcasts = await deps.ownedPodcasts.listOwnedPodcasts()
        let rows = podcasts.map { podcastInfoPayload($0) }
        return toolSuccess(["count": rows.count, "podcasts": rows])
    }

    // MARK: - publish_episode

    static func publishEpisodeTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let episodeID = (args["episode_id"] as? String)?.trimmed, !episodeID.isEmpty else {
            return toolError("Missing or empty 'episode_id'")
        }
        do {
            guard let naddr = try await deps.ownedPodcasts.publishEpisodeToNostr(episodeID: episodeID) else {
                return toolError("Episode '\(episodeID)' was not published — verify the podcast is agent-owned, its visibility is 'public', and Nostr is enabled in Settings.")
            }
            return toolSuccess(["episode_id": episodeID, "naddr": naddr])
        } catch {
            return toolError("publish_episode failed: \(error.localizedDescription)")
        }
    }

    // MARK: - generate_podcast_artwork

    static func generatePodcastArtworkTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let prompt = (args["prompt"] as? String)?.trimmed, !prompt.isEmpty else {
            return toolError("Missing or empty 'prompt'")
        }
        do {
            let url = try await deps.ownedPodcasts.generateAndUploadArtwork(prompt: prompt)
            return toolSuccess(["image_url": url.absoluteString, "prompt": prompt])
        } catch {
            return toolError("generate_podcast_artwork failed: \(error.localizedDescription)")
        }
    }

    // MARK: - Shared helper

    private static func podcastInfoPayload(_ info: AgentOwnedPodcastInfo) -> [String: Any] {
        var row: [String: Any] = [
            "podcast_id":    info.podcastID,
            "title":         info.title,
            "description":   info.description,
            "author":        info.author,
            "visibility":    info.visibility,
            "episode_count": info.episodeCount,
        ]
        if let url = info.imageURL { row["image_url"] = url.absoluteString }
        if let eventID = info.nostrEventID { row["nostr_event_id"] = eventID }
        if let naddr = info.nostrAddr { row["naddr"] = naddr }
        if let count = info.episodesPublishedToNostr { row["episodes_published_to_nostr"] = count }
        return row
    }
}
