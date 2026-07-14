---
title: "Launch Floor"
category: references
sources:
  - raw/notes/2026-05-09-repo-spec-sources.md
created: 2026-05-09
updated: 2026-05-09
tags: [launch, baseline, podcast-player]
aliases: [Table Stakes, Baseline Features]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "The agent layer cannot compensate for missing baseline podcast-player features such as playback controls, queue, downloads, transcripts, sync, and accessibility."
---

# Launch Floor

The launch floor is the set of features that make Podcastr credible as a podcast app before any AI differentiation is considered.

## Must-Have Areas

- Playback: speed, skip controls, silence trim, voice boost, sleep timer, chapters, background audio, AirPlay, Bluetooth, Now Playing, and interruption handling.
- Queue and library: Up Next, resume position, mark played, bookmarks, RSS subscribe, OPML import/export, feed refresh, downloads, auto-delete, filters, and sort modes.
- Discovery: Apple Podcasts directory search, Podcast Index search, in-library keyword search, and in-show search.
- Sync: subscriptions, playback position, and played state across devices.
- Accessibility: Dynamic Type, VoiceOver, high contrast, reduced motion, reduced transparency, and transcript rendering.
- Sharing: episode links, timestamp links, Shortcuts, Siri, Live Activity, and widgets.
- Privacy: local-first listening data, clear data delete, per-show cache clear, analytics opt-out, and honest App Store privacy declarations.

## Product Interpretation

The AI and wiki features can be scoped for early versions, but the player cannot feel incomplete. A user may forgive a first version of cross-episode synthesis; they will not forgive unreliable playback, missing background audio, poor queue controls, or inaccessible controls.

## See Also

- [[product-vision|Product Vision]] ([Product Vision](../topics/product-vision.md)) - why the floor matters to the product promise.
- [[capability-map|Capability Map]] ([Capability Map](../topics/capability-map.md)) - how table-stakes capability fits with agent differentiation.

## Sources

- [Repo spec source map](../../raw/notes/2026-05-09-repo-spec-sources.md)
