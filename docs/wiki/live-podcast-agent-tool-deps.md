---
title: Live Podcast Agent Tool Deps
slug: live-podcast-agent-tool-deps
topic: agent-system
summary: "The `openScreen(route:)` tool is fully advertised to the LLM, but its implementation is currently a logger.info no-op until a navigation router lands"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-13
updated: 2026-06-13
verified: 2026-05-13
compiled-from: conversation
sources:
  - session:0f3f24f7-54de-49f8-b160-a92f735f6a00
  - session:f11c47b8-a7bd-47d3-9eb0-79dd02904d04
  - session:rollout-2026-05-09T14-56-23-019e0c98-8803-7ef0-b7a2-bf0b605a2360
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Live Podcast Agent Tool Deps

## Tool Stubs and Missing-Host Behavior

The unified `play_episode` tool replaces the prior `play_external_episode`, `play_episode_at`, and `queue_episode_segments` tools. `play_episode` accepts `episode_id` (library path) or `audio_url` + `title` (external path), plus optional `start_seconds` and `end_seconds`, and a required `queue_position` (`now`|`next`|`end`). `set_now_playing` is removed; `play_episode` already sets the episode context and starts playback. `open_screen` is removed from the agent tool surface. For missing playback hosts, `setPlaybackRate`, `setSleepTimer`, and `pausePlayback` return `Optional`/`Bool` instead of faking success; their dispatch emits `toolError('Playback is unavailable.')`. (Previously: the missing playback host returned a 1.0 rate to the LLM as if applied.)

The agent has implemented tools: pause_playback, set_playback_rate, set_sleep_timer, mark_episode_played, mark_episode_unplayed, download_episode, request_transcription, refresh_feed, and delegate(recipient, prompt). <!-- [^rollo-13] -->

<!-- citations: [^0f3f2-43] [^f11c4-3] -->
## Data Defaults

The placeholder podcast title defaults to the feed hostname and persists permanently if hydration fails. <!-- [^0f3f2-44] -->

## File Size

LivePodcastAgentToolDeps.swift is 664 lines, which is 164 lines over the 500-line hard limit. <!-- [^0f3f2-45] -->

## User Interaction

The user can interact with the agent via STT/TTS to give orders and ask questions about podcasts, including playing specific timestamps, finding past podcasts by topic, and generating TLS briefings with conversational follow-up. <!-- [^rollo-14] -->

The Android Agent chat screen uses a Compose UI with transcript LazyColumn, composer, in-flight indicator, and dispatches `podcast.agent` ops through `ActionDispatcher`. <!-- [^c1691-285] -->
