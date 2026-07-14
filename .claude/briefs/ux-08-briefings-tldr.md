# UX 08 — AI Briefings / TLDR Player

> A briefing should not feel like a podcast you happened to receive. It should feel like a podcast that was **made for you, this morning, by someone who has been listening on your behalf.**

---

## 1. Vision

The Briefings surface is where the agent stops being a search box and becomes a *producer*. A briefing is a synthesized audio episode: TTS narration stitched with original-audio quotes, structured as segments, framed by cinematic stings, and **interruptible at any point** by voice — it listens back.

Visceral target: *NPR's All Things Considered crossed with The Atlantic's print layout.* Where now-playing (UX 1) frames someone else's recording, the briefing player is *the agent's stage*. The user must know at a glance — without a label — that this audio was made for them. That is the entire visual challenge.

Briefings earn trust: every claim carries an attribution chip, every quote a "go to source," length and scope are the user's choice. Trust through legibility. Magic through restraint.

---

## 2. Key User Moments

1. **Morning routine** — User sees a Liquid Glass card: *"Tuesday briefing — 8 min — drawn from 7 episodes."* Tap; intro sting plays before the lock screen resolves. By segment two they've heard the only news that matters today.
2. **"Catch me up on X"** — Off Lex Fridman for six weeks. Long-press the show, *Brief me*, slide the length puck to 20 min, go. Ninety seconds later an episode-shaped object lands in the library tagged *Briefing*.
3. **Branch via voice** — Agent says *"…and Sundar mentioned a new TPU."* User: *"Wait — what's the headline number?"* Audio ducks, agent answers in 6 seconds, briefing resumes from the exact word it dropped. Breadcrumb in the rail.
4. **Share** — Three-finger tap on the Ozempic segment composes a share card: generative cover, 90-second extract, citations. Off to iMessage.
5. **Scheduled drop** — *Every weekday 6:45am, 8 min, my subs only.* A Liquid Glass tile lands on the lock screen, pre-generated, ready on tap. (#14 owns scheduling logic; this surface owns presentation.)

---

## 3. Information Architecture

**Three surfaces:**

- **Compose** — preset row (Daily, Weekly, Catch-up on…, Topic deep-dive), freeform *"Brief me on…"* field, length puck (3 / 8 / 15 / 25 min), scope chips (My subs / This show / This topic / This week), pinned recents.
- **Player** — **transcript, chapter list, and segment rail are the same surface in three densities**, not three UIs. Collapsed: horizontal glass strip. Up: chapter list with attribution chips. Up again: full live-transcript auto-scrolling with playback.
- **Library shelf** — dedicated, tint-segregated (§4) so a briefing is never mistaken for an episode. Filter by date, scope, length.

**Briefing object model:**
```
Briefing
 ├─ intro sting
 ├─ Segment[] (title · TTS body · original-audio quotes? · sources[])
 │    └─ Branch[] (forks; Briefing-shaped sub-objects with breadcrumb back)
 ├─ outro sting
 └─ metadata: scope, length_target, generated_at, sources[]
```

**Branch contract: pause-and-resume**, not fork-and-replace. The main thread freezes at the sample the user spoke over; the branch plays as a parenthetical; on completion or *back*, main resumes from that sample. Branches persist and resurface on re-listen as optional side-paths in the rail.

---

## 4. Visual Treatment

A briefing must be **immediately distinguishable** from an episode while still feeling like part of the player family.

**Material.** Now-playing uses neutral glass over episode artwork. Briefings use a **warm-tinted variant** — `glassEffect(.regular.tint(brassAmber.opacity(0.18)).interactive(), in: .rect(cornerRadius: 28))` — over a slow-drifting generative gradient (warm ink, brass, parchment). Brass-amber glass = the agent owns this audio.

**Typography.** Episode titles use the system display font. Briefing titles use *New York Large* at 34pt with a dropcap on each segment's leading word; transcript bodies in the same serif at 17/26 with hanging punctuation; chips small-caps, tracked +40, muted ink.

**Cinematic intro/outro.** Two-second open: hairline rule draws edge-to-edge, title fades in as the sting plays, rail crystallizes from the rule (the rule *is* the rail's spine). Outro reverses to a point, fades.

**Segment transitions — the signature motion.** `glassEffectID` morphing inside a `GlassEffectContainer`: the active pill morphs into the next while a thin gradient ribbon sweeps under the title (250ms, cubic-bezier 0.2, 0.8, 0.2, 1.0); a waveform glyph re-forms at the new title. No fade-cuts — always morphs.

**Voice barge-in (UX 6 handoff).** Audio ducks 12 dB. Glass deepens tint with an inner glow on its leading edge — *listening*. Segment title freezes mid-word, italicized. The agent's answer lifts as a second glass card above the rail; on resume it morphs back as a branch crumb.

**Glass map.** Chrome → tinted. Rail → tinted strip with `glassEffectUnion` linking active/next pills. Branch crumbs → smaller untinted chips. Share card → prominent tile. All inside one `GlassEffectContainer(spacing: 24)`.

---

## 5. Microinteractions

- **Segment navigate.** Horizontal swipe on the rail jumps to next/previous segment.
- **Skip-and-stitch.** Swipe-down on the active card drops it; agent re-stitches with a half-second crossfade.
- **Go deeper (tap-to-branch).** A *↳ deeper* glyph on the active card (or long-press a rail segment) triggers the pause-and-resume branch contract with a typed prompt sheet.
- **"Make it shorter" pinch.** Pinch-in; pill *"Re-narrate at half length?"* regenerates remaining segments only.
- **Hold-to-pause-and-ask.** Long-press chrome; audio ducks, mic glyph fades in; release sends the question to UX 6.
- **Branch-and-return.** A *Return to briefing* chip persists with a 4s auto-resume ring. Tap to resume; swipe down to stay.
- **Share.** Tap a rail segment to select; share glyph reveals on its card.
- **Transcript scrub** drags the playhead.

---

## 6. ASCII Wireframes

### W1 — Compose

```
┌──────────────────────────────────────────┐
│  ◀                              Compose  │
│                                          │
│   Brief me on…                           │
│  ┌────────────────────────────────────┐  │
│  │  what's been said about Ozempic ▏ │  │  ← freeform, large serif
│  └────────────────────────────────────┘  │
│                                          │
│   Length                                 │
│   ●━━━━━━━○━━━━━━━━━━━━━━━━━━━━━━━━━     │
│   3      8       15           25 min     │
│                                          │
│   Scope                                  │
│   [ My subs ] [ This show ] [ Topic ]    │
│                                          │
│   Quick presets                          │
│   ┌──────────┐ ┌──────────┐ ┌─────────┐  │
│   │ Daily    │ │ Weekly   │ │ Catch   │  │
│   │ briefing │ │ TLDR     │ │ up on…  │  │
│   └──────────┘ └──────────┘ └─────────┘  │
│                                          │
│         ┌────────────────────┐           │
│         │   Compose Brief    │ ← glass   │
│         └────────────────────┘  prominent│
└──────────────────────────────────────────┘
```

### W2 — Briefing Player (full)

```
┌──────────────────────────────────────────┐
│ ─────────────────────────────────────── │ ← editorial hairline
│                                          │
│   Tuesday Briefing                       │ ← serif 34pt, dropcap
│   8 min · drawn from 7 episodes          │ ← small caps tracked
│                                          │
│   ╭──────────────────────────────────╮   │
│   │  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │   │  ← warm-tinted glass
│   │     “…and Sundar mentioned         │   │     transcript pane
│   │      a new TPU this week.”         │   │     (live, auto-scroll)
│   │  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │   │
│   ╰──────────────────────────────────╯   │
│                                          │
│   ◀◀  ⏸  ▶▶            03:12 / 08:00     │
│                                          │
│   ↳ deeper   ⤓ skip   ½ shorter   ⤴ share │ ← per-segment actions
│                                          │
│  ┌──────────────── segment rail ───────┐ │
│  │ ●─── ○ ── ○ ── ○ ── ○               │ │ ← active pill morphs
│  │ Intro Google AI Ozempic Lex …       │ │
│  └─────────────────────────────────────┘ │
│                                          │
│   [ Ep. Hard Fork · 34:12 ⤴ ]            │ ← attribution chip
└──────────────────────────────────────────┘
```

### W3 — Segment rail expanded (chapter list)

```
┌──────────────────────────────────────────┐
│   Segments                          ▾    │
│                                          │
│   1.  Intro                       0:00   │
│   2.  Google's TPU news           0:42   │  ← tinted glass row = active
│       └ Hard Fork · The Verge cast       │
│   3.  Ozempic across your shows   2:18   │
│       └ Huberman · Peter Attia (×3)      │
│   4.  Lex backlog highlights      4:01   │
│   5.  This week's threads         6:11   │
│   6.  Outro                       7:38   │
│                                          │
│   ─── branches ───                       │
│   ↳  "headline TPU number?"       0:58   │ ← breadcrumb chip
└──────────────────────────────────────────┘
```

### W4 — Branch interaction (mid-barge-in)

```
┌──────────────────────────────────────────┐
│   Tuesday Briefing      ⟜ listening      │ ← glass deepens, glows
│                                          │
│   ╭──────────────────────────────────╮   │
│   │   "…and Sundar mentioned…"       │   │ ← italicized, frozen
│   ╰──────────────────────────────────╯   │
│                                          │
│   ╔══════════════════════════════════╗   │
│   ║  Agent                           ║   │ ← lifted glass card
│   ║  256 exaflops, double last gen.  ║   │
│   ║  Source: Hard Fork · 34:40       ║   │
│   ╚══════════════════════════════════╝   │
│                                          │
│   ┌──────────────────────────────────┐   │
│   │ ⟲  Return to briefing  ●──○      │   │ ← 4s auto-resume ring
│   └──────────────────────────────────┘   │
└──────────────────────────────────────────┘
```

### W5 — Saved briefing detail

```
┌──────────────────────────────────────────┐
│  ◀                       ⤴ Share   ⋯     │
│                                          │
│   Tuesday Briefing                       │ ← serif title
│   May 9 · 8:04 min · 7 sources           │
│                                          │
│   ╭──────────────────────────────────╮   │
│   │  generative gradient cover       │   │
│   │  (brass / amber / parchment)     │   │
│   ╰──────────────────────────────────╯   │
│                                          │
│   ▶  Play          ⤓ Export .m4a         │
│                                          │
│   Sources                                │
│   • Hard Fork — Google's New TPU         │
│   • Huberman Lab — Ozempic Deep-Dive     │
│   • Peter Attia Drive — Ep. 287, 291     │
│   • The Verge cast — Weekly              │
│   • Lex Fridman — #432, #434             │
│                                          │
│   Branches taken                         │
│   ↳ "headline TPU number?"               │
└──────────────────────────────────────────┘
```

### W6 — Generation in progress

```
┌──────────────────────────────────────────┐
│                                          │
│        Composing your briefing           │
│                                          │
│   ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    │ ← shimmering glass bar
│                                          │
│   ✓  Selected 7 episodes                 │
│   ✓  Drafted 6 segments                  │
│   ◌  Synthesizing voice…                 │ ← live status, ticks in
│   ◌  Stitching original quotes…          │
│                                          │
│         ┌────────────────────┐           │
│         │  Listen as it's    │           │ ← stream-as-generated
│         │  ready             │           │
│         └────────────────────┘           │
└──────────────────────────────────────────┘
```

### W7 — Share card

```
┌──────────────────────────────────────────┐
│   ╭──────────────────────────────────╮   │
│   │  generative cover · "Ozempic"    │   │
│   │                                  │   │
│   │   A 90-second briefing segment   │   │ ← serif quote
│   │   from Tuesday's TLDR.           │   │
│   │                                  │   │
│   │   ▶  0:00 ──────── 1:32           │   │
│   │                                  │   │
│   │   Sources:                       │   │
│   │   Huberman · Attia · Hard Fork   │   │
│   ╰──────────────────────────────────╯   │
│                                          │
│   [ iMessage ] [ Nostr DM ] [ Save ]     │
└──────────────────────────────────────────┘
```

---

## 7. Edge Cases

- **Generation in progress.** Stream-as-ready: segment 1 plays before the last synthesizes. Interrupt early — agent buffers and resumes.
- **Mid-generation cancel.** *Save partial briefing?* Partials are valid artifacts with a torn-edge cover motif.
- **Original audio fetch fails.** Substitute paraphrased TTS; mark the chip *paraphrased*. Never silently drop a citation.
- **Briefing too long.** If the request exceeds corpus, agent counter-proposes (*"12 min on this, or 25 with adjacent shows"*) rather than padding.
- **Unfeasible scope.** *"Brief me on Ozempic"* with no health subs — empty state offers a `perplexity_search` *web briefing* (cool tint, distinct texture, labeled out-of-corpus).
- **Already-heard.** Agent flags overlap with listening history; *already heard* badge on those segments. Setting: skip vs include.

---

## 8. Accessibility

**Text and audio at parity** — the live transcript is canonical (every segment, branch, citation). VoiceOver reads segment titles with chips as a single rotor element. Dynamic Type scales the serif to AX5; tint contrast tested at 4.5:1 against light and dark grounds (min tint opacity 0.18, body text ink-black at AX sizes). Cinematic stings emit VoiceOver hints, not audio-only cues. Hold-to-pause-and-ask has a button equivalent in Accessibility settings. CarPlay drops the serif for system dynamic font; shows only rail, current line, chip — no decorative motion.

---

## 9. Open Questions / Risks

- **Original-audio rights.** Fair-use duration? Publisher excerpting rights? Likely needs a *use original quotes* toggle (default on) plus a separately-negotiated allowlist.
- **Hallucination.** The surface most likely to put words in a host's mouth. Mitigation: every factual sentence carries a tap-revealable source span; unattributed sentences render in muted ink — *summary* vs. *sourced* at a glance.
- **Attribution legibility.** Field test required. Bias: over-attribute, trim later.
- **Voice identity.** Branded narrator or user's preferred TTS? Branded default, swappable.
- **Coordination with #14.** Scheduling UI here; triggering logic there. This surface emits a `BriefingSchedule`; #14 owns wake-time generation and lock-screen delivery.
- **Library noise.** Dedicated shelf with 30-day auto-archive unless saved.

---

/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-08-briefings-tldr.md
