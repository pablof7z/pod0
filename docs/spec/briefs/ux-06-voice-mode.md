# UX-06 — Voice Conversational Mode

> Owner: Designer agent. Scope: voice input/output for the embedded podcast agent. Out of scope: text chat (UX-05), now-playing chrome (UX-01), TLDR briefing player chrome (UX-08 — collaboration only), CarPlay/Watch shells (UX-11 — collaboration only).

---

## 1. Vision

Voice mode is the feature that should make a stranger gasp the first time they use the app. The product promise — *"talk to all of your podcasts as if they were one continuous conversation"* — collapses if the conversational layer feels like Siri-with-a-paint-job. It must feel like a calm, intelligent companion who is *already listening* and *gracefully steps aside* the moment you speak.

The marquee moment is **barge-in mid-briefing**. The agent is reading you a 12-minute TLDR. You say *"Wait — who was that guest?"* and within ~150 ms three things happen, beautifully:

1. The agent's voice **ducks and dissolves** mid-syllable (no abrupt cut).
2. Its glass orb **inhales** — collapses from a speaking bloom into a small, listening lens that pulses with your voice.
3. The briefing's now-playing card **dims and recedes one z-layer**, signaling "I'm holding your place."

When the answer is delivered, the orb **exhales** and the briefing card swims back forward. That handoff — never fumbled, never doubled-up, never accidentally talking over you — is the entire feature. Everything else in this brief exists to protect that 800 ms.

Three principles:

- **The agent is a guest in your ear.** It defers. Always.
- **Glass breathes.** The orb is a single living material whose state is legible without reading text.
- **Audio first, screen optional.** Every voice interaction must work blind, one-handed, in a car, with AirPods only.

---

## 2. Key User Moments

1. **Hands-free entry while walking with AirPods.** Squeeze stem → faint chime → orb wakes on lock screen → user speaks → answer plays → orb sleeps. Screen never required.
2. **Ask while an episode is playing.** Long-press the mini-player → episode ducks (-12 dB) → user asks *"who's the woman they're quoting?"* → agent answers in 3–4 seconds → episode un-ducks. Episode never pauses.
3. **Interrupt a TLDR briefing.** The marquee moment described in §1. The user does not need to press anything — they just speak. App is in *Ambient mode* during briefings only.
4. **Follow-up question.** After an answer, the orb stays awake for ~3 s with a soft "still here" pulse. Speak again with no gesture; silence dismisses gracefully.
5. **Exit gracefully.** Swipe down on the orb, say "thanks" / "that's it", or simply stop talking. The orb dissolves into the playback surface; the episode/briefing resumes at full volume with a 400 ms fade.

---

## 3. Information Architecture

**Two voice modes, one visual language.**

| Mode | Trigger | Listening posture | Cost / privacy |
|---|---|---|---|
| **Push-to-talk (PTT)** | Action button, AirPods squeeze, long-press mini-player, lock-screen orb tap, "Hey, podcast" hot-phrase (opt-in) | Listens *only* during gesture / hot-phrase window | On-device VAD; cloud STT with explicit start |
| **Ambient (barge-in)** | Activated *automatically* whenever the agent is speaking (briefing or answer playback) | Listens for the duration of agent TTS only | On-device VAD continuously; cloud STT only fires once VAD + endpoint detector decide it's speech, not cough/road noise |

Ambient mode is **not** an always-on home-screen mic. It only exists during the windows when the agent itself holds the audio session. This is the privacy contract.

**State machine** (single source of truth, drives the orb):

```
        ┌──────── idle ────────┐
        │                       │
  gesture/squeeze            agent_tts_start
        ▼                       ▼
    listening ──vad_speech── listening (ambient)
        │                       │
   endpoint                 endpoint
        ▼                       ▼
    transcribing ────────► transcribing
        │
   transcript_final
        ▼
    thinking ──tool_call──► thinking (with tool chip)
        │
    first_token
        ▼
    speaking ◄──────── interrupt ──────── listening
        │
    tts_complete
        ▼
    follow_up_window (3s) ─silence→ idle
                            └─speech→ listening
```

**Handoff rules.** Only one of {episode audio, briefing TTS, agent answer TTS, user listening} owns the audio session. Transitions cross-fade (200–400 ms). Episode audio is *ducked* (not paused) during PTT answers under 8 s; *paused* for answers expected to exceed 8 s (predicted from token budget). Briefings are always paused on barge-in and resumed on exit, with a "rewinding 2 s for context" microcopy.

---

## 4. Visual Treatment

**The orb.** A single 88 pt liquid-glass sphere, rendered with `GlassEffectContainer` + `.glassEffect(.regular.interactive(), in: .circle)`, tinted with a slow-shifting gradient sampled from the currently playing episode artwork. It lives at the bottom-center safe area when active; collapses into the mini-player when idle.

Orb morphology by state:

- **Idle** → not present. The mini-player owns the surface.
- **Listening** → 64 pt lens-shaped glass disk. Surface ripples in response to mic input level (Metal shader displacement, 60 fps, amplitude clamped). Tint cools toward iOS system blue.
- **Transcribing** → orb holds shape; partial transcript streams into a caption strip *above* it, monospaced for stability, then settles into SF Pro Text once finalized.
- **Thinking** → orb compresses to 56 pt, internal light source rotates slowly (1 rotation / 1.6 s). When a tool fires (`query_transcripts`, `play_episode_at`, etc.), a small glass chip morphs out of the orb's perimeter — `glassEffectUnion` keeps it visually attached — labeled *"Searching transcripts…"*, *"Opening Tim Ferriss Ep 712…"*. Chips dissolve back in on completion.
- **Speaking** → orb blooms to 96 pt and gains a slow **breath rhythm**: scale 1.00 → 1.04 → 1.00 over 2.4 s, eased with a custom spring (response 1.2, damping 0.85). Caption text streams beneath at TTS-synchronized rate.
- **Barge-in detected** → in ≤120 ms: TTS ducks 18 dB, orb scale snaps to 0.7, tint cools to listening blue, breath rhythm halts, ripples activate. This snap is the only "sharp" motion in the system — every other transition is spring-based.

**Captions.** Always available. Default visible during PTT answers; default hidden during ambient mode (toggleable). Two tracks: the user's transcribed prompt (top, dimmed) and the agent's spoken answer (bottom, primary). Type ramp: 17 pt SF Pro Text Regular, 22 pt line-height, max 3 lines visible with vertical fade mask.

**Materials.** Orb sits on a 24 pt blur backdrop that *only* appears during voice mode, leaving the underlying playback UI legible at ~40% effective contrast. Backdrop fades in over 300 ms on entry, out over 500 ms on exit.

**Motion language.**

| Transition | Curve | Duration |
|---|---|---|
| idle → listening | spring(response: 0.4, damping: 0.75) | ~450 ms |
| listening → thinking | spring(response: 0.5, damping: 0.9) | ~500 ms |
| thinking → speaking | spring(response: 0.6, damping: 0.7), bloom | ~600 ms |
| speaking → listening (barge-in) | linear ramp on audio + snap-spring(response: 0.25, damping: 0.85) on visual | ≤120 ms perceived |
| any → idle | spring(response: 0.55, damping: 0.9) + opacity fade | ~500 ms |

---

## 5. Microinteractions

- **Interrupt detection visual.** The orb's edge develops a faint rim-light the instant VAD triggers, *before* STT confirms. If VAD turns out to be a false positive (cough, road noise), the rim fades and TTS resumes at full volume — no perceived stutter. This optimistic preview is the secret to the magical feel.
- **Tool-call chip.** Small glass pill emerging from the orb perimeter via `glassEffectUnion`, e.g. *"Searching across 412 episodes…"*. Caps at one chip at a time; subsequent tools queue and rotate text. Chip is tappable post-answer to peek at the source.
- **Agent breath rhythm.** While speaking, orb scale modulates with a 2.4 s sine curve. Breath halts on barge-in — visually echoing a person stopping mid-sentence to listen.
- **Listening waveform.** Mic input drives a Metal displacement shader on the orb surface — *not* a literal waveform UI. We do not show a separate waveform graph; the orb *is* the waveform.
- **Follow-up "still here" pulse.** Orb expands and contracts once over 600 ms, then dims to 60% for 3 s. Speaking resumes the loop; silence collapses to idle.
- **Source peek.** When the agent answers from a specific episode, a small glass card slides up from below the orb showing artwork + episode title + the cited timestamp. Tap → opens the player at that timestamp via `play_episode_at`.

---

## 6. ASCII Wireframes

### A. Entry (lock screen, AirPods squeeze)

```
┌─────────────────────────────────┐
│  9:41                           │
│                                 │
│       ╭───────────────╮         │
│       │  ░░ Tim 712 ░░│  ← now playing card
│       ╰───────────────╯         │
│                                 │
│                                 │
│              ◯                  │  ← orb wakes, 64pt
│         ╭─────────╮             │
│         │ Listening│  caption   │
│         ╰─────────╯             │
│                                 │
│   ▮ swipe down to dismiss ▮     │
└─────────────────────────────────┘
```

### B. Listening (in-app, episode ducked)

```
┌─────────────────────────────────┐
│ ◀ Library            ⌕    ⋯     │
│                                 │
│  [ episode artwork — dimmed 40%]│
│                                 │
│  Episode title (dimmed)         │
│  ····················           │
│                                 │
│         ░░░░ ◉ ░░░░             │  ← orb, ripples
│   ╭─────────────────────╮       │
│   │ "who's the woman    │       │  ← live partial transcript
│   │  they're quoting…"  │       │
│   ╰─────────────────────╯       │
│                                 │
│  ▶ episode ducked  •  ✕ cancel  │
└─────────────────────────────────┘
```

### C. Thinking with tool chip

```
┌─────────────────────────────────┐
│                                 │
│           ░░░░◯░░░░             │  ← orb compressed, light rotates
│         ╱           ╲           │
│        ╱  Searching  ╲          │  ← glass chip morphed from orb
│        ╲  transcripts ╱         │
│         ╲___________╱           │
│                                 │
│   "who's the woman they're      │  ← user prompt, dimmed
│    quoting?"                    │
│                                 │
└─────────────────────────────────┘
```

### D. Speaking with caption + source peek

```
┌─────────────────────────────────┐
│                                 │
│          ░░░░░◉░░░░░            │  ← orb bloomed, breathing
│                                 │
│  ╭─────────────────────────╮    │
│  │ "She's quoting Lyn      │    │  ← TTS-synced caption
│  │  Alden, a macroeconomic │    │
│  │  analyst — at 23:14."   │    │
│  ╰─────────────────────────╯    │
│                                 │
│  ╭───────────────────────────╮  │
│  │ ▢  Tim 712 — 23:14   ▶︎   │  │  ← source peek card
│  │    "Lyn Alden on debt"    │  │
│  ╰───────────────────────────╯  │
│                                 │
│  ◐ mute   ⌨ switch to text   ✕  │
└─────────────────────────────────┘
```

### E. Barge-in mid-briefing (the marquee moment)

```
   BEFORE (briefing speaking)         AFTER (~120ms later, user spoke)
┌─────────────────────────────────┐  ┌─────────────────────────────────┐
│  TLDR — This week (4 of 9)      │  │  TLDR — This week (4 of 9)      │
│                                 │  │      ·· paused at 04:12 ··      │  ← dimmed, recedes
│         ░░░░░◉░░░░░             │  │                                 │
│        (breathing)              │  │         ░░░░ ◉ ░░░░             │  ← orb collapsed
│                                 │  │            (ripples)            │
│  "…and over in tech, the big   │  │                                 │
│   story this week was…"         │  │   "wait — who was that guest?" │  ← partial transcript
│                                 │  │                                 │
└─────────────────────────────────┘  └─────────────────────────────────┘
```

### F. Voice + episode card visible (post-answer)

```
┌─────────────────────────────────┐
│ ◀ Briefing                      │
│                                 │
│  ╭─ Source ───────────────────╮ │
│  │ ▢ Tim Ferriss #712         │ │  ← peek persists for 6s
│  │   23:14 — "Lyn Alden…"  ▶︎ │ │
│  ╰────────────────────────────╯ │
│                                 │
│              ◌                  │  ← orb idling at 60%, "still here"
│                                 │
│  ▶ resuming briefing in 1s…     │
└─────────────────────────────────┘
```

---

## 7. Edge Cases

- **STT misheard.** Show transcribed prompt above the answer with a small *"not what I said"* affordance (single-tap → re-listen, no full reset). After 2 corrections in a session, offer "switch to text" inline.
- **Very long answer.** Cap spoken answer at ~45 s. After 30 s of TTS, show *"continue speaking / show full answer"* affordance. Long answers default to a generated card the user can read while the orb summarizes the headline aloud.
- **User goes silent in listening state.** 1.5 s of silence after VAD endpoint → transcribe. 4 s of silence with no VAD trigger → orb fades and apologizes briefly: *"Didn't catch that."* No more than once per session.
- **Ambient noise / road / café.** Raise VAD threshold dynamically based on a 2 s rolling noise floor. In CarPlay, prefer explicit PTT (steering wheel button) over ambient barge-in to avoid false positives at highway speed.
- **Bluetooth device contention.** If user switches output mid-answer (AirPods → car), pause TTS, show *"resuming on \[device]"* toast, fade back in over 600 ms.
- **Mid-call interruption (phone rings).** Voice mode immediately yields the audio session, orb dissolves, app retains full state. On call end, surface a single banner: *"Resume where we left off?"* — no auto-resume.
- **Two devices listening (iPhone + Watch).** Hot-phrase / squeeze on Watch routes through the phone's agent. Orb appears on whichever screen is active; the other shows a glass dot indicator.

---

## 8. Accessibility

- **Captions are non-negotiable.** Every spoken word, both directions, has a synchronized caption. User can pin captions on permanently in Settings → Voice → Always show captions.
- **Hearing-impaired flow.** Voice mode can be configured to *display* the agent's response without TTS playback ("silent voice mode"): captions only, full-screen, no audio out. Prompt input still accepts voice OR keyboard.
- **VoiceOver.** Orb is a single accessibility element with a live state label: *"Listening", "Thinking, searching transcripts", "Speaking, 1 of 3 sentences"*. Captions are a separate live region announced once per agent utterance.
- **Reduced Motion.** Disables breath rhythm, ripple shader, and bloom; replaces with a 0.6 s opacity cross-fade between states. Tool chips become static text.
- **Reduced Transparency.** Orb material falls back to a solid system-tinted disc with rim stroke; backdrop blur replaced by 60% opaque scrim. State legibility preserved.
- **Color-blind safety.** State is communicated by *shape and motion*, not color. Tint is decorative.
- **One-handed / driving.** All voice mode actions are reachable from the bottom 30% of the screen. PTT works through the Action button, AirPods squeeze, lock-screen orb, and CarPlay steering control.
- **Alternative input.** "Switch to text" affordance always visible during voice mode → preserves conversation context, hands keyboard to UX-05 chat.
- **CarPlay.** No barge-in by default (false-positive risk in cars). PTT only via steering wheel + Siri-style alert tone. Captions render as large glass strips on the CarPlay surface; orb collapses to a single status pill in the status bar.

---

## 9. Open Questions / Risks

1. **Always-on listening privacy.** Even constrained to "during agent TTS only," this is a sensitive contract. Required: local-only VAD, an unmistakable on-screen indicator (the orb itself), an opt-out per briefing, and a Privacy disclosure equivalent to dictation. **Recommend** App Privacy Report-style transparency listing every STT invocation.
2. **On-device vs cloud STT.** On-device (`SFSpeechRecognizer` with `requiresOnDeviceRecognition = true`) is mandatory for the *barge-in detection path* — latency budget is ≤120 ms and we cannot wait for a network round trip. Final transcription for thinking can use a cloud model (ElevenLabs Scribe, Whisper) for accuracy. **Decision needed:** dual-pipeline complexity vs single on-device pipeline.
3. **Battery cost of ambient VAD.** VAD during a 12-minute briefing = ~12 minutes of low-power mic + Metal shader. Needs a measured budget; suspect ≤2% per hour but unverified.
4. **TTS voice identity.** The agent has *one* voice across briefings, answers, and the orb. Choice of voice (ElevenLabs custom? Apple Neural? cloned?) is a brand decision out of scope here, but locked-in before launch.
5. **False barge-in rate.** The single biggest risk. If the orb interrupts itself for coughs or laughs, the magic dies. Need a tunable confidence threshold + a forgiveness behavior (resume TTS within 400 ms if no STT confirmation arrives) — see §5 *interrupt detection visual*.
6. **Wake-word vs gesture.** Recommend launching gesture-only ("Hey, podcast" hot-phrase deferred to v1.1). Hot-phrases invite parody and false triggers in podcast audio itself ("Hey, Tim, what about…").
7. **Source peek dismissal.** Should it auto-dismiss after 6 s, or persist until next answer? **Recommend** persist within voice session, dismiss on session end.
8. **Multi-turn memory horizon.** How many turns does the agent retain context for in a voice session? Suggest 8 turns or 5 minutes idle, whichever first — coordinate with UX-05.

---

File path: `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-06-voice-mode.md`
