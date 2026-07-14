---
title: "User-Facing Capabilities"
summary: "Current user-visible Podcastr surfaces and the runtime systems each one owns."
tags: [app, ux, features, surfaces]
aliases: [Podcastr features, app surfaces]
sources:
  - raw/notes/2026-05-12-app-system-source-map.md
created: 2026-05-12
updated: 2026-05-12
verified: 2026-05-12
volatility: warm
confidence: high
---

# User-Facing Capabilities

Podcastr's visible product surface is split into a few stable entry points, but most of the value comes from how they interact. The Home feed, Now Playing, Wiki, Agent, Voice, Settings, and Feedback surfaces all share the same state and knowledge substrate.

## Home And Library

Home is the main library surface. It combines a dateline, active category chips, featured resume cards, agent picks, threaded-today signals, subscription lists or grids, filters for all/unplayed/downloaded/transcribed, and pull-to-refresh. Empty states route users into adding shows.

Library details are distributed through show detail, episode detail, OPML import/export, category settings, and subscription management rather than a separate standalone Library tab.

## Search And Subscription

Search is a first-class tab. It supports discovery and subscription through iTunes/Apple podcast search and routes subscribed shows through the same subscription service used by paste URL and OPML paths.

## Player

The player has two levels: a persistent mini-player and full `PlayerView`. The full view owns playback chrome, artwork, chapter rail, transcript/ask-agent entry points, sleep timer, playback rate, share, downloads, AirPlay, AutoSnip hints, chapter hydration, AI chapter compilation, and gesture-driven clipping.

The player is also a context source for agent chat and voice notes. A chapter, transcript line, clip, or voice note can become an agent prompt with episode and timestamp context.

## Wiki

The Wiki tab lists generated in-app wiki pages from `WikiStorage`. It supports search, generation, detail reading, citation peeks, regeneration, deletion, and navigation to cross-episode threading topics. The wiki is a user-facing knowledge layer, not only a documentation system for developers.

## Agent And Voice

Agent chat is a sheet, not a tab, because it is available from every tab and can survive dismissal. It supports conversations, history, transcript export, tool activity inspection, retry/regenerate, seeded context from playback, and model/provider gating.

Voice mode opens through shortcuts and hardware routes, then can hand off to text chat. Briefings are generated audio artifacts that combine retrieval, scripting, TTS, and optional source clips.

## Settings

Settings is organized around account, library, listening, intelligence, and system concerns. Intelligence includes agent identity and permissions, providers, model selection, transcripts, and wiki controls. Listening includes player behavior and downloads. System includes notifications and data/storage.

## Feedback And Identity

Shake-to-feedback is global and uses a compose/capture/annotate workflow. The app also carries Nostr identity, remote signing, friends, pending approvals, and feedback thread surfaces, so external agent/user interaction is part of the app model.

## See Also

- [[app-operating-model|App Operating Model]] ([App Operating Model](app-operating-model.md))
- [[core-surfaces|Core Surfaces]] ([Core Surfaces](../../../experience/wiki/concepts/core-surfaces.md))
- [[ambient-and-accessibility-surfaces|Ambient And Accessibility Surfaces]] ([Ambient And Accessibility Surfaces](../../../experience/wiki/concepts/ambient-and-accessibility-surfaces.md))
- [[data-and-integration-flows|Data And Integration Flows]] ([Data And Integration Flows](../concepts/data-and-integration-flows.md))

## Sources

- [Podcastr App System Source Map](../../raw/notes/2026-05-12-app-system-source-map.md)
