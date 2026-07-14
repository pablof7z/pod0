import Foundation

// MARK: - YouTubeIngestionSkill
//
// Defines the `youtube_ingestion` skill. When activated via
// `use_skill(skill_id: "youtube_ingestion")` the agent receives the manual
// below and gains access to `search_youtube` and `ingest_youtube_video`.
//
// Requires a user-configured extractor endpoint in Settings → Providers →
// YouTube Ingestion. Without it the tools return a clear error.

enum YouTubeIngestionSkill {

    static let skill = AgentSkill(
        id: AgentSkillID.youtubeIngestion,
        displayName: "YouTube Ingestion",
        summary: "Search YouTube and download video audio as podcast episodes via a self-hosted extractor; transcribes on ingest so RAG and wiki tools can reference the content.",
        manual: manualText,
        toolNames: [AgentTools.PodcastNames.searchYouTube, AgentTools.PodcastNames.ingestYouTubeVideo],
        schema: { schemaEntries }
    )

    // MARK: - Manual

    private static let manualText: String = """
    # YouTube Ingestion Skill

    Two tools: `search_youtube` to find videos, and `ingest_youtube_video` to
    download audio and publish it to the "Agent Generated" podcast — making it
    transcribable, searchable via `query_transcripts`, summarisable with
    `summarize_episode`, and usable as a snippet in `generate_tts_episode`.

    ## Prerequisites

    The user must have configured a YouTube extractor endpoint in
    Settings → Providers → YouTube Ingestion. Both tools return a clear error
    if it is missing — tell the user what to configure and do not retry.

    ## When to use this

    - The user wants to find YouTube videos on a topic before deciding what to add.
    - The user pastes a YouTube URL and asks to "add it", "download it", or
      "make a podcast from it".
    - The user wants to transcribe or summarise a YouTube video.
    - The user wants a YouTube clip as a snippet in a TTS episode (ingest first,
      then use the returned `episode_id` in a `generate_tts_episode` snippet turn).

    ## Tools

    `search_youtube(query, limit?)`
    - `query` (required) — search terms.
    - `limit` (optional, default 5, max 20) — number of results to return.
    - Returns a list of hits, each with `url`, `title`, `author`, and optionally
      `duration_seconds`. Pass `url` from a result directly to `ingest_youtube_video`.

    `ingest_youtube_video(url, title?, transcribe?)`
    - `url` (required) — full YouTube URL (youtube.com/watch?v=… or youtu.be/…).
    - `title` (optional) — override the title; defaults to the video's own title.
    - `transcribe` (optional, default true) — start transcription immediately.
      Required before `query_transcripts` or `summarize_episode` can work.
    - Returns `episode_id`, `title`, `author`, `duration_seconds`,
      `transcript_status` ("ready" / "failed" / "queued"), and `message`.

    ## Suggested flow

    1. If the user doesn't have a specific URL, call `search_youtube` first and
       surface a short list for the user to pick from.
    2. Call `ingest_youtube_video` with the chosen URL. Default `transcribe: true`
       unless the user only wants audio without search capability.
    3. When `transcript_status == "ready"`, proceed with `query_transcripts`,
       `summarize_episode`, or a `generate_tts_episode` snippet pipeline.
    4. When `transcript_status == "failed"`, tell the user — the episode is still
       playable, just not searchable yet.
    """

    // MARK: - Schema

    @MainActor
    private static var schemaEntries: [[String: Any]] {
        [searchYouTubeSchema, ingestYouTubeVideoSchema]
    }

    private static var searchYouTubeSchema: [String: Any] {
        functionTool(
            name: AgentTools.PodcastNames.searchYouTube,
            description: """
            Search YouTube for videos matching a query. Returns a list of results with url, title, \
            author, and duration_seconds. Use this before ingest_youtube_video when the user doesn't \
            have a specific URL — surface the results and let the user pick. \
            Requires a YouTube extractor endpoint configured in Settings → Providers → YouTube Ingestion.
            """,
            properties: [
                "query": ["type": "string", "description": "Search terms to find YouTube videos."],
                "limit": ["type": "integer", "description": "Maximum results to return (1–20). Defaults to 5."],
            ],
            required: ["query"]
        )
    }

    private static var ingestYouTubeVideoSchema: [String: Any] {
        functionTool(
            name: AgentTools.PodcastNames.ingestYouTubeVideo,
            description: """
            Download a YouTube video's audio and publish it to the 'Agent Generated' podcast, \
            then optionally transcribe it so it becomes searchable via query_transcripts and \
            summarise_episode. Returns the episode_id for use in subsequent tool calls. \
            Requires a YouTube extractor endpoint configured in Settings → Providers → YouTube Ingestion.
            """,
            properties: [
                "url": ["type": "string", "description": "Full YouTube video URL (youtube.com/watch?v=… or youtu.be/…)."],
                "title": ["type": "string", "description": "Optional title override. Defaults to the video's own title."],
                "transcribe": ["type": "boolean", "description": "Start transcription immediately after download (default: true). Required for query_transcripts and summarize_episode to work on the episode."],
            ],
            required: ["url"]
        )
    }

    // MARK: - Helper

    private static func functionTool(
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
