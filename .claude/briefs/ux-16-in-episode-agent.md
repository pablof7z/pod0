# UX-16 — In-Episode Voice Drop

> Owner: Designer agent. Scope: context-aware agent invocation from within the Now Playing screen while an episode is actively playing. Out of scope: full Voice Mode orb (UX-06), Agent Chat thread (UX-05), Briefing barge-in (UX-08), bookmarks without agent action.

---

## 1. Vision

The in-episode agent is a **one-tap thought-catcher with agency**. The user is deep in an episode — walking, driving, dishes — and a thought fires. Today they pause, fumble open a notes app, type something half-baked, and either lose the context or never act on it. Podcastr collapses that entire gap into a single tap: the agent chip glows, the episode ducks, the user speaks, the agent acts — and the episode picks back up. The user never looked at the screen.

Three premises:

- **Context is already there.** The agent receives the current transcript window (≈ 90 s back, 10 s forward if available) plus the user's speech. It knows exactly what was just said, who said it, and what the active topic thread is.
- **Actions, not notes.** A voice drop is not a dictation buffer. The agent interprets intent and executes: seek, clip, annotate, research. The user doesn't need to know which; they just speak naturally.
- **Never leaves the player.** Results — a clip card, a seek confirmation, a research thread, an anchored note — surface as overlays or banners in the Now Playing screen. No navigation, no modal push.

---

## 2. Entry Points

| Trigger | Context |
|---------|---------|
| Tap the agent chip (lower right, copper glow) | Primary. Works in both artwork-dominant and transcript-focus modes. |
| AirPods long-press (if Voice Mode permission granted) | Hands-free. Same flow, no screen interaction required. |
| Action Button shortcut (opt-in) | Power users; labeled "In-Episode Agent" in Settings → Action Button. |
| Double-tap a transcript sentence | Opens the in-episode agent **pre-seeded** with that sentence as context anchor. |

The standard Voice Mode (UX-06) mic button on the Ask tab is a different entry; it has no episode-position context and is not the same feature.

---

## 3. Audio Session Behavior

On entry:
- Episode volume ducks to **−18 dB** (not paused — the waveform and transcript keep scrolling so the user maintains their mental position).
- Agent chip animates to a listening lens (same material language as UX-06 orb, miniaturised to 44 pt).
- A **soft chime (in-ear only)** signals recording has started — no UI banner needed.

On speech endpoint detection (VAD):
- Episode volume held at −18 dB while agent processes (< 2 s target, capped at 5 s before fallback banner).
- If a tool call fires (seek, clip, etc.), a **light haptic burst** fires on completion.
- Episode un-ducks with a 300 ms fade to full volume.

User can abandon at any time via:
- Tap chip again → dismisses, episode immediately un-ducks.
- Swipe down on the mini orb → same.
- Silence > 4 s with no speech detected → auto-dismiss with a softer haptic.

---

## 4. Agent Tools Available in This Mode

The agent loop is scoped — only these tools are active when invoked from the in-episode context. No full library-wide synthesis or briefing generation from this surface.

| Tool | What it does | Example trigger |
|------|-------------|-----------------|
| `seek_to_topic_start` | Locates the transcript sentence where the current active topic or named concept was first introduced in this episode; seeks to that timestamp. Topic detection uses the most recent topic-shift heuristic (silence + speaker turn + TF-IDF delta). | *"I didn't follow that, go back to where they started talking about this"* |
| `create_clip_semantic` | Determines in/out timestamps based on topic/sentence boundaries around the current position (±60 s window); creates a `ClipCard` artifact surfaced in-player. Default: start = current topic boundary, end = next silence after current position. | *"clip that"*, *"save that part"*, *"that was good"* |
| `anchor_note` | Drops a timestamped text annotation on the episode at the current playhead position. Body is the agent's distillation of user intent + transcript context (not a verbatim transcription). Visible in Episode Detail (UX-03) transcript view. | *"interesting — note this"*, *"remind me to look this up"* |
| `research_inline` | Fires a lightweight async research thread. On completion (typically 10–20 s), surfaces a glass banner in the Now Playing screen with a 2–3 sentence answer + sources. Does not interrupt playback. Draws from: episode transcript, per-podcast wiki (UX-04), library-wide wiki, and optionally Perplexity (BYOK). | *"I wonder how this would apply to X"*, *"wait, what is [term]?"*, *"is that actually true?"* |

Intent classification runs locally (lightweight on-device classifier + LLM fallback) before tool dispatch. If confidence < 0.7, agent asks one clarifying question via TTS (short; ≤ 8 words) before acting.

---

## 5. Result Surfaces (Still In Player)

Results never push navigation. They appear as:

- **Seek confirmation pill** — a copper "↩ Rewound to 14:22 · Topic: mitochondrial efficiency" pill slides up from the transport row; auto-dismisses after 5 s. Tap to undo.
- **Clip card** — a glass waveform card slides up from the bottom edge at 30 % screen height. Shows in/out handles, speaker label, play button. Action row: *Keep · Edit · Share · Discard*. Tapping anywhere outside dismisses but keeps the clip.
- **Anchored note** — a small copper dot appears on the waveform scrubber at the playhead timestamp; a 2-line banner slides up confirming the note text. Full note visible in Episode Detail.
- **Research banner** — when `research_inline` completes, a dismissible glass strip appears above the transport row: eyebrow "Agent · 23 s ago", 2–3 sentence answer, sources row (show-name chips, tap to cite-jump). Persists until dismissed. If the user is in transcript-focus mode, the banner animates in from the left edge.

---

## 6. Microinteraction Detail

| Moment | Animation / Haptic |
|--------|-------------------|
| Chip tap → listening | Chip scales 1.0 → 1.15 with a spring; copper glow pulses at 1.2 Hz; episode ducks over 200 ms |
| VAD speech detected | Chip waveform rings appear (concentric, 3 rings, expand to 40 pt) |
| Processing | Chip switches to spinner ring; rings dissolve; `.selectionChanged` haptic |
| Tool completes | Light haptic burst (`.impactOccurred(intensity: 0.6)`); result surface animates in |
| Abandon / dismiss | Chip collapses to resting state over 300 ms; episode un-ducks; no haptic |
| Seek fires | Scrubber jumps + copper flash on waveform at destination; `.rigid` haptic |

---

## 7. Privacy Contract

- Mic is active **only during the gesture window** (tap-to-release or VAD endpoint). Never ambient during regular playback.
- Audio is streamed to the cloud STT provider only after on-device VAD confirms speech (not just noise).
- The transcript window passed to the agent is the episode's own transcript — no ambient audio is captured or sent.
- A persistent **orange mic indicator** follows iOS system rules during recording window.

---

## 8. Failure Modes

| Failure | UX |
|---------|-----|
| No transcript available for this episode | Chip disabled; long-press shows tooltip *"Transcript needed for in-episode agent — transcribing"* with progress. |
| STT failed / no speech detected | Soft chime (error tone), episode un-ducks, no banner. |
| Agent tool fails | "Couldn't do that right now" TTS whisper + dismiss; episode continues. |
| Research takes > 10 s | Subtle spinner in the corner of the player; banner arrives whenever ready (no blocking). |
| User is in CarPlay | In-episode agent is accessible via PTT button only; all result surfaces degrade to TTS confirmation only (no visual cards). |

---

## 9. Open Questions

- **Clip review flow**: should `create_clip_semantic` immediately open the clip editor or default to a "saved, review later" pattern? Current default: saved with quick-confirm banner; editor accessible via tap. Revisit after user testing.
- **research_inline depth**: one-shot RAG vs. full multi-step agent loop? Current: one-shot RAG for speed; multi-step if user explicitly says "deep dive". To be validated.
- **Note body**: agent-distilled vs. verbatim user speech? Current: agent-distilled (cleaner, but loses exact wording). Consider offering both.
