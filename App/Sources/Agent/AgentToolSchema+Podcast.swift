import Foundation

// MARK: - Podcast tool schema (OpenAI tool format)
//
// This file declares the OpenAI-compatible JSON-schema entries for every
// podcast-domain tool the agent can call. The orchestrator concatenates this
// array onto `AgentTools.schema` at merge time:
//
//     let allTools = AgentTools.schema + AgentTools.podcastSchema
//
// Adding a new podcast tool requires four steps, in order:
//   1. Add the canonical name to `AgentTools.PodcastNames`
//      in `AgentTools+Podcast.swift`.
//   2. Add a `tool(...)` entry below.
//   3. Add a `case` in `AgentTools.dispatchPodcast(...)`.
//   4. Add a unit test in `AgentToolsPodcastTests.swift`.

extension AgentTools {

    /// OpenAI-compatible tool schema array for the podcast tool surface.
    ///
    /// Kept as a separate computed property (not merged into `AgentTools.schema`)
    /// so the existing template tests continue to assert against the original
    /// schema unchanged.
    @MainActor
    static var podcastSchema: [[String: Any]] {
        [
            podcastTool(
                name: PodcastNames.playEpisode,
                description: """
                Single play verb. Covers library episodes, one-off external URLs, jump-to-timestamp, \
                bounded-segment playback, and queue management — pick `queue_position` to control \
                whether playback starts now or the episode lands in Up Next. \
                Identify the episode with `episode_id` when it is already in the library \
                (from list_episodes / search_episodes / list_recent_unplayed / etc.). Otherwise pass \
                `audio_url` plus `title` to play an arbitrary URL (e.g. an enclosure from \
                search_podcast_directory or a user-pasted link); include `feed_url` when you have it \
                so the app captures the show's real artwork and metadata, otherwise the episode parents \
                to an "Unknown" podcast row. \
                Use this when the user says 'play that part where…' (start_seconds), \
                'play the intro' (start_seconds + end_seconds), 'play this next' (queue_position=next), \
                or 'add this to my queue' (queue_position=end). \
                For multi-segment playback (e.g. 'play the intro then the interview part'), call this \
                tool once per segment — first with queue_position=now, the rest with queue_position=end. \
                A subtle audio cue marks each transition when the queue advances.
                """,
                properties: [
                    "episode_id": ["type": "string", "description": "The episode's stable ID (UUID). Use when the episode is already in the library. Omit when supplying audio_url instead."],
                    "audio_url": ["type": "string", "description": "Direct audio URL of an episode NOT yet in the library — e.g. the enclosure URL from search_podcast_directory, or a user-pasted link. Pair with title. Mutually exclusive with episode_id."],
                    "title": ["type": "string", "description": "Episode title for audio_url plays. Required when audio_url is set; ignored otherwise."],
                    "feed_url": ["type": "string", "description": "Optional RSS feed URL of the source podcast (only meaningful with audio_url). Pass this whenever you have it so the app captures the show's real metadata and artwork; omit for raw URLs with no known feed."],
                    "duration_seconds": ["type": "number", "description": "Optional episode duration in seconds. Only used for audio_url plays; ignored for episode_id."],
                    "start_seconds": ["type": "number", "description": "Position to start playback from, in seconds. Defaults to 0 (beginning)."],
                    "end_seconds": ["type": "number", "description": "Optional position to stop playback and advance to the next queue item. Omit for open-ended playback to the end of the episode. Must be greater than start_seconds when both are set."],
                    "queue_position": [
                        "type": "string",
                        "enum": ["now", "next", "end"],
                        "description": "Where to land this play. 'now' = start playing immediately; existing Up Next items are preserved and resume afterward. 'next' = insert at the head of Up Next so it plays after the current item ends. 'end' = append to the bottom of Up Next.",
                    ],
                ],
                required: []
            ),
            podcastTool(
                name: PodcastNames.pausePlayback,
                description: "Pause the current podcast playback and persist the playhead. Use when the user asks you to stop or pause.",
                properties: [:],
                required: []
            ),
            podcastTool(
                name: PodcastNames.setPlaybackRate,
                description: "Set the active playback speed. Use when the user asks to listen faster or slower.",
                properties: [
                    "rate": ["type": "number", "description": "Requested playback speed multiplier. The player supports roughly 0.5x to 3.0x."],
                ],
                required: ["rate"]
            ),
            podcastTool(
                name: PodcastNames.setSleepTimer,
                description: "Arm or clear the sleep timer. Use for requests like 'stop after 30 minutes' or 'pause at the end of this episode'.",
                properties: [
                    "mode": ["type": "string", "enum": ["off", "minutes", "end_of_episode"], "description": "Timer mode."],
                    "minutes": ["type": "integer", "description": "Duration in minutes when mode is 'minutes'. Max 180."],
                ],
                required: ["mode"]
            ),
            podcastTool(
                name: PodcastNames.searchEpisodes,
                description: "Semantic + keyword search across every episode in the user's library (subscribed shows AND one-off captured episodes). Use for fuzzy recall like 'the one about stamps last week' or topical queries like 'episodes on Zone 2 training'.",
                properties: [
                    "query": ["type": "string", "description": "Natural-language query."],
                    "scope": ["type": "string", "description": "Optional podcast ID to constrain the search to one show."],
                    "limit": ["type": "integer", "description": "Maximum results (1–25). Defaults to 10."],
                ],
                required: ["query"]
            ),
            podcastTool(
                name: PodcastNames.queryWiki,
                description: "Look up a topic in the LLM-generated podcast wiki. Use this for definitional questions ('what is Zone 2?') or to surface cross-episode context the user has already built up.",
                properties: [
                    "topic": ["type": "string", "description": "Topic, person, place, or concept to look up."],
                    "scope": ["type": "string", "description": "Optional podcast ID to constrain the wiki lookup to one show's wiki."],
                    "limit": ["type": "integer", "description": "Maximum pages returned (1–10). Defaults to 5."],
                ],
                required: ["topic"]
            ),
            // NOTE: `create_wiki_page`, `list_wiki_pages`, and
            // `delete_wiki_page` live under the `wiki_research` skill
            // (see `App/Sources/Agent/Skills/WikiResearchSkill.swift`).
            // Their tool name constants stay in `PodcastNames.all` so
            // `dispatchPodcast` can route them; the LLM only sees their
            // schemas when the skill is enabled.
            // `query_wiki` stays always-on as a cheap lookup.
            podcastTool(
                name: PodcastNames.queryTranscripts,
                description: "RAG search over transcript chunks. Returns timestamped excerpts with speaker labels. Use this when the user asks 'what did they say about X?' and you need direct quotes to ground the answer.",
                properties: [
                    "query": ["type": "string", "description": "Natural-language question."],
                    "scope": ["type": "string", "description": "Optional scope: an episode_id, a podcast_id, or omit for all-corpus search."],
                    "limit": ["type": "integer", "description": "Maximum chunks returned (1–25). Defaults to 8."],
                ],
                required: ["query"]
            ),
            podcastTool(
                name: PodcastNames.generateBriefing,
                description: "Compose a personalized TLDR audio briefing across episodes. Use when the user asks 'catch me up on this week' or wants a synthesized digest. Returns a briefing handle the user can play.",
                properties: [
                    "scope": ["type": "string", "description": "Selection scope: 'this_week', 'unlistened', a podcast_id, or a custom keyword."],
                    "length": ["type": "integer", "description": "Target length in minutes (3–30)."],
                    "style": ["type": "string", "enum": ["news", "deep_dive", "quick_hits"], "description": "Optional style hint."],
                ],
                required: ["scope", "length"]
            ),
            podcastTool(
                name: PodcastNames.perplexitySearch,
                description: "Run an online web search via Perplexity for facts that are NOT in the user's podcast corpus. Use for current events, fact-checks, citation lookups, or contrarian-take queries.",
                properties: [
                    "query": ["type": "string", "description": "Question to send to Perplexity."],
                ],
                required: ["query"]
            ),
            podcastTool(
                name: PodcastNames.summarizeEpisode,
                description: "Generate an on-demand summary of a single episode. Use when the user asks 'TLDR this' or 'what's this episode about?' for a specific episode.",
                properties: [
                    "episode_id": ["type": "string", "description": "The episode to summarize."],
                    "length": ["type": "string", "enum": ["short", "medium", "long"], "description": "Optional summary length. Defaults to 'medium'."],
                ],
                required: ["episode_id"]
            ),
            podcastTool(
                name: PodcastNames.findSimilarEpisodes,
                description: "Discovery — find episodes semantically similar to a seed episode. Use when the user says 'more like this' or 'what else should I listen to after this?'.",
                properties: [
                    "seed_episode_id": ["type": "string", "description": "The episode to use as a similarity seed."],
                    "k": ["type": "integer", "description": "Number of similar episodes to return (1–20). Defaults to 5."],
                ],
                required: ["seed_episode_id"]
            ),
            podcastTool(
                name: PodcastNames.markEpisodePlayed,
                description: "Mark an episode as played. Use only when the user explicitly asks to mark something listened/done.",
                properties: [
                    "episode_id": ["type": "string", "description": "The episode to mark played."],
                ],
                required: ["episode_id"]
            ),
            podcastTool(
                name: PodcastNames.markEpisodeUnplayed,
                description: "Mark an episode as unplayed. Use when the user says they have not listened yet or wants to undo a played mark.",
                properties: [
                    "episode_id": ["type": "string", "description": "The episode to mark unplayed."],
                ],
                required: ["episode_id"]
            ),
            podcastTool(
                name: PodcastNames.downloadEpisode,
                description: "Download an episode's audio for offline playback. Use when the user asks to save or download an episode.",
                properties: [
                    "episode_id": ["type": "string", "description": "The episode to download."],
                ],
                required: ["episode_id"]
            ),
            podcastTool(
                name: PodcastNames.requestTranscription,
                description: "Request transcript ingestion for an episode, using publisher transcripts first and the configured STT provider when needed.",
                properties: [
                    "episode_id": ["type": "string", "description": "The episode to transcribe/index."],
                ],
                required: ["episode_id"]
            ),
            podcastTool(
                name: PodcastNames.refreshFeed,
                description: "Refresh a known podcast's feed and ingest newly published episodes. Works on any podcast in the library — subscribed or not — as long as it has a feed URL. Use when the user asks for the latest from one show.",
                properties: [
                    "podcast_id": ["type": "string", "description": "The podcast/feed UUID to refresh. Get it from list_subscriptions, list_podcasts, or list_episodes."],
                ],
                required: ["podcast_id"]
            ),
            podcastTool(
                name: PodcastNames.listSubscriptions,
                description: """
                List the podcasts the user is currently subscribed to, sorted by title. \
                Use when the user asks 'what am I subscribed to?' or before suggesting they \
                unsubscribe from a specific show. Returns each show's id, title, author, \
                total + unplayed episode counts, and last-published date. \
                Distinct from `list_podcasts`, which returns every known podcast — subscribed \
                AND unsubscribed (one-off external plays, captured-via-browse feeds, the \
                AI-generated show). Use `list_podcasts` when the user asks 'what shows do I \
                have in the app?' or before calling `delete_podcast`.
                """,
                properties: [
                    "limit": ["type": "integer", "description": "Maximum subscriptions to return. Defaults to 25, capped at 100."],
                ],
                required: []
            ),
            podcastTool(
                name: PodcastNames.listPodcasts,
                description: """
                List EVERY podcast known to the store — subscribed AND unsubscribed — sorted \
                by title. Each row carries a `subscribed: true|false` flag so you can \
                distinguish followed shows from one-off captures (external-played episodes, \
                feeds browsed via list_episodes, the AI-generated show). \
                Use this when the user asks about 'all my podcasts', wants to clean up the \
                library, or before calling `delete_podcast`. For the subscribed set only, \
                use `list_subscriptions`.
                """,
                properties: [
                    "limit": ["type": "integer", "description": "Maximum podcasts to return. Defaults to 25, capped at 100."],
                ],
                required: []
            ),
            podcastTool(
                name: PodcastNames.listCategories,
                description: "List the LLM-generated podcast categories in the user's library. Use this before answering category-management questions or before moving a show between categories.",
                properties: [
                    "limit": ["type": "integer", "description": "Maximum categories to return. Defaults to 25, capped at 100."],
                    "include_podcasts": ["type": "boolean", "description": "Whether to include compact podcast rows under each category. Defaults to true."],
                ],
                required: []
            ),
            podcastTool(
                name: PodcastNames.changePodcastCategory,
                description: "Move a subscribed podcast into an existing generated category. Use only after identifying the target category by list_categories or from the user's explicit wording.",
                properties: [
                    "podcast_id": ["type": "string", "description": "The subscribed podcast/feed ID to move."],
                    "category_id": ["type": "string", "description": "Preferred target category UUID from list_categories."],
                    "category_slug": ["type": "string", "description": "Alternative target category slug when the UUID is unavailable."],
                    "category_name": ["type": "string", "description": "Alternative target category display name when the UUID or slug is unavailable."],
                ],
                required: ["podcast_id"]
            ),
            podcastTool(
                name: PodcastNames.listEpisodes,
                description: """
                List episodes of a specific podcast, newest first. \
                Returns played/unplayed state and current playback position for each episode. \
                Pass EXACTLY ONE of `podcast_id` or `feed_url`:
                  - `podcast_id` as an internal UUID → reads the user's already-known library row.
                  - `podcast_id` as an iTunes numeric collection ID (e.g. "863897795" from \
                    search_podcast_directory) → resolves the feed and captures metadata WITHOUT \
                    subscribing the user.
                  - `feed_url` as an RSS URL → captures metadata WITHOUT subscribing.
                Use the external paths to offer episode lists for shows the user does not subscribe \
                to, so you never have to subscribe them just to browse. The response always includes \
                `podcast_id` (resolved internal UUID) for follow-up calls like play_episode.
                """,
                properties: [
                    "podcast_id": ["type": "string", "description": "Either an internal podcast UUID (from list_subscriptions / search_episodes) or an iTunes collection_id (from search_podcast_directory). Mutually exclusive with feed_url."],
                    "feed_url": ["type": "string", "description": "RSS feed URL of the podcast. Mutually exclusive with podcast_id."],
                    "limit": ["type": "integer", "description": "Maximum episodes to return. Defaults to 25, capped at 100."],
                ],
                required: []
            ),
            podcastTool(
                name: PodcastNames.listInProgress,
                description: "List episodes the user has started but not finished, newest first. Use to answer 'what was I listening to?' or 'what should I resume?' without spending a search call.",
                properties: [
                    "limit": ["type": "integer", "description": "Maximum episodes to return. Defaults to 25, capped at 100."],
                ],
                required: []
            ),
            podcastTool(
                name: PodcastNames.listRecentUnplayed,
                description: "List recently published episodes the user hasn't played, newest first. Mirrors the Today tab's New Episodes feed. Use to answer 'what's new?' or 'what should I listen to next?'.",
                properties: [
                    "limit": ["type": "integer", "description": "Maximum episodes to return. Defaults to 25, capped at 100."],
                ],
                required: []
            ),
            podcastTool(
                name: PodcastNames.createClip,
                description: "Save a clip of an episode on behalf of the user — creates a timestamped excerpt that appears in the user's Clippings tab. Use when the user says 'clip that', 'save that part', or asks you to bookmark a moment. Always confirm the start/end range with the user before clipping unless they were explicit. Prefer supplying transcript_text when you already have it from query_transcripts.",
                properties: [
                    "episode_id": ["type": "string", "description": "The episode to clip (UUID string)."],
                    "start_seconds": ["type": "number", "description": "Clip start time in seconds from the episode origin."],
                    "end_seconds": ["type": "number", "description": "Clip end time in seconds from the episode origin."],
                    "caption": ["type": "string", "description": "Optional user-visible headline for the clip."],
                    "transcript_text": ["type": "string", "description": "Verbatim transcript text for the clipped span. Supply this when you have it from a prior query_transcripts call so it doesn't need to be re-fetched."],
                ],
                required: ["episode_id", "start_seconds", "end_seconds"]
            ),
            podcastTool(
                name: PodcastNames.downloadAndTranscribe,
                description: """
                Download an episode for offline playback AND await the full transcription pipeline \
                before returning. Blocks until the transcript is ready (or fails). \
                Use this when you need the transcript to be available immediately after the call \
                — for example before calling query_transcripts on a fresh episode. \
                Prefer this over separate download_episode + request_transcription calls when the \
                goal is to have the transcript ready to query in the same turn. \
                For episodes not yet in the library, supply feed_url and audio_url instead of \
                episode_id — the tool captures the feed's metadata and episodes WITHOUT subscribing \
                the user, locates the episode, then downloads and transcribes it. If the user then \
                wants to follow the show, call subscribe_podcast separately.
                """,
                properties: [
                    "episode_id": ["type": "string", "description": "The episode to download and transcribe (UUID string). Omit when using feed_url + audio_url for external episodes."],
                    "feed_url": ["type": "string", "description": "RSS feed URL of the podcast. Required when episode_id is omitted."],
                    "audio_url": ["type": "string", "description": "Direct audio URL of the specific episode. Required when episode_id is omitted, used to locate the episode within the feed."],
                ],
                required: []
            ),
            // NOTE: `generate_tts_episode`, `configure_agent_voice`, and
            // `list_available_voices` live under the `podcast_generation`
            // skill (see `App/Sources/Agent/Skills/PodcastGenerationSkill.swift`).
            // Their tool name constants stay in `PodcastNames.all` so
            // `dispatchPodcast` can route them; the LLM only sees their
            // schemas when the skill is enabled.
            podcastTool(
                name: PodcastNames.searchPodcastDirectory,
                description: """
                Search the global Apple Podcasts directory by show name, episode title, or guest name. \
                Use this when the user asks about a podcast or episode that may not be in their library \
                — e.g. 'find the Lyn Alden episode on Library of Wealth' or 'is there a show about X?'. \
                Returns feed URLs, episode audio URLs, and metadata. \
                For recency-sensitive lookups ('most recent appearance', 'May 2026') combine with perplexity_search. \
                After finding a feed URL you can call subscribe_podcast or play_episode (with audio_url + feed_url).
                """,
                properties: [
                    "query": ["type": "string", "description": "Show name, episode title, guest name, or topic to search for."],
                    "type": ["type": "string", "enum": ["podcast", "episode"], "description": "Search for shows ('podcast') or individual episodes ('episode'). Defaults to 'episode'."],
                    "limit": ["type": "integer", "description": "Maximum results (1–20). Defaults to 5."],
                ],
                required: ["query"]
            ),
            podcastTool(
                name: PodcastNames.subscribePodcast,
                description: """
                Subscribe to a podcast feed by RSS URL — creates the `PodcastSubscription` row \
                and imports the feed's episodes. Use this only when the user explicitly says they \
                want to subscribe / follow / add the show to their library. \
                Idempotent at the subscription level: if the user is already subscribed the result \
                carries `already_subscribed: true`. If the Podcast row already exists from a prior \
                one-off play (via `play_episode` with `audio_url`) but no subscription row, this \
                promotes it into a subscription and backfills missing episodes.
                """,
                properties: [
                    "feed_url": ["type": "string", "description": "RSS feed URL of the podcast to subscribe to."],
                ],
                required: ["feed_url"]
            ),
            podcastTool(
                name: PodcastNames.deletePodcast,
                description: """
                Fully remove a podcast from the user's library: deletes the `Podcast` row, any \
                `PodcastSubscription` for it, and every episode tied to it. This matches the \
                'Unsubscribe' destructive action in the Subscriptions list AND the swipe-to-delete \
                on the All Podcasts screen — both of those are the same nuke under the hood. \
                Use when the user says 'unsubscribe from X', 'remove X from my library', or \
                'delete X'. The response includes `was_subscribed` so you can tell the user whether \
                this was a subscribed show or a one-off capture. Cannot delete the Unknown sentinel.
                """,
                properties: [
                    "podcast_id": ["type": "string", "description": "The podcast UUID to delete. Get it from list_subscriptions, list_podcasts, or list_episodes."],
                ],
                required: ["podcast_id"]
            ),
            podcastTool(
                name: PodcastNames.sendFriendMessage,
                description: "Send a Nostr kind:1 text note to a friend on the user's behalf. Use this when the user tells you to message, tell, ask, or hand off something to a named friend — from any context, including owner chat. The friend_pubkey MUST match a friend in the user's Friends list — the tool refuses unknown pubkeys. Inside a peer conversation the note threads as a NIP-10 reply; from owner chat it publishes as a standalone note.",
                properties: [
                    "friend_pubkey": ["type": "string", "description": "Hex pubkey or 6-character pubkey prefix of the friend (the prefix is shown in parentheses next to each name in the Friends list). Must match a friend in the user's Friends list."],
                    "message": ["type": "string", "description": "Plain text body of the note to send. Be direct and concise."],
                ],
                required: ["friend_pubkey", "message"]
            ),
        ] + ownedPodcastSchema
    }

    /// Tools that are only valid inside a Nostr peer conversation.
    /// `end_conversation` suppresses a reply to the active peer turn —
    /// the concept is meaningless in owner-chat.
    @MainActor
    static var peerOnlySchema: [[String: Any]] {
        [
            podcastTool(
                name: PodcastNames.endConversation,
                description: "Signal that you have nothing to say for the current peer message — publish no reply for this turn. Call this INSTEAD OF replying when the latest peer message is mere acknowledgment or social closure (thanks, ok, sounds good, see you) and there is nothing substantive to add. The conversation stays open — future messages from the peer will still be handled. Do not call this if the peer asked a question, made a request, or raised an ambiguity.",
                properties: [
                    "reason": ["type": "string", "description": "Why you are not replying. Logged locally for diagnostics; never transmitted to the peer."],
                ],
                required: ["reason"]
            ),
        ]
    }

    /// Local copy of the OpenAI function-tool builder. The base file's helper
    /// is `private`, so we duplicate the (tiny) shape here to keep this lane
    /// self-contained.
    private static func podcastTool(
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
