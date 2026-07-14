---
title: "Ambient And Accessibility Surfaces"
category: concepts
sources:
  - raw/notes/2026-05-09-experience-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [ambient, accessibility, carplay, voice, widgets]
aliases: [Ambient Surfaces, Accessibility Surfaces]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Podcastr's core workflows must extend to lock screen, widgets, CarPlay, Siri, AirPods, VoiceOver, Dynamic Type, and reduced motion settings."
---

# Ambient And Accessibility Surfaces

Podcast apps are often used when the phone is not the center of attention. Podcastr's agent features must work in the same contexts as playback.

## Ambient Surfaces

- Lock Screen and Now Playing controls.
- Dynamic Island and Live Activity.
- Widgets for Now Playing, Up Next, and briefing.
- App Shortcuts for voice mode, resume playback, and start briefing.
- CarPlay surfaces for library, queue, and safe voice entry.
- AirPods and Action Button setup through Shortcuts.
- Notifications for new episodes, ready transcripts, generated briefings, and approved agent actions.

## Accessibility Surface Area

- Full Dynamic Type support.
- VoiceOver labels and action order for player, queue, citations, and agent action cards.
- Reduced motion alternatives for cinematic transitions.
- Reduced transparency alternatives for Liquid Glass surfaces.
- High contrast and color-independent state.
- Transcript and caption rendering with speaker labels.
- One-handed operation for common playback and voice actions.

## Design Rule

If a feature only works in a beautiful full-screen view, it is not done. The product promise has to survive screen-off listening, CarPlay, VoiceOver, and brief glance interactions.

## See Also

- [[experience-north-star|Experience North Star]] ([Experience North Star](../topics/experience-north-star.md)) - audio-first UX principles.
- [[voice-briefing-loop|Voice Briefing Loop]] ([Voice Briefing Loop](../../../agent/wiki/concepts/voice-briefing-loop.md)) - voice behavior in ambient contexts.
- [[launch-floor|Launch Floor]] ([Launch Floor](../../../product/wiki/references/launch-floor.md)) - baseline platform requirements.

## Sources

- [Experience source map](../../raw/notes/2026-05-09-experience-source-map.md)
