---
title: "Nostr Command Safety"
category: concepts
sources:
  - raw/notes/2026-05-09-agent-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [nostr, remote-control, safety, agent]
aliases: [Remote Agent Safety]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Nostr-mediated commands should use the existing relay bridge while narrowing remote tool permissions and requiring approval for sensitive actions."
---

# Nostr Command Safety

The template already has Nostr identity, relay, allowlist, pending approvals, and an agent relay bridge. Podcastr should keep that subsystem and extend it carefully.

## Remote Command Risk

A Nostr DM can arrive when the user is not looking at the device. That makes the following tools sensitive:

- playback changes
- clip sharing
- external web research
- notification changes
- data export
- account or provider settings
- actions that message another person

## Default Policy

Remote commands should default to read-only knowledge lookup and text answers. Mutating actions should be grouped:

- safe: answer a question from wiki or transcript context
- low-risk: prepare a briefing or queue an episode without playing
- needs approval: start playback, share a clip, send messages, or use external paid tools
- blocked: secrets, account changes, destructive data operations

## UX Rule

When approval is required, the app should show the requested action, the requesting pubkey or friend name, and the exact tool call in human-readable form.

## See Also

- [[tool-surface|Tool Surface]] ([Tool Surface](tool-surface.md)) - tool classes that need safety policy.
- [[agent-runtime-and-context|Agent Runtime And Context]] ([Agent Runtime And Context](../topics/agent-runtime-and-context.md)) - shared runtime used by the bridge.

## Sources

- [Agent source map](../../raw/notes/2026-05-09-agent-source-map.md)
