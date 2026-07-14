# UX-03 — Episode Detail & Transcript Reader

## 1. Vision

This surface is where a podcast becomes a **document**. Most apps bury the transcript behind a debug tab; we treat it as the primary readable object — typeset like *The New Yorker*, navigable like a book, addressable like a webpage. Detail is the magazine cover; reader is the essay; follow-along is karaoke for ideas. Every other app capability — agent recall, clipping, wiki cross-linking, cross-episode threading — terminates at a sentence on this page.

## 2. Key User Moments

1. **Read instead of listen.** Commute is noisy, meeting in 20 minutes — user reads the transcript like an article. Player collapses. Pure prose.
2. **Follow along while listening.** Driving glance, dishes, walking. Current sentence highlighted, page auto-scrolls, tap any line to jump audio there.
3. **Search within a 3h interview.** "Where did Huberman talk about creatine?" In-transcript find with chapter-aware results, speaker filter, one-tap audio jump.
4. **Capture a quote.** Long-press → expand to paragraph → "Share as image" with artwork, speaker, timestamp, deep-link back. Or "Make a clip" — drag handles, share as audio + subtitled video.
5. **Ask the agent about this passage.** Select text → "Ask about this" pipes selection + episode + timestamp into agent chat (hand-off to **UX-05**) so the agent answers grounded in this context.

## 3. Information Architecture — Three Modes

**(A) Episode Detail.** Hero (artwork, show, guest, date, duration), show-notes HTML, chapters list, agent-generated summary, "Open transcript" CTA, related-episodes rail. Floating glass player (see **UX-01**).

**(B) Reading Mode.** Player vanishes. Single column, editorial type, no chapter rail. Subtle progress gutter on the leading edge. The ad-free, listening-free reading room.

**(C) Follow-Along.** Audio playing *and* transcript visible. Current sentence tinted; page auto-scrolls to keep it in the upper third (Kindle-style). Vertical **chapter rail** on the trailing edge as a Liquid Glass strip (§4). Tapping any sentence scrubs audio. Player docks as a bottom glass pill.

Transitions are gestural: pull-up enters reading; tap-to-play enters follow-along; scrub the rail to return to detail.

## 4. Visual Treatment — Typography Is Paramount

**Type family.** Body = **New York** (Apple's editorial serif, optical sizes, Dynamic Type-aware). UI chrome and timestamps = **SF Pro Text**. Speakers = **SF Rounded Semibold**. The serif is the load-bearing decision — it signals "document, not interface."

**Type ramp (default Dynamic Type size, Large).**
| Token | Family | Size / Leading | Use |
|---|---|---|---|
| Display | NY Large Bold | 34 / 40 | Episode title (hero) |
| Lede | NY Medium Italic | 21 / 30 | Agent-generated summary |
| Body | NY Regular | 19 / 30 (1.58) | Transcript paragraphs |
| Body-tight | NY Regular | 17 / 26 | Show notes HTML |
| Speaker | SF Rounded Semibold | 13 / 18 | Speaker name above paragraph |
| Timestamp | SF Pro Mono | 11 / 16 | Inline timestamps, tabular numerals |
| Caption | SF Pro Text | 13 / 18 | Chapter labels, citations |

**Line length.** On iPhone, body fills the column with **20pt outer margins** and reflows naturally (~38–42ch at body 19pt). The **64-character optimum clamp** (58–72 range) engages from iPad width upward, pinning the column at 600pt and centering it. Never edge-to-edge.

**Vertical rhythm.** Paragraphs separated by 0.75× line-height. Speaker hangs in the left margin on iPad, inline on iPhone. Chapter breaks: 64pt air, hairline rule, chapter name in small caps.

**Liquid Glass.**
- **Chapter rail.** Vertical `GlassEffectContainer` (spacing 12), trailing edge. Each chapter is `.glassEffect(.regular.interactive(), in: .capsule)`; active chapter morphs via `glassEffectID` into a labeled pill — reads as a single liquid bead tracking scroll position.
- **Floating player.** Bottom-centered glass capsule, `.glassProminent` play/pause, inline scrubber. Slides off-screen in reading; pull-down restores. We own dock geometry; **UX-01** owns internals.
- **RAG citations.** Not chips (too noisy). 2pt dotted underline + superscript glass dot. Tap → glass popover ("Contradicted in *Episode 142*"). Hand-off to **UX-09**.
- **Wiki links.** Solid 1pt underline in `tintColor.secondary`. Tap pushes wiki (**UX-04**); long-press peeks a glass card.

## 5. Microinteractions

- **Long-press sentence** (300ms): lifts with glass scale 1.02, soft haptic. Selection handles snap to sentence; drag to expand to paragraph or contract to phrase. Action bar: Copy · Share Image · Clip · Ask Agent · Bookmark.
- **Drag-to-clip.** Two-finger vertical drag = span selection with waveform preview; release opens clip composer, sentence-snapped (word-snap via second long-press).
- **Sentence scrub.** Dragging the leading gutter snaps audio to sentence starts with a tick haptic per snap — scrubbing in *meaning units*.
- **Speaker filter.** Tap a speaker label → others dim to 35%.
- **Pull-to-find.** Pull past the hero reveals an inline find field; results highlight in-place with chapter context.
- **Annotations.** *Highlights* = soft yellow tint on the sentence's text background (never the glass chrome). *Notes* = sentence-attached, surface as a tiny glass asterisk in the leading margin; tap opens a bottom sheet. *Bookmarks* = solid dots on the chapter rail at their exact position. All three roll up into a per-episode "Marks" tab and a global view (owned by **UX-02**), reachable via a VoiceOver rotor.

## 6. ASCII Wireframes

### 6.1 Episode Detail (hero + chapters)
```
┌─────────────────────────────────┐
│ ‹  Show Name              • • • │
│                                 │
│   ┌─────────┐                   │
│   │         │  HOW TO THINK     │
│   │ artwork │  ABOUT KETO       │
│   │         │  Tim Ferriss · #732│
│   └─────────┘  May 4 · 2h 14m   │
│                                 │
│   ▶ Play   ⤓ Download   ⊕ Save  │
│                                 │
│   ── Summary ──────────────     │
│   "Ferriss and Attia trace the  │
│    arc of metabolic research…"  │
│                          (lede) │
│                                 │
│   ── Chapters ─────────────     │
│   00:00  Cold open              │
│   04:12  Why ketones matter     │
│   28:40  The Inuit objection    │
│   ...                           │
│                                 │
│   ── Show notes ───────────     │
│   Lorem ipsum… (HTML)           │
│                                 │
│  ╭───── Read transcript ─────╮  │
│  ╰───────────────────────────╯  │
│                                 │
│  ◐  ▶ 0:14:22 ─────●─── 2:14   │  ← floating glass player
└─────────────────────────────────┘
```

### 6.2 Follow-Along Mode
```
┌─────────────────────────────────┐
│ ‹  Ch. 2 · Why ketones matter ⌕ │
│ ┃                               │
│ ┃  TIM FERRISS · 14:08          │
│ ┃  So when you talk about       │
│ ┃  metabolic flexibility, what ┃│ ◀ chapter rail (glass)
│ ┃  do you actually mean? Like  ┃│   active chip morphs as
│ ┃ ▓▓▓ in a clinical sense? ▓▓▓ ┃│   user scrolls
│ ┃                               │
│ ┃  PETER ATTIA · 14:31          │
│ ┃  Right, so the term gets      │
│ ┃  thrown around, but really    │
│ ┃  what we're measuring is…     │
│ ┃                               │
│              ╭───────────────╮  │
│              │ ⏸  14:22 / 2:14│  │ ← docked glass pill
│              ╰───────────────╯  │
└─────────────────────────────────┘
  (▓ = current sentence highlight; ┃ = leading progress gutter)
```

### 6.3 Reading Mode
```
┌─────────────────────────────────┐
│              · · ·              │   no chrome, no player
│                                 │
│   HOW TO THINK ABOUT KETO       │
│   Tim Ferriss · #732 · 2h 14m   │
│                                 │
│   ─────────────────────         │
│                                 │
│   TIM FERRISS                   │
│   So when you talk about        │
│   metabolic flexibility, what   │
│   do you actually mean? Like    │
│   in a clinical sense?          │
│                                 │
│   PETER ATTIA                   │
│   Right, so the term gets       │
│   thrown around — and there's   │
│   a Inuit study¹ that pushed    │
│   ─── back on the orthodoxy.    │
│                                 │
│   (1) dotted = RAG citation     │
│                                 │
│  swipe up ↑ for player          │
└─────────────────────────────────┘
```

### 6.4 Transcribing-in-Progress
```
┌─────────────────────────────────┐
│ ‹  Transcribing…    ◐ 38%       │
│                                 │
│   We're transcribing this       │
│   episode now. You can read     │
│   along as it streams in.       │
│                                 │
│   ─────────────────────         │
│   TIM FERRISS · 00:00           │
│   Welcome back to the show.     │
│   Today I'm joined by…  ▌       │ ← live cursor
│                                 │
│   ░░░░░░░░░░░░░░░░░░░░░░        │
│   ░░░░░░░░░░░░░  (skeleton)     │
│   ░░░░░░░░░░░░░░░░░░            │
│                                 │
│   [ Notify me when ready ]      │
│   ETA 4 min · ElevenLabs Scribe │
└─────────────────────────────────┘
```
Partial transcript streams in real-time; user can read what exists, the rest is animated skeleton paragraphs.

### 6.5 Quote Share
```
┌─────────────────────────────────┐
│   ╭─────────────────────────╮   │
│   │  ┌───┐                  │   │
│   │  │art│  Tim Ferriss Show│   │
│   │  └───┘  #732 · May 4    │   │
│   │                         │   │
│   │  "Metabolic flexibility │   │
│   │   isn't a diet — it's a │   │
│   │   property of the       │   │
│   │   mitochondria."        │   │
│   │                         │   │
│   │   — Peter Attia, 14:31  │   │
│   │                         │   │
│   │   podcast.app/e/732?t=871│  │
│   ╰─────────────────────────╯   │
│                                 │
│   [ Image ] [ Audio+Sub ] [ Link]│
└─────────────────────────────────┘
```

### 6.6 Clip Creation
```
┌─────────────────────────────────┐
│ ‹  New Clip                Done │
│                                 │
│   ▁▂▅█▇▆▃▂▁▂▄▆█▇▅▃▂  waveform   │
│   ╞══════════╡  in 14:22 → 14:48│
│                                 │
│   "So when you talk about       │
│    metabolic flexibility…"      │
│                                 │
│   ┌─ Subtitle style ──────┐     │
│   │ ● Editorial  ○ Bold   │     │
│   │ ● Speaker labels  ☑   │     │
│   └───────────────────────┘     │
│                                 │
│  [ Preview ]   [ Share clip ]   │
└─────────────────────────────────┘
```

## 7. Edge Cases

- **No transcript, no budget.** Show notes become the readable object; one-tap "Transcribe this episode (1 credit)."
- **Low-confidence regions.** Words below 0.6 confidence get a 1pt dotted underline in `tintColor.tertiary`. Long-press shows top-3 alternates + "report correction."
- **5h Lex Fridman.** Virtualize the transcript (±2 chapters around scroll laid out). Long-press the chapter rail expands it into a full-height speaker timeline minimap.
- **Non-English / RTL.** Swap NY for Noto Serif fallback; mirror rail and gutter.
- **Live episode.** Hide "Read transcript" until first chapter finalized; show "Transcribing live" badge.

## 8. Accessibility

- **Dynamic Type** end-to-end; reflows at AX5. At AX3+ the 64-char clamp drops.
- **VoiceOver.** Each paragraph is one accessible element with a custom rotor: *Paragraph · Speaker · Chapter · Citation*. Swipe = paragraph; double-tap plays from there. Announces `"Peter Attia, 14:31, paragraph."`
- **Reading speed.** Auto-scroll cadence decoupled from playback — user sets "scroll lead" (−2 to +2 sentences).
- **Reduce Motion.** Auto-scroll → step-scroll; rail morph → cross-fade.
- **Reduce Transparency.** Glass falls back to `Material.thick` + hairline.
- **Contrast.** Body text never glass-on-glass — reading column on solid `systemBackground`; glass is chrome only.
- **One-handed.** All quote/clip/agent actions reachable from the long-press bar at thumb height.

## 9. Open Questions / Risks

1. **Three modes vs two.** Worth testing whether reading and follow-along collapse into one mode differing only by player presence.
2. **Citation density.** Cap RAG citations at 2 per paragraph; surface the rest in a margin marker.
3. **Live transcript freshness.** 30s ElevenLabs blocks make the cursor sticky — needs a smoothing buffer.
4. **Hand-off with UX-01.** Assumes the player exposes `currentTime`, `seek(to:)`, `presentationMode` (.docked / .hidden / .floating).
5. **Clip composer ownership.** May deserve its own surface; flag for synthesis.
6. **Wiki disambiguation.** "Cold" the noun vs "Cold open" the chapter — hand-off to **UX-04** for linking policy.

---

**File path:** `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-03-episode-detail.md`
