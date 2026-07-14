---
title: "Snipd Feature Research"
source: "Public Snipd website, help center, share pages, and App Store listing"
type: notes
ingested: 2026-05-09
tags: [product, research, snipd, snips, guests, books, chapters]
summary: "Source-backed findings on Snipd's headphone snipping, mentioned books, guest graph, auto-chapters, AI DJ, exports, and inferred episode-processing model for Podcastr."
---

# Snipd Feature Research

This note captures public product evidence about Snipd and the product implications for Podcastr. It does not claim to know Snipd's private implementation.

## Source Findings

- Snipd's feature overview centers on transcripts with speaker identification, AI-generated chapters, episode summaries, AI snips, guests, mentioned books, headphone snipping, auto-snipping, custom prompts, shareable quotes, daily recap, Readwise/Notion/Obsidian/Markdown exports, uploaded audio, YouTube imports, and 26-language AI support.
- Headphone snipping uses the headphones' skip-back control as a capture trigger. When triggered, Snipd's AI captures and summarizes the relevant segment, including transcript and speaker names, then plays a confirmation beep.
- CarPlay applies the same model: a steering-wheel skip-back action creates a snip, and Snipd's AI identifies the right segment and writes the note. CarPlay also supports auto-snipping while listening.
- Snip settings expose generated-title control, short/long/custom AI summaries, and snip duration as auto or fixed. The auto duration tries to choose context and start/end boundaries.
- Snipd's AI DJ plays an AI-selected path through the most valuable original-audio moments, with short spoken bridges, targeting roughly one quarter of the original episode length.
- Mentioned books are extracted for processed episodes with title, author, cover, description, purchase links, and context from the episode. Snipd exposes episode-level book lists plus public top-books pages.
- Guest pages include bios and ranked podcast appearances; the App Store listing describes AI-identified guest names, bios, pictures, followed favorites, and similar guests.
- Snipd has a distinct "AI processing" state. Missing transcripts, speaker identification, and summaries can leave a snip as a plain timestamp until AI processing is run.
- Uploaded audio can receive the same AI-powered transcripts, chapters, snips, chat, and offline listening behavior.

## Product Implications

- Podcastr needs a visible processed-episode state machine, not a boolean "has transcript."
- Snips should be span-grounded artifacts with trigger time, selected start/end time, transcript spans, speakers, generated title, generated summary, custom prompt profile, source surface, and export metadata.
- Remote-command snipping must be configurable so it does not silently replace normal skip-back behavior.
- Auto-chapters, highlights, and DJ paths should be modeled separately even when they share extraction inputs.
- Mentioned books should use the same entity extraction and resolver pipeline as topics, guests, and claims.
- Guest pages should combine local-library speaker resolution with enriched person metadata, appearance lists, follow state, and local-library similar-guest recommendations.
- Snip export should be powered by the wiki/snip model and support Markdown, quote cards, audio/video clips, and PKM integrations.

## Sources

- https://www.snipd.com/all-features
- https://support.snipd.com/en/articles/10225450-create-snips-with-your-headphones
- https://support.snipd.com/en/articles/10225413-how-to-use-snipd-on-apple-carplay
- https://support.snipd.com/en/articles/11926947-snip-customizations
- https://support.snipd.com/en/articles/14287414-snipd-s-ai-dj
- https://www.snipd.com/blog/mentioned-books-release
- https://share.snipd.com/popular-guests
- https://share.snipd.com/person/marc-andreessen/RyltOuyyQ8SCP7SFLGBZdg
- https://share.snipd.com/podbooks
- https://support.snipd.com/en/articles/10226381-upload-your-own-audio-files
- https://support.snipd.com/en/articles/10537507-why-are-my-snips-missing-ai-summaries-and-only-show-1min-snip
- https://apps.apple.com/us/app/snipd-ai-podcast-player/id1557206126
