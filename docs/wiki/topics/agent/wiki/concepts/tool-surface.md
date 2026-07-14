---
title: "Tool Surface"
category: concepts
sources:
  - raw/notes/2026-05-09-agent-source-map.md
  - raw/notes/2026-05-09-agent-action-tools-implementation.md
created: 2026-05-09
updated: 2026-05-09
tags: [agent, tools, playback, retrieval]
aliases: [Podcast Agent Tools]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "The agent needs retrieval, playback, library mutation, briefing, UI, delegation, and research tools, with risky tools gated by permission context."
---

# Tool Surface

The tool surface is the agent's controlled interface to the app and knowledge base. This page captures the early podcast-specific slice. The full lifetime design lives in [[lifetime-tool-catalog|Lifetime Tool Catalog]] ([Lifetime Tool Catalog](lifetime-tool-catalog.md)) and [[agent-tool-platform|Agent Tool Platform]] ([Agent Tool Platform](../topics/agent-tool-platform.md)).

## Retrieval Tools

- `search_episodes(query, scope?)`
- `query_transcripts(query, scope?)`
- `query_wiki(topic, scope?)`
- `find_similar_episodes(seed_episode_id)`

## Playback And UI Tools

- `play_episode_at(episode_id, timestamp)`
- `pause_playback()`
- `set_playback_rate(rate)`
- `set_sleep_timer(mode, minutes?)`
- `set_now_playing(episode_id, timestamp?)`
- `open_screen(route)`
- `share_clip(episode_id, start, end, recipient?)`

## Library And Feed Action Tools

- `mark_episode_played(episode_id)`
- `mark_episode_unplayed(episode_id)`
- `download_episode(episode_id)`
- `request_transcription(episode_id)`
- `refresh_feed(podcast_id)`

## Synthesis And Research Tools

- `summarize_episode(episode_id)`
- `generate_briefing(scope, length, voice?)`
- `perplexity_search(query)`
- `delegate(recipient, prompt)`

## In-Episode Context Tools

These tools are scoped exclusively to the **In-Episode Agent** (UX-16) surface — invoked while an episode is actively playing. The agent receives the current transcript window (≈90 s look-back) as implicit context; these tools act on that episode and position.

- `seek_to_topic_start()` — detects the transcript sentence where the current active topic began (topic-shift heuristic: silence + speaker turn + TF-IDF delta) and seeks to it.
- `create_clip_semantic(position?, window_s?)` — derives in/out timestamps from topic/sentence boundaries near the current playhead; creates a `ClipCard` artifact without requiring the user to specify times.
- `anchor_note(body)` — drops a timestamped annotation at the current playhead position, visible in the Episode Detail transcript view.
- `research_inline(query)` — fires a lightweight async RAG thread (transcript → podcast wiki → library wiki → Perplexity BYOK) and surfaces the answer as a dismissible glass banner in the Now Playing screen without interrupting playback.

## Safety And Logging

Mutating tools should record agent activity before they are exposed to remote actors. The current direct `AgentTools.dispatchPodcast` implementation returns JSON envelopes and is covered by unit tests, but central audit/activity logging still belongs in the future `ToolGateway` wrapper. Remote Nostr calls should expose a narrower default tool set than local voice or in-app chat. Tools that start playback, send messages, share clips, delegate work, or perform external research should be explicitly classified.

## See Also

- [[agent-runtime-and-context|Agent Runtime And Context]] ([Agent Runtime And Context](../topics/agent-runtime-and-context.md)) - why tools carry library access.
- [[agent-tool-platform|Agent Tool Platform]] ([Agent Tool Platform](../topics/agent-tool-platform.md)) - complete infrastructure design.
- [[lifetime-tool-catalog|Lifetime Tool Catalog]] ([Lifetime Tool Catalog](lifetime-tool-catalog.md)) - full tool family catalog.
- [[retrieval-and-citation-model|Retrieval And Citation Model]] ([Retrieval And Citation Model](../../../knowledge/wiki/concepts/retrieval-and-citation-model.md)) - retrieval contracts behind the tools.
- [[nostr-command-safety|Nostr Command Safety]] ([Nostr Command Safety](nostr-command-safety.md)) - remote invocation boundaries.

## Sources

- [Agent source map](../../raw/notes/2026-05-09-agent-source-map.md)
- [Agent action tools implementation](../../raw/notes/2026-05-09-agent-action-tools-implementation.md)
