---
title: "Agent Tool Platform"
category: topics
sources:
  - raw/notes/2026-05-09-agent-tool-platform-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [agent, tools, infrastructure, platform]
aliases: [Tool Platform, Agent Operating Layer]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Podcastr needs a tool platform: dynamic tool palettes, typed dependencies, permission gates, durable jobs, artifact handles, audit logs, and evals."
---

# Agent Tool Platform

The agent should not be a chat model with a handful of hard-coded functions. It should be an operating layer over the app, with typed capabilities, context-specific tool palettes, durable jobs, explicit permissions, and an audit trail.

## Platform Shape

The current `AgentTools` pattern is a good start, but it should become a `ToolGateway` with a registry behind it. The gateway owns:

- schema assembly for the current surface
- typed argument decoding
- capability availability checks
- permission and approval checks
- rate, cost, and timeout limits
- dispatch into domain services
- activity and audit logging
- durable artifact and job handles in tool results

The LLM should see the smallest useful tool set for the current surface. A Now Playing voice turn should not see storage reset tools. A Nostr DM should not see social send, playback start, or destructive tools without approval.

## Design Principles

- Return handles, not huge bodies.
- Separate read tools from mutating tools.
- Treat external network, paid providers, public sharing, and destructive actions as permission classes.
- Make long-running work a job, not a synchronous tool call.
- Never expose secrets or raw provider keys to model context.
- Make every claim-producing tool return citations or provenance.
- Keep every side effect inspectable and, when possible, undoable.

## Required Runtime Pieces

- `ToolRegistry`: metadata for every tool.
- `AgentExecutionContext`: user, surface, route, playback state, network state, privacy mode, budget, and actor identity.
- `ToolGateway`: validates and dispatches calls.
- `ApprovalQueue`: persists pending actions needing user confirmation.
- `AgentJobQueue`: durable async work.
- `ArtifactStore`: clips, briefings, exports, reports, generated wiki pages, and research outputs.
- `ToolAuditLog`: immutable tool-call history with redaction.
- `ToolEvals`: fixtures and golden tests for schemas, dispatch, permissions, and result shape.
- `TENEXDelegationBridge`: adapter for `delegate(recipient, prompt)` and completion routing.

## See Also

- [[lifetime-tool-catalog|Lifetime Tool Catalog]] ([Lifetime Tool Catalog](../concepts/lifetime-tool-catalog.md)) - full tool families.
- [[tenex-delegate-tool|TENEX Delegate Tool]] ([TENEX Delegate Tool](../concepts/tenex-delegate-tool.md)) - async delegation contract.
- [[tool-execution-infrastructure|Tool Execution Infrastructure]] ([Tool Execution Infrastructure](../concepts/tool-execution-infrastructure.md)) - runtime mechanics.
- [[tool-permissions-and-approvals|Tool Permissions And Approvals]] ([Tool Permissions And Approvals](../concepts/tool-permissions-and-approvals.md)) - safety model.
- [[background-agent-operations|Background Agent Operations]] ([Background Agent Operations](../concepts/background-agent-operations.md)) - long-running work.

## Sources

- [Agent tool platform source map](../../raw/notes/2026-05-09-agent-tool-platform-source-map.md)
