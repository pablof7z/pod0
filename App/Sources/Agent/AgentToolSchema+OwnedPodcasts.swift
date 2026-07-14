import Foundation

// MARK: - Owned podcast tool schema (OpenAI tool format)
//
// Defines the OpenAI-compatible JSON-schema entries for the five agent tools
// that manage agent-owned shows (create, update, delete, list, artwork).
// Split from AgentToolSchema+Podcast.swift to keep that file under 500 lines.
//
// `podcastSchema` in AgentToolSchema+Podcast.swift concatenates this array
// so callers see a single merged schema surface.

extension AgentTools {

    @MainActor
    static var ownedPodcastSchema: [[String: Any]] {
        [
            ownedPodcastTool(
                name: PodcastNames.createPodcast,
                description: """
                Create a new agent-owned podcast show. The show is private by default \
                (stored in the user's library only). Set visibility to 'public' to publish \
                a NIP-74 kind:30074 Nostr event signed by the agent's key. \
                Before creating, consider calling generate_podcast_artwork to produce cover art \
                and pass the resulting image_url here. Returns the stable podcast_id for use \
                in generate_tts_episode and update_podcast.
                """,
                properties: [
                    "title": ["type": "string", "description": "Show title."],
                    "description": ["type": "string", "description": "One-paragraph show summary shown on the detail page and in the Nostr event."],
                    "author": ["type": "string", "description": "Author or host name."],
                    "image_url": ["type": "string", "description": "Cover art URL (use generate_podcast_artwork to produce one). Optional but strongly recommended for public shows."],
                    "language": ["type": "string", "description": "BCP-47 language tag, e.g. 'en', 'es'. Optional."],
                    "categories": ["type": "array", "items": ["type": "string"], "description": "Topic tags / genre strings. Optional."],
                    "visibility": ["type": "string", "enum": ["private", "public"], "description": "Whether to publish a NIP-74 Nostr event. Defaults to 'private'."],
                ],
                required: ["title"]
            ),
            ownedPodcastTool(
                name: PodcastNames.updatePodcast,
                description: """
                Update metadata on an agent-owned podcast. Omit any field to leave it unchanged. \
                If the podcast is public and Nostr is enabled, the updated kind:30074 event is \
                re-published. Use this to set cover art after calling generate_podcast_artwork, \
                rename the show, or change visibility.
                """,
                properties: [
                    "podcast_id": ["type": "string", "description": "The podcast UUID returned by create_podcast or list_my_podcasts."],
                    "title": ["type": "string", "description": "New show title. Omit to keep current value."],
                    "description": ["type": "string", "description": "New show summary. Omit to keep current value."],
                    "author": ["type": "string", "description": "New author / host name. Omit to keep current value."],
                    "image_url": ["type": "string", "description": "New cover art URL. Omit to keep current value."],
                    "visibility": ["type": "string", "enum": ["private", "public"], "description": "New visibility. Omit to keep current value."],
                ],
                required: ["podcast_id"]
            ),
            ownedPodcastTool(
                name: PodcastNames.deleteMyPodcast,
                description: """
                Permanently delete an agent-owned podcast and all its episodes. \
                Cannot be undone. Only works on agent-owned shows (those created with create_podcast); \
                use delete_podcast for subscribed external feeds.
                """,
                properties: [
                    "podcast_id": ["type": "string", "description": "The podcast UUID to delete."],
                ],
                required: ["podcast_id"]
            ),
            ownedPodcastTool(
                name: PodcastNames.listMyPodcasts,
                description: """
                List all agent-owned podcasts (shows created with create_podcast). \
                Returns title, description, visibility, episode count, and cover art URL for each. \
                Use this before update_podcast or delete_my_podcast to confirm the target podcast_id.
                """,
                properties: [:],
                required: []
            ),
            ownedPodcastTool(
                name: PodcastNames.generatePodcastArtwork,
                description: """
                Generate cover art for a podcast using an image-generation model \
                (via OpenRouter), upload the result to Blossom, and return the public URL. \
                Pass the returned image_url to create_podcast or update_podcast as image_url. \
                Requires an OpenRouter key and image-generation model to be configured in Settings > Models.
                """,
                properties: [
                    "prompt": ["type": "string", "description": "Detailed description of the artwork to generate. Include style, mood, subject, and any branding cues. Square images work best for podcast covers."],
                ],
                required: ["prompt"]
            ),
            ownedPodcastTool(
                name: PodcastNames.publishEpisode,
                description: """
                Publish an existing episode as a NIP-74 kind:30075 Nostr event to its parent \
                agent-owned podcast. Uploads the episode audio, chapters, and transcript to \
                Blossom, then signs and broadcasts the event to the configured relay. \
                Returns the NIP-19 naddr of the published event so it can be shared or referenced. \
                Fails with a descriptive error when: Nostr is disabled, the parent podcast is \
                private, no relay is configured, or the episode does not belong to an \
                agent-owned podcast.
                """,
                properties: [
                    "episode_id": ["type": "string", "description": "The UUID of the episode to publish. Must belong to an agent-owned podcast whose visibility is 'public'."],
                ],
                required: ["episode_id"]
            ),
        ]
    }

    /// Local copy of the OpenAI function-tool builder for owned-podcast schemas.
    private static func ownedPodcastTool(
        name: String,
        description: String,
        properties: [String: Any],
        required: [String]
    ) -> [String: Any] {
        [
            "type": "function",
            "function": [
                "name": name,
                "description": description,
                "parameters": [
                    "type": "object",
                    "properties": properties,
                    "required": required,
                ] as [String: Any],
            ] as [String: Any],
        ]
    }
}
