import Foundation

// MARK: - Agent-owned podcast tool handlers
//
// Implements four tools backed by `PodcastAgentToolDeps.ownedPodcasts`:
//
//   create_podcast           — create a new agent-owned show
//   update_podcast           — update metadata on an existing agent-owned show
//   delete_my_podcast        — delete an agent-owned show and all its episodes
//   list_my_podcasts         — list all agent-owned shows
//   generate_podcast_artwork — generate artwork via image-gen + Blossom upload

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

        do {
            let info = try await deps.ownedPodcasts.createPodcast(
                title: title,
                description: description,
                author: author,
                imageURL: imageURL,
                language: language,
                categories: categories
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

        do {
            let info = try await deps.ownedPodcasts.updatePodcast(
                podcastID: podcastID,
                title: title,
                description: description,
                author: author,
                imageURL: imageURL
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
            "episode_count": info.episodeCount,
        ]
        if let url = info.imageURL { row["image_url"] = url.absoluteString }
        return row
    }
}
