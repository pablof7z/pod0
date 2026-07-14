---
title: "TENEX Delegate Tool"
category: concepts
sources:
  - raw/notes/2026-05-09-agent-tool-platform-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [agent, tools, tenex, delegation]
aliases: [delegate tool, TENEX delegation]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Podcastr's agent should expose a TENEX-compatible `delegate(recipient, prompt)` tool for async work by another local agent or team."
---

# TENEX Delegate Tool

Podcastr should include `delegate()` as the collaboration primitive instead of ad hoc social draft or helper-agent tools. The contract should stay compatible with TENEX so this app can reuse TENEX-style async delegation, completion routing, and audit semantics.

## Tool Contract

```text
delegate(recipient: string, prompt: string)
```

- `recipient`: agent slug or team name.
- `prompt`: task plus full context for the delegated agent.

On success, the tool returns a delegation event ID and tells the caller to stop for the turn. The parent agent resumes when the delegated work completes.

## TENEX Compatibility Requirements

- Fresh delegations are represented as kind `1` Nostr events.
- The event p-tags the recipient agent.
- When there is a parent conversation root, include a delegation parent tag equivalent to `["delegation", parent_root_id]`.
- Preserve project or workspace context tags when applicable.
- Emit a tool-use audit event for `delegate`.
- Store the delegation route so completions can be validated before resuming the parent.
- Completion handling must validate sender and recipient against stored route state, not just trust tags.

## Podcastr Use Cases

- Delegate a deep podcast-topic research task to a research agent.
- Delegate a transcript quality review to a transcript agent.
- Delegate a generated briefing critique to a reviewer agent.
- Delegate UX or accessibility review of a new surface to a specialist agent.
- Delegate a long-running implementation or verification job from the app's internal work queue.

## Not In Scope

Podcastr should not expose `delegate_followup`, `delegate_crossproject`, or `self_delegate` as first-pass app tools. Those can be added later if the app embeds more of TENEX directly. The first contract should be one simple TENEX-compatible `delegate()`.

## See Also

- [[lifetime-tool-catalog|Lifetime Tool Catalog]] ([Lifetime Tool Catalog](lifetime-tool-catalog.md)) - where `delegate` fits in the surface.
- [[tool-execution-infrastructure|Tool Execution Infrastructure]] ([Tool Execution Infrastructure](tool-execution-infrastructure.md)) - gateway and result envelope.
- [[tool-permissions-and-approvals|Tool Permissions And Approvals]] ([Tool Permissions And Approvals](tool-permissions-and-approvals.md)) - remote and async action gates.

## Sources

- [Agent tool platform source map](../../raw/notes/2026-05-09-agent-tool-platform-source-map.md)
