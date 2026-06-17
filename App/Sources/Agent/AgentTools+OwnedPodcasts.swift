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
    private struct OwnedPodcastCreatePlan: Decodable {
        let error: String?
        let title: String?
        let description: String?
        let author: String?
        let language: String?
        let categories: [String]?
        let imageURL: String?
        let visibility: String?

        enum CodingKeys: String, CodingKey {
            case error, title, description, author, language, categories, visibility
            case imageURL = "image_url"
        }
    }

    private struct OwnedPodcastUpdatePlan: Decodable {
        let error: String?
        let podcastID: String?
        let title: String?
        let description: String?
        let author: String?
        let imageURL: String?
        let visibility: String?

        enum CodingKeys: String, CodingKey {
            case error, title, description, author, visibility
            case podcastID = "podcast_id"
            case imageURL = "image_url"
        }
    }

    private struct OwnedPodcastIDPlan: Decodable {
        let error: String?
        let podcastID: String?
        let episodeID: String?
        let prompt: String?

        enum CodingKeys: String, CodingKey {
            case error, prompt
            case podcastID = "podcast_id"
            case episodeID = "episode_id"
        }
    }

    // MARK: - create_podcast

    static func createPodcastTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await ownedPodcastPlan(OwnedPodcastCreatePlan.self, op: "create_plan", args: args) else {
            return toolError("create_podcast planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let title = plan.title else { return toolError("create_podcast plan was incomplete") }
        let visibility = Podcast.NostrVisibility(rawValue: plan.visibility ?? "private") ?? .private

        do {
            let info = try await deps.ownedPodcasts.createPodcast(
                title: title,
                description: plan.description ?? "",
                author: plan.author ?? "",
                imageURL: plan.imageURL.flatMap(URL.init(string:)),
                language: plan.language,
                categories: plan.categories ?? [],
                visibility: visibility
            )
            return await ownedPodcastResult(op: "info_result", payload: ["podcast": rawPodcastInfo(info)])
                ?? toolError("create_podcast result shaping is unavailable")
        } catch {
            return toolError("create_podcast failed: \(error.localizedDescription)")
        }
    }

    // MARK: - update_podcast

    static func updatePodcastTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await ownedPodcastPlan(OwnedPodcastUpdatePlan.self, op: "update_plan", args: args) else {
            return toolError("update_podcast planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let podcastID = plan.podcastID else { return toolError("update_podcast plan was incomplete") }

        do {
            let info = try await deps.ownedPodcasts.updatePodcast(
                podcastID: podcastID,
                title: plan.title,
                description: plan.description,
                author: plan.author,
                imageURL: plan.imageURL.flatMap(URL.init(string:)),
                visibility: plan.visibility.flatMap(Podcast.NostrVisibility.init(rawValue:))
            )
            return await ownedPodcastResult(op: "info_result", payload: ["podcast": rawPodcastInfo(info)])
                ?? toolError("update_podcast result shaping is unavailable")
        } catch {
            return toolError("update_podcast failed: \(error.localizedDescription)")
        }
    }

    // MARK: - delete_my_podcast

    static func deleteMyPodcastTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await ownedPodcastPlan(OwnedPodcastIDPlan.self, op: "delete_plan", args: args) else {
            return toolError("delete_my_podcast planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let podcastID = plan.podcastID else { return toolError("delete_my_podcast plan was incomplete") }
        do {
            try await deps.ownedPodcasts.deletePodcast(podcastID: podcastID)
            return await ownedPodcastResult(op: "delete_result", payload: ["podcast_id": podcastID])
                ?? toolError("delete_my_podcast result shaping is unavailable")
        } catch {
            return toolError("delete_my_podcast failed: \(error.localizedDescription)")
        }
    }

    // MARK: - list_my_podcasts

    static func listMyPodcastsTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        let podcasts = await deps.ownedPodcasts.listOwnedPodcasts()
        return await ownedPodcastResult(
            op: "list_result",
            payload: ["podcasts": podcasts.map(rawPodcastInfo)]
        ) ?? toolError("list_my_podcasts result shaping is unavailable")
    }

    // MARK: - publish_episode

    static func publishEpisodeTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await ownedPodcastPlan(OwnedPodcastIDPlan.self, op: "publish_plan", args: args) else {
            return toolError("publish_episode planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let episodeID = plan.episodeID else { return toolError("publish_episode plan was incomplete") }
        do {
            var payload: [String: Any] = ["episode_id": episodeID]
            if let naddr = try await deps.ownedPodcasts.publishEpisodeToNostr(episodeID: episodeID) {
                payload["naddr"] = naddr
            }
            return await ownedPodcastResult(op: "publish_result", payload: payload)
                ?? toolError("publish_episode result shaping is unavailable")
        } catch {
            return toolError("publish_episode failed: \(error.localizedDescription)")
        }
    }

    // MARK: - generate_podcast_artwork

    static func generatePodcastArtworkTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let plan = await ownedPodcastPlan(OwnedPodcastIDPlan.self, op: "artwork_plan", args: args) else {
            return toolError("generate_podcast_artwork planning is unavailable")
        }
        if let error = plan.error { return toolError(error) }
        guard let prompt = plan.prompt else { return toolError("generate_podcast_artwork plan was incomplete") }
        do {
            let url = try await deps.ownedPodcasts.generateAndUploadArtwork(prompt: prompt)
            return await ownedPodcastResult(
                op: "artwork_result",
                payload: ["image_url": url.absoluteString, "prompt": prompt]
            ) ?? toolError("generate_podcast_artwork result shaping is unavailable")
        } catch {
            return toolError("generate_podcast_artwork failed: \(error.localizedDescription)")
        }
    }

    // MARK: - Shared helper

    private static func rawPodcastInfo(_ info: AgentOwnedPodcastInfo) -> [String: Any] {
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

    private static func ownedPodcastPlan<T: Decodable>(
        _ type: T.Type,
        op: String,
        args: [String: Any]
    ) async -> T? {
        guard let envelope = await ownedPodcastTool(op: op, payload: ["args": args]),
              let data = envelope.data(using: .utf8)
        else { return nil }
        return try? JSONDecoder().decode(T.self, from: data)
    }

    private static func ownedPodcastResult(op: String, payload: [String: Any]) async -> String? {
        await ownedPodcastTool(op: op, payload: payload)
    }

    private static func ownedPodcastTool(op: String, payload: [String: Any]) async -> String? {
        let handleBits = await MainActor.run {
            KernelModel.shared?.podcastHandlePointer.map { Int(bitPattern: $0) }
        }
        guard let handleBits else { return nil }
        var request = payload
        request["op"] = op
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return await Task.detached(priority: .userInitiated) {
            guard let handle = UnsafeMutableRawPointer(bitPattern: handleBits) else {
                return nil
            }
            return json.withCString { ptr in
                guard let result = nmp_app_podcast_agent_owned_podcast_tool(handle, ptr) else {
                    return nil
                }
                defer { nmp_free_string(result) }
                return String(cString: result)
            }
        }.value
    }
}
