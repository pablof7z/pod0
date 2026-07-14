---
title: "Agent Source Map"
source: "Local repo docs/spec and App/Sources"
type: notes
ingested: 2026-05-09
tags: [agent, tools, voice, nostr, sources]
summary: "Pointers to repo docs and source modules relevant to the embedded agent, voice mode, Nostr relay bridge, and implementation plan."
---

# Agent Source Map

Primary local sources:

- [Project Context](../../../../../spec/PROJECT_CONTEXT.md) - user stories and desired agent tool list.
- [Template Architecture and Extension Plan](../../../../../spec/research/template-architecture-and-extension-plan.md) - actual agent files, tool dispatcher shape, Nostr bridge, and prompt strategy.
- [Voice Stack](../../../../../spec/research/voice-stt-tts-stack.md) - STT/TTS, barge-in, AVAudioSession, and latency budget.
- [Embeddings and RAG Stack](../../../../../spec/research/embeddings-rag-stack.md) - retrieval tools the agent should call.
- `App/Sources/Agent/` - current prompt and tool schema files.
- `App/Sources/Features/Agent/` - current agent chat session and OpenRouter client shape.
- `App/Sources/Agent/AgentRelayBridge.swift` - Nostr inbound agent loop.
