---
title: "Voice Briefing Loop"
category: concepts
sources:
  - raw/notes/2026-05-09-agent-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [voice, briefing, stt, tts, barge-in]
aliases: [Interruptible Briefing Loop, Voice Mode]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Voice mode combines local streaming STT, tool-calling agent turns, low-latency TTS, and briefing resume semantics."
---

# Voice Briefing Loop

The marquee voice experience is an audio briefing that the user can interrupt, question, and resume. This is more demanding than a normal push-to-talk assistant because the app is already playing audio when the user speaks.

## State Flow

1. Briefing or episode audio is playing.
2. User triggers voice mode or starts speaking during an active briefing.
3. The app ducks or pauses playback and opens the mic.
4. STT streams partials to the agent loop.
5. The agent retrieves wiki or transcript context through tools.
6. TTS begins as soon as a stable reply segment exists.
7. The original briefing resumes at the nearest stable beat.

## Requirements

- One audio-session coordinator owns all `AVAudioSession` transitions.
- Voice mode should prefer on-device STT where available.
- Cloud TTS can be used for quality, with on-device fallback.
- Barge-in detection must avoid triggering on the agent's own TTS.
- Generated briefings need script anchors so resume points are stable.

## See Also

- [[tool-surface|Tool Surface]] ([Tool Surface](tool-surface.md)) - tools used during a voice turn.
- [[agent-runtime-and-context|Agent Runtime And Context]] ([Agent Runtime And Context](../topics/agent-runtime-and-context.md)) - shared loop behind voice and text.
- [[experience-north-star|Experience North Star]] ([Experience North Star](../../../experience/wiki/topics/experience-north-star.md)) - why voice must feel calm and polished.

## Sources

- [Agent source map](../../raw/notes/2026-05-09-agent-source-map.md)
