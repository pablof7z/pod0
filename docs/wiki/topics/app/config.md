---
title: "Podcastr App Wiki"
description: "End-to-end operating guide for the current Podcastr app, codebase, runtime flows, integrations, and development rules."
created: 2026-05-12
freshness_threshold: 30
---

# Wiki Configuration

## Scope

This topic wiki is the app-wide entry point for Podcastr. It explains how the current shipped code hangs together across product surfaces, Swift modules, persistent state, audio playback, transcripts, local wiki/RAG, agent tools, BYOK provider credentials, Nostr identity, settings, feedback, and release operations.

## Conventions

- Prefer current repo files over older product-intent notes when describing runtime behavior.
- Link out to the product, experience, agent, knowledge, and adjacent topic wikis instead of duplicating their full content.
- Keep implementation guidance concrete enough that a future agent can choose the right file or service before editing.
