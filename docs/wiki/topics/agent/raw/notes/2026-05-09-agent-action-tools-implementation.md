---
title: "Agent Action Tools Implementation"
source: "Local repo App/Sources/Agent and AppTests/Sources"
type: notes
ingested: 2026-05-09
tags: [agent, tools, implementation, playback, library]
summary: "Current implementation source note for concrete podcast action tools: playback pause/rate/sleep timer, episode state mutation, download, transcription request, feed refresh, and TENEX-compatible delegation."
---

# Agent Action Tools Implementation

Current implementation files:

- `App/Sources/Agent/AgentTools+Podcast.swift` owns canonical podcast tool names and dispatch.
- `App/Sources/Agent/AgentTools+PodcastActions.swift` implements action tools for playback, sleep timer, episode state, download, transcription, feed refresh, and delegation.
- `App/Sources/Agent/AgentToolSchema+Podcast.swift` exposes OpenAI-compatible schema entries for the same tools.
- `App/Sources/Agent/LivePodcastAgentToolDeps.swift` wires the tools to `PlaybackState`, `EpisodeDownloadService`, `TranscriptIngestService`, `SubscriptionRefreshService`, and `LiveTENEXDelegationBridge`.
- `AppTests/Sources/AgentToolsPodcastActionTests.swift` covers action dispatch against actor-based mocks.

Implemented action tools:

- `pause_playback`
- `set_playback_rate(rate)`
- `set_sleep_timer(mode, minutes?)`
- `mark_episode_played(episode_id)`
- `mark_episode_unplayed(episode_id)`
- `download_episode(episode_id)`
- `request_transcription(episode_id)`
- `refresh_feed(podcast_id)`
- `delegate(recipient, prompt)`

The current action implementation returns JSON success/error envelopes through the existing `AgentTools.dispatchPodcast` path. Audit/activity logging is still a platform-layer follow-up unless the future `ToolGateway` wraps these calls.
