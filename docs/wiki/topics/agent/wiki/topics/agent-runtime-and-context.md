---
title: "Agent Runtime And Context"
category: topics
sources:
  - raw/notes/2026-05-09-agent-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [agent, context, prompt, runtime]
aliases: [Agent Context Strategy, Embedded Agent Runtime]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "The agent should carry inventory and handles in prompt context, then use tools for transcripts, wiki pages, playback, briefings, and UI actions."
---

# Agent Runtime And Context

The agent cannot keep a podcast library in its prompt. A user with many subscriptions will produce thousands of episodes, transcript chunks, wiki pages, and briefings. The runtime needs a handle-based context strategy.

## Prompt Strategy

The prompt should include:

- current playback state
- subscription inventory
- recent and new episodes
- current user intent and voice mode state
- available tools
- safety policy for remote commands
- concise memory and preferences

The prompt should not include full episode transcripts, full wiki pages, or long listening history dumps. Those belong behind tools.

## Runtime Strategy

Text chat, voice mode, and Nostr inbound messages should use the same underlying tool-calling loop where possible. That keeps behavior consistent and concentrates safety controls in one place.

## Operational Rule

The agent's eyes are its tools. If it needs facts, it calls `query_wiki`, `query_transcripts`, or `search_episodes`. If it needs to act, it calls playback or UI tools. If a remote Nostr command asks for a sensitive action, the bridge should gate or ask for approval before dispatch.

## See Also

- [[tool-surface|Tool Surface]] ([Tool Surface](../concepts/tool-surface.md)) - available tools.
- [[voice-briefing-loop|Voice Briefing Loop]] ([Voice Briefing Loop](../concepts/voice-briefing-loop.md)) - voice-specific runtime behavior.
- [[knowledge-pipeline|Knowledge Pipeline]] ([Knowledge Pipeline](../../../knowledge/wiki/topics/knowledge-pipeline.md)) - data the agent can query.

## Sources

- [Agent source map](../../raw/notes/2026-05-09-agent-source-map.md)
