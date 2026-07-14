---
title: "Implementation Priority"
category: references
sources:
  - raw/notes/2026-05-09-online-adjacent-research.md
created: 2026-05-09
updated: 2026-05-09
tags: [implementation, priority, roadmap]
aliases: [Adjacent Ideas Priority]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Adjacent ideas should be sequenced by reuse of existing architecture, product value, and risk: metadata and exports first, social/value/live later."
---

# Implementation Priority

Adjacent ideas should not distract from core playback, transcripts, wiki compilation, retrieval, and voice. Sequence them where they reinforce the core.

## Near-Term

- Parse and store Podcasting 2.0 metadata already relevant to the app: chapters, transcript, person, soundbite, podroll, location, and alternateEnclosure.
- Add snip/highlight model with timestamp, transcript text, speaker, note, and tags.
- Export generated wiki pages and highlights as Markdown.
- Add App Intent entry points for start voice mode, resume, save snip, and start briefing.

## Mid-Term

- Support NotebookLM-style briefing formats: brief, deep dive, critique, debate.
- Add daily review of podcast highlights.
- Render creator recommendations from podrolls.
- Show OP3 public stats where available.
- Add Live Activity citation/chapter surface.

## Later

- socialInteract comment reading/posting.
- liveItem listening rooms.
- generated video overviews.
- full Obsidian vault sync and Readwise-style integrations.

## Product Risk

The social and live features are rich but can pull the app away from the core "converse with my podcast library" promise. Metadata parsing, snips, exports, and ambient controls are safer because they directly strengthen that promise.

## See Also

- [[expansion-opportunity-map|Expansion Opportunity Map]] ([Expansion Opportunity Map](../topics/expansion-opportunity-map.md)) - full idea map.
- [[podcasting-2-rich-metadata|Podcasting 2 Rich Metadata]] ([Podcasting 2 Rich Metadata](../concepts/podcasting-2-rich-metadata.md)) - source metadata to parse first.
- [[learning-and-knowledge-exports|Learning And Knowledge Exports]] ([Learning And Knowledge Exports](../concepts/learning-and-knowledge-exports.md)) - export ideas to prioritize.

## Sources

- [Online adjacent research](../../raw/notes/2026-05-09-online-adjacent-research.md)
