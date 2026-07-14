---
title: "Tool Execution Infrastructure"
category: concepts
sources:
  - raw/notes/2026-05-09-agent-tool-platform-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [agent, infrastructure, execution, registry]
aliases: [Tool Gateway, Tool Runtime]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Tool calls should flow through a gateway that decodes, authorizes, dispatches, logs, and returns stable handles."
---

# Tool Execution Infrastructure

The current code has `AgentTools.dispatch` and `dispatchPodcast`. That should evolve into a gateway rather than a larger switch statement.

## Tool Registry Metadata

Each tool registration should include:

- name, description, JSON schema, and result schema
- domain and owning service
- allowed surfaces: chat, voice, Now Playing, widget, Nostr, background
- permission class and approval strategy
- timeout, retry policy, and idempotency key behavior
- cost class: free, local compute, network, paid provider
- offline availability
- whether it mutates state
- undo or compensation handler
- audit redaction rules

## Execution Context

Every call receives an `AgentExecutionContext`:

- surface and route
- current playback state
- current episode and transcript window when present
- actor: local user, approved friend, remote Nostr pubkey, background scheduler
- network, battery, privacy mode, and provider availability
- current per-turn and per-month budget
- locale and accessibility mode

The same tool can behave differently by context. `query_transcripts` is safe everywhere. `play_episode_at` is local-safe, voice-safe, Nostr-approval-only, and background-blocked.

## Result Envelope

Every tool should return a normalized result envelope:

- `success` or `error`
- `tool_call_id`
- `summary`
- `artifacts`
- `citations`
- `job_id` when async
- `approval_id` when blocked pending approval
- `undo_id` when reversible
- `user_visible_card` for UI rendering

This keeps the agent, chat UI, voice captions, and activity sheets from each inventing result parsing.

## See Also

- [[agent-tool-platform|Agent Tool Platform]] ([Agent Tool Platform](../topics/agent-tool-platform.md)) - platform overview.
- [[background-agent-operations|Background Agent Operations]] ([Background Agent Operations](background-agent-operations.md)) - async jobs.
- [[tool-permissions-and-approvals|Tool Permissions And Approvals]] ([Tool Permissions And Approvals](tool-permissions-and-approvals.md)) - authorization path.

## Sources

- [Agent tool platform source map](../../raw/notes/2026-05-09-agent-tool-platform-source-map.md)
