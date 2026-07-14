---
title: "In-Episode Agent"
category: concepts
sources:
  - ux-brief/ux-16-in-episode-agent.md
created: 2026-05-09
updated: 2026-05-09
tags: [agent, voice, now-playing, clip, seek, notes, research]
aliases: [Voice Drop, In-Episode Voice Drop]
confidence: high
volatility: warm
verified: 2026-05-09
summary: "A one-tap voice drop from within the Now Playing screen that gives the agent full transcript context and a scoped toolset — seek, clip, annotate, or research — without leaving the player."
---

# In-Episode Agent

The in-episode agent is a **context-aware action layer** surfaced through the agent chip in Now Playing. While an episode is playing the user taps (or long-presses AirPods), speaks one sentence of intent, and the agent acts — without navigation, without pausing, without the user knowing which tool fires.

## Why It Exists

Standard voice mode (UX-06) and agent chat (UX-05) are powerful but contextless with respect to a currently-playing episode. The in-episode agent is the missing layer: it holds the current transcript window, speaker identities, and active topic thread as implicit input, so the user never needs to explain *what they were just listening to*.

## Context Window Passed to the Agent

On activation the agent receives:
- **Transcript look-back**: ≈90 s of diarized transcript ending at the current playhead.
- **Transcript look-ahead**: ≈10 s if already transcribed (often available via streaming transcription).
- **Episode metadata**: show name, episode title, current timestamp, total duration, chapter (if any).
- **Active topic label**: output of the last topic-shift heuristic run (updated every ~15 s during playback).
- **User speech**: the transcribed voice drop, provided as a follow-up turn in the agent conversation.

## Intent → Tool Mapping

The agent classifies intent from the user's speech and fires one tool. An on-device classifier handles the common cases; the LLM handles ambiguous or compound requests.

| User says (examples) | Classified intent | Tool fired |
|----------------------|------------------|-----------|
| *"I didn't follow that, go back to where this started"*, *"rewind to the beginning of this topic"* | `seek_to_topic` | `seek_to_topic_start()` |
| *"clip that"*, *"save that part"*, *"oh that was good"* | `clip` | `create_clip_semantic()` |
| *"note this"*, *"remind me to look this up"*, *"interesting idea"* | `annotate` | `anchor_note(body)` |
| *"I wonder how X applies to Y"*, *"wait, what is [term]?"*, *"is that actually true?"* | `research` | `research_inline(query)` |

If intent confidence < 0.7, the agent asks a single clarifying question (≤8 words) before acting.

## Result Surfaces

All results surface within the Now Playing screen:

- **Seek**: copper "↩ Rewound to HH:MM · Topic: [label]" pill on the transport row; tap to undo.
- **Clip**: glass waveform card at 30 % screen height with in/out handles; action row: Keep · Edit · Share · Discard.
- **Note**: copper dot on the waveform scrubber at the timestamp; 2-line confirmation banner.
- **Research**: async glass strip above the transport row when ready; 2–3 sentence answer + source chips.

## Audio Session

Episode ducks to −18 dB (not paused) on entry. Un-ducks with 300 ms fade on tool completion or abandonment. Ambient barge-in is **off** during regular episode playback — this mode is PTT-only. (Ambient barge-in applies only during agent TTS output per the UX-06 contract.)

## Privacy

- Mic is active only during the gesture window; no ambient capture.
- Only the episode's own transcript is sent to the LLM — no ambient audio.
- Orange system mic indicator follows iOS rules.

## See Also

- [[tool-surface|Tool Surface]] ([Tool Surface](tool-surface.md)) — full scoped tool list for this mode.
- [[voice-briefing-loop|Voice and Briefing Loop]] ([Voice and Briefing Loop](voice-briefing-loop.md)) — UX-06 voice mode this complements.
- [UX-16 brief](../../../../../spec/briefs/ux-16-in-episode-agent.md) — full microinteraction tables, failure modes, and open questions.
