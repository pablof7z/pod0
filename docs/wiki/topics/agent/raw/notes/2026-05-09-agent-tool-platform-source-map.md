---
title: "Agent Tool Platform Source Map"
source: "Local repo code, docs, and conversation request on 2026-05-09"
type: notes
ingested: 2026-05-09
tags: [agent, tools, infrastructure, source-map]
summary: "Source context for designing a complete tool platform: current AgentTools, PodcastAgentToolDeps, podcast schemas, agent prompt, Nostr bridge, knowledge stack, briefing stack, voice stack, and existing wiki articles."
---

# Agent Tool Platform Source Map

The user asked for a full design because the existing agent tool list is too small for the app's lifetime needs.

Primary local sources:

- [Agent runtime article](../../wiki/topics/agent-runtime-and-context.md)
- [Existing tool surface article](../../wiki/concepts/tool-surface.md)
- [Nostr safety article](../../wiki/concepts/nostr-command-safety.md)
- [In-episode agent article](../../wiki/concepts/in-episode-agent.md)
- [Knowledge pipeline](../../../knowledge/wiki/topics/knowledge-pipeline.md)
- [Retrieval and citation model](../../../knowledge/wiki/concepts/retrieval-and-citation-model.md)
- `App/Sources/Agent/AgentTools.swift`
- `App/Sources/Agent/AgentTools+Podcast.swift`
- `App/Sources/Agent/AgentToolSchema+Podcast.swift`
- `App/Sources/Agent/PodcastAgentToolDeps.swift`
- `App/Sources/Agent/AgentPrompt.swift`
- `App/Sources/Agent/AgentRelayBridge.swift`
- `App/Sources/Knowledge/`
- `App/Sources/Briefing/`
- `App/Sources/Voice/`
- `../TENEX-ff3ssq/docs/RUST-AGENT-SPEC.md`
- `/Users/pablofernandez/wiki/topics/tenex-protocol-notes/wiki/topics/tenex-delegation-and-ral.md`

Design direction:

- Move from a flat list of function tools to a tool platform.
- Keep tool schemas dynamic and surface-specific.
- Keep direct side effects behind a permission gateway.
- Return durable handles and citations instead of large payloads.
- Support long-running jobs for transcription, indexing, wiki compilation, exports, and generated media.
- Include a TENEX-compatible `delegate()` tool for async agent-to-agent work.
- Keep explicitly rejected bulk, diagnostics, provider-test, cache, creator monetization, study-drill, and draft-social actions out of the agent surface.
