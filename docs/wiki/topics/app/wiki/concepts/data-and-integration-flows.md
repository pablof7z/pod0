---
title: "Data And Integration Flows"
summary: "How podcasts, playback, transcripts, RAG, wiki, agent tools, provider credentials, and Nostr communication connect inside Podcastr."
tags: [data-flow, integrations, rag, byok, nostr]
aliases: [Podcastr data flows, integration map]
sources:
  - raw/notes/2026-05-12-app-system-source-map.md
created: 2026-05-12
updated: 2026-05-12
verified: 2026-05-12
volatility: warm
confidence: high
---

# Data And Integration Flows

Podcastr's architecture is best understood as a set of loops around episodes. Feeds add episodes; playback produces position and context; transcripts make episodes searchable; wiki and RAG turn them into durable knowledge; agent tools turn knowledge back into playback, clips, briefings, or app mutations.

## Feed To Episode

Subscriptions come from search, feed URLs, or OPML import. Feed refresh writes subscriptions and episodes into app state, with larger episode needs backed by SQLite. Episode projections in `AppStateStore` power fast Home and Library filters.

## Episode To Playback

`PlaybackState` wraps the audio engine. `RootView` wires callbacks so playback positions persist through `AppStateStore`, finished episodes can be marked played, queue advancement can honor sleep timer state, and lock-screen metadata can resolve show names, artwork, and active chapter titles from live store data.

## Episode To Transcript And Index

`TranscriptIngestService` chooses the transcript route:

1. Use publisher transcript metadata when present.
2. Parse and persist the transcript.
3. Chunk transcript text with episode and podcast keys.
4. Embed chunks with the configured provider.
5. Upsert chunks into `RAGService`.
6. Update episode transcript state.
7. Fall back to the selected STT provider only when configured and allowed.

The selected STT provider can be ElevenLabs Scribe, AssemblyAI, OpenRouter Whisper, or Apple on-device STT.

## Transcript To Wiki And Briefing

RAG search and wiki storage share the indexed transcript base. `WikiStorage` persists generated pages and inventory, while briefing modules use retrieval to build personalized audio catch-ups. This is the bridge between passive listening and "talk to all my podcasts."

## Agent Tool Loop

`AgentPrompt` gives the model a compact live inventory. When the user asks for specific content or action, the agent calls tools: search episodes, query transcripts, query wiki, play at timestamp, refresh feeds, download episodes, request transcription, generate briefings, create clips, queue segments, generate TTS episodes, search external podcasts, subscribe, or delegate.

The key design rule is that the agent should use tools for detailed content and mutations instead of relying on prompt memory.

## Provider Credentials

BYOK returns provider tokens through a PKCE web-auth flow. `PodcastBYOKCredentialImporter` stores each raw API key in the right Keychain-backed credential store and leaves only source/key-label metadata in `Settings`. OpenRouter drives chat, models, wiki, embeddings, and Whisper; ElevenLabs drives Scribe and TTS; AssemblyAI and Apple can transcribe; Ollama can serve models/embeddings; Perplexity powers online lookup.

## Nostr And Feedback

Nostr is used for identity, remote signing, trusted friends, pending approvals, relay-backed communication, and feedback workflows. Treat Nostr writes as externally visible operations that need identity, relay, approval, and provenance checks.

## See Also

- [[app-operating-model|App Operating Model]] ([App Operating Model](../topics/app-operating-model.md))
- [[knowledge-pipeline|Knowledge Pipeline]] ([Knowledge Pipeline](../../../knowledge/wiki/topics/knowledge-pipeline.md))
- [[tool-surface|Tool Surface]] ([Tool Surface](../../../agent/wiki/concepts/tool-surface.md))
- [[nostr-command-safety|Nostr Command Safety]] ([Nostr Command Safety](../../../agent/wiki/concepts/nostr-command-safety.md))

## Sources

- [Podcastr App System Source Map](../../raw/notes/2026-05-12-app-system-source-map.md)
