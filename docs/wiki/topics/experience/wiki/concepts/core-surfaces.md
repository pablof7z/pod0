---
title: "Core Surfaces"
category: concepts
sources:
  - raw/notes/2026-05-09-experience-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [ux, surfaces, player, search, wiki]
aliases: [Product Surfaces, Main Screens]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "The main app surfaces should cover Now Playing, Library, Episode Detail, Search, Wiki, Agent Chat, Voice, and Briefings as connected workflows."
---

# Core Surfaces

The app should not hide its differentiator behind a single chat tab. Knowledge, citations, and agent actions need to show up inside the normal podcast surfaces.

## Primary Surfaces

- Now Playing: playback controls, transcript highlights, chapter context, agent entry, and timestamp citations.
- Library: subscriptions, queue, filters, download state, and readiness badges for transcript/wiki.
- Episode Detail: show notes, transcript, speaker list, wiki summary, clips, bookmarks, and related threads.
- Universal Search: keyword, semantic, transcript, wiki, and directory search in one place.
- Wiki: concept, person, show, and cross-episode pages.
- Agent Chat: typed and Nostr-aware conversation with visible citations and action cards.
- Voice Mode: full-screen or compact live conversation state.
- Briefings: generated audio summaries with source anchors and resumable beats.

## Workflow Rule

Any agent answer that references content should be able to open the exact episode and timestamp. Any episode page should be able to summon the agent with that episode as context.

## See Also

- [[experience-north-star|Experience North Star]] ([Experience North Star](../topics/experience-north-star.md)) - principles behind the surfaces.
- [[retrieval-and-citation-model|Retrieval And Citation Model]] ([Retrieval And Citation Model](../../../knowledge/wiki/concepts/retrieval-and-citation-model.md)) - citation data those surfaces render.
- [[tool-surface|Tool Surface]] ([Tool Surface](../../../agent/wiki/concepts/tool-surface.md)) - agent actions surfaced in the UI.

## Sources

- [Experience source map](../../raw/notes/2026-05-09-experience-source-map.md)
