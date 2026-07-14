---
title: "App Operating Model"
summary: "End-to-end model of how Podcastr hangs together at runtime: root shell, state, playback, knowledge, agent, providers, and platform services."
tags: [app, architecture, runtime, podcastr]
aliases: [Podcastr operating model, app overview]
sources:
  - raw/notes/2026-05-12-app-system-source-map.md
created: 2026-05-12
updated: 2026-05-12
verified: 2026-05-12
volatility: warm
confidence: high
---

# App Operating Model

Podcastr is an iOS podcast player whose baseline podcast behavior is deliberately fused with local knowledge and an embedded agent. The app is not just "play feeds plus chat"; the central loop is library -> playback -> transcript/wiki/RAG -> agent tools -> playback or generated outputs.

The product promise is documented in [[product-vision|Product Vision]] ([Product Vision](../../../product/wiki/topics/product-vision.md)), while the runtime mechanics are grounded in `RootView`, `AppStateStore`, `PlaybackState`, `TranscriptIngestService`, `RAGService`, `WikiStorage`, and `AgentChatSession`.

## Shell

`RootView` is the runtime shell. It owns the Home, Search, Clippings, and Wiki tabs; Settings and Agent are toolbar sheets; the player is a persistent mini-player that expands into `PlayerView`. This matters because many features are not isolated screens: player events seed agent context, deep links can seek playback, feedback is global, and voice mode can open from shortcuts or hardware actions.

The app gates first run with onboarding, listens for shake-to-feedback, routes Spotlight and custom URL deep links, handles ask-agent notifications from transcript/chapter gestures, and keeps the agent session alive even when the sheet is dismissed.

## State

The state model is intentionally centralized. `AppState` stores subscriptions, episodes, notes, friends, memories, categories, settings, Nostr trust state, activity logs, clips, and threading data. `AppStateStore` wraps that struct, owns mutations, persists changes, rebuilds episode projections, coalesces playback-position writes, wires widget refreshes, attaches RAG and download services, starts feed refresh, and syncs selected settings via iCloud KVS.

Secrets are outside this state blob. Provider API keys are stored by Keychain-backed credential stores, with only non-secret metadata in `Settings`.

## Knowledge

The knowledge pipeline starts with feeds and episodes, but the meaningful unit is indexed, cited, timestamped content. `TranscriptIngestService` prefers publisher transcripts, falls back to configured STT providers when allowed, chunks parsed transcripts, embeds them, upserts them into `RAGService`, persists transcript JSON, and updates episode status.

Generated wiki pages are persisted by `WikiStorage`; search and briefing flows consume the same retrieval substrate. See [[data-and-integration-flows|Data And Integration Flows]] ([Data And Integration Flows](../concepts/data-and-integration-flows.md)) and [[knowledge-pipeline|Knowledge Pipeline]] ([Knowledge Pipeline](../../../knowledge/wiki/topics/knowledge-pipeline.md)).

## Agent

The agent is both conversational UI and tool runner. `AgentPrompt` gives it a compact live inventory. The larger library, transcripts, wiki pages, generated briefings, playback actions, category changes, downloads, transcription requests, Nostr delegation, external podcast lookup, clips, and generated TTS episodes are exposed through tools rather than stuffing everything into the prompt.

This keeps the default prompt small while still letting the agent act on the app. The detailed agent strategy lives in [[agent-runtime-and-context|Agent Runtime And Context]] ([Agent Runtime And Context](../../../agent/wiki/topics/agent-runtime-and-context.md)).

## Provider Layer

BYOK is the primary provider setup path. `BYOKConnectService` opens the BYOK authorization flow with PKCE and a `podcastr://byok` callback; `PodcastBYOKCredentialImporter` fans returned tokens into the right Keychain-backed stores for OpenRouter, ElevenLabs, AssemblyAI, Ollama, and Perplexity.

Provider choice then affects agent chat, embeddings, wiki compilation, chapter compilation, transcription, TTS, briefings, reranking, and online search. Treat provider changes as app-wide, not as a settings-only concern.

## See Also

- [[user-facing-capabilities|User Facing Capabilities]] ([User Facing Capabilities](user-facing-capabilities.md))
- [[codebase-map|Codebase Map]] ([Codebase Map](../references/codebase-map.md))
- [[development-and-release-guide|Development And Release Guide]] ([Development And Release Guide](../references/development-and-release-guide.md))

## Sources

- [Podcastr App System Source Map](../../raw/notes/2026-05-12-app-system-source-map.md)
