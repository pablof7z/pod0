---
title: "Tool Permissions And Approvals"
category: concepts
sources:
  - raw/notes/2026-05-09-agent-tool-platform-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [agent, permissions, approvals, safety]
aliases: [Tool Safety Model, Approval Gates]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Tools need permission classes so local, voice, Nostr, background, paid, public, and destructive actions are treated differently."
---

# Tool Permissions And Approvals

The tool platform needs explicit permission classes. A single "friend allowed" or "agent enabled" toggle is too coarse for the lifetime of this app.

## Permission Classes

- `read_local`: local state, wiki, transcript, and search reads.
- `mutate_undoable`: notes, bookmarks, queue, playback context, and highlights.
- `attention`: start playback, speak audio, show notification, or interrupt current audio.
- `external_network`: web research, directory search, feed fetch, OP3 lookup.
- `paid_provider`: transcription, embeddings, TTS, rerank, long generation.
- `public_or_social`: post comment, send Nostr message, or share clip.
- `sensitive_settings`: provider connection flows, privacy settings, storage policy.
- `destructive`: delete data, unsubscribe, reset app, broad cache clearing, or broad user-data export.
- `secret`: raw keys and credentials; tools never receive these.

## Surface Defaults

- In-app chat: reads and undoable mutations allowed; attention and paid tools ask when surprising.
- Voice: reads, playback, and briefings allowed when local user initiated voice mode.
- Now Playing in-episode mode: only current-episode tools and safe context actions.
- Nostr: read-only by default; mutating, attention, social, paid, and destructive actions require approval or are blocked.
- Background: scheduled maintenance only; no public sharing, no playback, no destructive action.
- Widget/Control Center: explicit single-action tools only.

## Approval Objects

An approval record should persist:

- requested tool and arguments
- requesting actor
- human-readable summary
- risk class
- estimated cost
- expiration
- approving user and timestamp
- final execution result

Approvals should resume through the same `ToolGateway`, not bypass it.

## See Also

- [[nostr-command-safety|Nostr Command Safety]] ([Nostr Command Safety](nostr-command-safety.md)) - remote command implications.
- [[tool-execution-infrastructure|Tool Execution Infrastructure]] ([Tool Execution Infrastructure](tool-execution-infrastructure.md)) - where permissions run.
- [[lifetime-tool-catalog|Lifetime Tool Catalog]] ([Lifetime Tool Catalog](lifetime-tool-catalog.md)) - tools classified by risk.

## Sources

- [Agent tool platform source map](../../raw/notes/2026-05-09-agent-tool-platform-source-map.md)
