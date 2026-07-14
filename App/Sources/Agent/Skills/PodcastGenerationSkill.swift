import Foundation

// MARK: - PodcastGenerationSkill
//
// Defines the `podcast_generation` skill. When activated via
// `use_skill(skill_id: "podcast_generation")` the agent receives the manual
// below as the tool result and gains access to three TTS-domain tools:
// `list_available_voices`, `generate_tts_episode`, and `configure_agent_voice`.
//
// The manual is intentionally explicit about the turn-to-chapter mapping
// because that mapping is enforced by `AgentTTSComposer` and cannot be
// adjusted from outside the composer — the agent must structure turns to
// produce the chapter layout it wants.

enum PodcastGenerationSkill {

    static let skill = AgentSkill(
        id: AgentSkillID.podcastGeneration,
        displayName: "Podcast Generation",
        summary: "Create custom audio podcast episodes with TTS narration and audio snippets from existing episodes; auto-generates chapters and exposes the user's ElevenLabs voice library.",
        manual: manualText,
        toolNames: [
            AgentTools.PodcastNames.listAvailableVoices,
            AgentTools.PodcastNames.generateTTSEpisode,
            AgentTools.PodcastNames.configureAgentVoice,
        ],
        schema: { schemaEntries }
    )

    // MARK: - Manual

    private static let manualText: String = """
    # Podcast Generation Skill

    You can author a brand-new audio podcast episode and publish it to the user's
    library under the "Agent Generated" subscription. The pipeline accepts an
    ordered list of *turns* and stitches them into a single m4a, then attaches
    auto-generated chapters and a transcript.

    ## When to use this

    - The user asks for a TLDR, summary, or compilation episode.
    - The user wants a multi-speaker mock interview, a topic explainer, or a
      cross-episode highlight reel.
    - The user says "make me a podcast about…" / "narrate this with voice X" /
      "play the highlights of episodes A and B back to back".

    Skip this skill for tasks that don't end in a published episode — answer
    inline, summarise in chat, or use `play_episode` (called once per segment
    with `queue_position: "end"`) if the user just wants segments queued.

    ## Tools

    - `list_available_voices(query?: string)` — fetches the user's ElevenLabs
      voice library. Returns voice_id, name, category, gender, accent, age, and
      a preview URL. ALWAYS call this before picking a `voice_id` you haven't
      already confirmed with the user — voice IDs are account-specific.
    - `generate_tts_episode(title, description?, turns, play_now?)` — composes
      and publishes the episode. See "Turn structure" below.
    - `configure_agent_voice(voice_id)` — sets the agent's default voice for
      any future speech turn that omits `voice_id`. Persistent across
      conversations.

    ## Turn structure

    `turns` is an ordered array. Each turn is one of two kinds:

    1. `speech` turn → ElevenLabs TTS narration.
       Fields: `kind: "speech"`, `text` (required), `voice_id` (optional).
       Omitting `voice_id` uses the agent's configured default voice.
       Text supports ElevenLabs emotion markers inline: `[cheerfully]`,
       `[excitedly]`, `[laughs]`, `[whispers]`, `[sighs]`. Place the marker
       directly before the words it modifies.

    2. `snippet` turn → an original-audio excerpt from an existing episode.
       Fields: `kind: "snippet"`, `episode_id` (required), `start_seconds`
       (required), `end_seconds` (required, must be > start), `label`
       (optional, becomes the chapter title).
       Resolve `episode_id` + timestamps via `query_transcripts` or the
       episode's chapter list BEFORE calling — guessing wastes a generation.

    Turns play back in the order you provide them. Multi-speaker dialogue =
    alternate `speech` turns with different `voice_id` values.

    ## Chapter generation (auto)

    Chapters are produced from the turn structure — you do NOT pass chapters
    directly. The mapping is:

    - Consecutive `speech` turns collapse into ONE chapter. Its title is the
      first ~60 characters of the combined speech text. Pick your first few
      words deliberately — they become the chapter title.
    - Each `snippet` turn becomes its OWN chapter, even when sandwiched
      between speech turns. The chapter title uses the turn's `label` if
      provided, otherwise it falls back to the source episode's title.
    - Each snippet chapter also carries the source episode's artwork, so the
      now-playing card swaps art mid-episode. Pass meaningful `label` values
      so users see "[Guest Name] on X" rather than the raw episode title.
    - All generated chapters are flagged `isAIGenerated: true`.

    Chapter start times are derived from the concatenated turn durations.
    Speech-turn duration is whatever ElevenLabs returns; snippet-turn duration
    is `end_seconds - start_seconds`.

    ## Quality gate

    Composing a podcast is a multi-step authoring task. BEFORE drafting the
    script (i.e. before the first `generate_tts_episode` call) you should
    almost always call `upgrade_thinking` — voice selection, narrative flow,
    snippet sourcing, and chapter pacing all benefit from the stronger model.

    ## Suggested flow

    1. Call `upgrade_thinking` with a one-line reason ("composing a
       multi-turn podcast episode").
    2. Call `list_available_voices` (optionally with a `query` filter like
       "british female") and pick a voice or surface options to the user.
    3. If snippets are involved, call `query_transcripts` to pin exact
       `episode_id` + `start_seconds` + `end_seconds` for each excerpt.
    4. Call `generate_tts_episode` with an ordered turn list. Use a clear
       opening sentence per speech-block (it becomes the chapter title).
    5. Default `play_now` to `true` only when the user asked for immediate
       playback — otherwise leave it false so the episode lands quietly in
       the library.
    """

    // MARK: - Tool schemas

    @MainActor
    private static var schemaEntries: [[String: Any]] {
        [
            listAvailableVoicesSchema,
            generateTTSEpisodeSchema,
            configureAgentVoiceSchema,
        ]
    }

    // MARK: - list_available_voices

    private static var listAvailableVoicesSchema: [String: Any] {
        functionTool(
            name: AgentTools.PodcastNames.listAvailableVoices,
            description: """
            List the user's available ElevenLabs voices for use in `generate_tts_episode` speech turns. \
            Returns each voice's voice_id, name, category (premade/cloned/professional/generated), gender, \
            accent, age, description, and preview URL. \
            Use this before assigning `voice_id` values you have not already confirmed — voice IDs are \
            account-specific. Supply an optional `query` to filter by name, accent, gender, or use-case.
            """,
            properties: [
                "query": ["type": "string", "description": "Optional case-insensitive filter applied against voice name, voice ID, category, and labels (gender / accent / age / use_case / description)."],
                "limit": ["type": "integer", "description": "Maximum voices returned (1–50). Defaults to 30."],
            ],
            required: []
        )
    }

    // MARK: - generate_tts_episode (moved from AgentToolSchema+Podcast.swift)

    private static var generateTTSEpisodeSchema: [String: Any] {
        functionTool(
            name: AgentTools.PodcastNames.generateTTSEpisode,
            description: """
            Synthesise a custom audio episode using ElevenLabs TTS and/or original episode snippets, \
            then publish it to the 'Agent Generated' podcast so the user can play it like any other episode. \
            Use for requests like 'make me a TLDR of the ADHD stuff', 'create a fake podcast interview', \
            or 'summarise X with snippets from Y and Z'. \
            Turns are ordered: each turn is either a 'speech' turn (text → TTS with a specific voice) \
            or a 'snippet' turn (an original audio clip from an existing episode). \
            For multi-speaker dialogue, alternate speech turns with different voice_id values. \
            You can use ElevenLabs emotion cues in text like '[cheerfully]', '[excitedly]', '[laughs]'. \
            For snippet turns, resolve episode IDs and timestamps via query_transcripts first. \
            Chapters and transcripts are generated automatically from the turn structure — see the \
            podcast_generation skill manual for the exact mapping.
            """,
            properties: [
                "title": ["type": "string", "description": "Episode title shown in the library."],
                "description": ["type": "string", "description": "Short episode description (plain text)."],
                "turns": [
                    "type": "array",
                    "description": "Ordered sequence of turns that make up the episode. Must contain at least one entry.",
                    "items": [
                        "type": "object",
                        "properties": [
                            "kind": ["type": "string", "enum": ["speech", "snippet"], "description": "'speech' for TTS narration, 'snippet' for an original-audio excerpt from an existing episode."],
                            "text": ["type": "string", "description": "Text to synthesise. Required when kind='speech'. Supports ElevenLabs emotion markers like [cheerfully]."],
                            "voice_id": ["type": "string", "description": "ElevenLabs voice ID for this speech turn. Omit to use the agent's configured default voice."],
                            "episode_id": ["type": "string", "description": "UUID of the source episode. Required when kind='snippet'."],
                            "start_seconds": ["type": "number", "description": "Start of the audio excerpt in seconds. Required when kind='snippet'."],
                            "end_seconds": ["type": "number", "description": "End of the audio excerpt in seconds. Required when kind='snippet'."],
                            "label": ["type": "string", "description": "Optional label for a snippet turn (e.g. speaker name or topic). Becomes the chapter title."],
                        ] as [String: Any],
                        "required": ["kind"],
                    ] as [String: Any],
                ] as [String: Any],
                "play_now": ["type": "boolean", "description": "If true, immediately start playing the finished episode. Defaults to false."],
                "podcast_id": ["type": "string", "description": "UUID of an agent-owned podcast to publish the episode to (from create_podcast or list_my_podcasts). Omit to publish to the default 'Agent Generated' show. When the target podcast is public and Nostr is enabled, the episode is automatically published as a NIP-74 kind:30075 event."],
            ],
            required: ["title", "turns"]
        )
    }

    // MARK: - configure_agent_voice (moved from AgentToolSchema+Podcast.swift)

    private static var configureAgentVoiceSchema: [String: Any] {
        functionTool(
            name: AgentTools.PodcastNames.configureAgentVoice,
            description: """
            Set the agent's default ElevenLabs voice ID. Future `generate_tts_episode` speech turns \
            that omit `voice_id` will use this voice. \
            Use when the user says 'use a different voice', 'sound like X', or explicitly picks a voice \
            from the ElevenLabs voice list. Always confirm the voice_id exists in the user's ElevenLabs \
            account (via `list_available_voices`) before setting.
            """,
            properties: [
                "voice_id": ["type": "string", "description": "ElevenLabs voice ID to set as the agent's default."],
            ],
            required: ["voice_id"]
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
