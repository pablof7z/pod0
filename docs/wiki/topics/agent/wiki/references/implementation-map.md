---
title: "Implementation Map"
category: references
sources:
  - raw/notes/2026-05-09-agent-source-map.md
  - raw/notes/2026-05-09-agent-action-tools-implementation.md
created: 2026-05-09
updated: 2026-05-09
tags: [implementation, swift, agent, files]
aliases: [Agent Implementation Placement]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Agent additions should extend existing AgentTools, AgentPrompt, AgentRelayBridge, Voice, Briefing, and Knowledge modules; current action tools are split into Podcast, PodcastActions, Inventory, and live dependency adapters."
---

# Implementation Map

Agent work should follow the current codebase shape.

## Existing Areas To Extend

- `App/Sources/Agent/AgentPrompt.swift` - prompt inventory and handle strategy.
- `App/Sources/Agent/AgentToolSchema.swift` - schema entries for new tools.
- `App/Sources/Agent/AgentTools.swift` - shared dispatch routing.
- `App/Sources/Agent/AgentTools+Podcast.swift` - canonical podcast names and dispatch.
- `App/Sources/Agent/AgentTools+PodcastActions.swift` - playback, sleep timer, library mutation, transcription, feed refresh, and delegation tools.
- `App/Sources/Agent/AgentTools+PodcastInventory.swift` - list subscriptions, episodes, in-progress, and recent-unplayed inventory queries.
- `App/Sources/Agent/AgentToolSchema+Podcast.swift` - OpenAI-compatible podcast tool schemas.
- `App/Sources/Agent/LivePodcastAgentToolDeps.swift` - live adapters to playback, library, transcript, feed-refresh, and delegation services.
- `App/Sources/Agent/LiveTENEXDelegationBridge.swift` - TENEX-compatible delegation bridge.
- `App/Sources/Agent/AgentTools+RAG.swift` - transcript retrieval.
- `App/Sources/Agent/AgentTools+Wiki.swift` - compiled wiki lookup.
- `App/Sources/Agent/AgentTools+Briefing.swift` - briefing generation.
- `App/Sources/Agent/AgentTools+Web.swift` - external research.
- `App/Sources/Agent/AgentRelayBridge.swift` - Nostr inbound loop and remote safety gates.
- `App/Sources/Agent/ToolRegistry.swift` - proposed metadata registry for dynamic tool palettes.
- `App/Sources/Agent/ToolGateway.swift` - proposed central execution, permission, audit, and dispatch path.
- `App/Sources/Agent/AgentExecutionContext.swift` - proposed per-call context object.
- `App/Sources/Agent/AgentApprovalQueue.swift` - proposed persistent approvals for risky actions.
- `App/Sources/Agent/AgentJobQueue.swift` - proposed durable async job queue.
- `App/Sources/Voice/AudioConversationManager.swift` - voice orchestration.
- `App/Sources/Briefing/BriefingComposer.swift` - briefing scripts and anchors.
- `App/Sources/Knowledge/` - wiki, embeddings, and retrieval services.

## File Length Rule

Keep each concern split before it approaches the 300-line soft limit. The tool files should be grouped by domain, not by one giant podcast-agent file.

## See Also

- [[tool-surface|Tool Surface]] ([Tool Surface](../concepts/tool-surface.md)) - tool domains that map to these files.
- [[agent-tool-platform|Agent Tool Platform]] ([Agent Tool Platform](../topics/agent-tool-platform.md)) - proposed infrastructure beyond the current files.
- [[tool-family-matrix|Tool Family Matrix]] ([Tool Family Matrix](tool-family-matrix.md)) - service/store mapping for tool families.
- [[data-model-notes|Data Model Notes]] ([Data Model Notes](../../../knowledge/wiki/references/data-model-notes.md)) - persistence boundaries for tool implementations.

## Sources

- [Agent source map](../../raw/notes/2026-05-09-agent-source-map.md)
- [Agent action tools implementation](../../raw/notes/2026-05-09-agent-action-tools-implementation.md)
