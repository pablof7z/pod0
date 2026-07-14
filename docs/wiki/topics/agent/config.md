---
title: "Podcastr Agent Wiki"
description: "Embedded agent runtime, context strategy, tool surface, voice loop, and Nostr command safety."
created: 2026-05-09
freshness_threshold: 30
---

# Wiki Configuration

## Scope

This topic wiki captures how the embedded agent should reason, retrieve podcast knowledge, manipulate UI and playback, answer through voice, and communicate through Nostr.

## Conventions

- Keep the agent's context strategy tool-first and handle-based.
- Treat local UI and playback tools as sensitive operations when invoked remotely.
- Keep voice, text chat, and Nostr on the same underlying agent loop wherever possible.
