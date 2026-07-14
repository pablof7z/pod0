---
title: "Lifetime Tool Catalog"
category: concepts
sources:
  - raw/notes/2026-05-09-agent-tool-platform-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [agent, tools, catalog]
aliases: [Agent Tool Catalog, Full Tool Surface]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "The agent needs tools across library, playback, transcripts, knowledge, briefings, highlights, social, settings, diagnostics, and exports."
---

# Lifetime Tool Catalog

The lifetime tool surface should be organized by domain. The model sees only the domain slice relevant to the current surface and permission context.

## Delegation Tool

- `delegate(recipient, prompt)` - TENEX-compatible async delegation to another local agent or team. `recipient` is an agent slug or team name. `prompt` contains the task and full context. The tool returns a delegation event ID, records an audit entry, and the agent must stop for the turn after a successful delegation.

## Library And Feed Tools

- `search_directory`, `subscribe_to_feed`, `unsubscribe_from_feed`
- `refresh_feed`, `refresh_all_feeds`
- `get_subscription`, `list_subscriptions`, `set_subscription_policy`
- `list_episodes`, `get_episode`, `mark_played`, `archive_episode`, `star_episode`
- `queue_episode`, `reorder_queue`, `clear_queue`

## Playback And Listening Tools

- `play_episode_at`, `pause_playback`, `resume_playback`, `seek_relative`
- `set_playback_rate`, `set_skip_durations`, `set_sleep_timer`
- `set_now_playing`, `get_now_playing`, `open_chapter`
- `toggle_smart_speed`, `toggle_voice_boost`, `set_show_playback_defaults`

## Transcript And Clip Tools

- `get_transcript_status`, `request_transcription`, `retry_transcription`
- `get_transcript_window`, `query_transcripts`, `correct_speaker_label`
- `create_bookmark`, `create_highlight`, `anchor_note`
- `create_clip_semantic`, `create_clip_range`, `share_clip`

## Knowledge And Retrieval Tools

- `search_episodes`, `find_similar_episodes`, `query_wiki`
- `get_wiki_page`, `list_wiki_topics`, `compile_episode_wiki`
- `compile_show_wiki`, `compile_concept_wiki`, `verify_claim`
- `track_topic`, `track_person`, `export_wiki_markdown`

## Briefing And Generated Media Tools

- `plan_briefing`, `generate_briefing`, `synthesize_briefing_audio`
- `play_briefing`, `pause_briefing`, `resume_briefing_at_beat`
- `generate_debate_brief`
- `generate_video_overview` as a later, high-cost artifact tool

## Research And External Source Tools

- `perplexity_search`, `fetch_url_summary`, `fact_check_claim`
- `ingest_external_source`, `compare_podcast_claim_to_web`
- `create_research_note`, `attach_source_to_wiki_page`

## Social, Nostr, And Creator Tools

- `send_nostr_reply`, `share_episode_to_contact`
- `request_remote_action_approval`, `approve_remote_action`
- `read_episode_discussion`

## Settings, Privacy, Storage, And Diagnostics

- `get_settings_summary`, `request_settings_change`
- `connect_provider_flow`, `disconnect_provider`
- `get_storage_usage`
- `delete_user_data` only through explicit non-agent confirmation
- `run_health_check`, `get_job_status`

## See Also

- [[tool-family-matrix|Tool Family Matrix]] ([Tool Family Matrix](../references/tool-family-matrix.md)) - risk and infrastructure per family.
- [[tenex-delegate-tool|TENEX Delegate Tool]] ([TENEX Delegate Tool](tenex-delegate-tool.md)) - delegation compatibility details.
- [[tool-permissions-and-approvals|Tool Permissions And Approvals]] ([Tool Permissions And Approvals](tool-permissions-and-approvals.md)) - which tools require gates.
- [[agent-tool-platform|Agent Tool Platform]] ([Agent Tool Platform](../topics/agent-tool-platform.md)) - platform context.

## Sources

- [Agent tool platform source map](../../raw/notes/2026-05-09-agent-tool-platform-source-map.md)
