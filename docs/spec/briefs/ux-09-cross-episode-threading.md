# UX 09 — Cross-Episode Knowledge Threading

> The connective tissue. Inline and contextual surfaces that reveal how a topic, claim, or person threads across many episodes — pulling users into cross-episode insight at the moments it matters most.

---

## 1. Vision

The user should feel like the agent is **gently revealing patterns they couldn't see alone**. Not a dashboard. Not a graph. Not a research tool. A quiet co-listener who, every so often, leans in and whispers: *"You've heard this before — three times, actually. And one of them disagreed."*

Threading is a **second voice in the margin**. It is calm by default, alive on demand. It never interrupts the listening experience; it offers itself. The user's reward for following a thread is *recognition* — the satisfying click of "oh, that's why this felt familiar."

Where the wiki destination (UX 4) is the **library**, threading is the **librarian who finds you in the stacks**.

---

## 2. Key User Moments

1. **The Familiar Phrase** — While listening, host says "Ozempic." A discreet ribbon glows once at the bottom edge: *"Heard 7 times before."* User taps; a peek slides up showing a horizontal timeline of every prior mention with a 6-second waveform preview each.

2. **The Contradiction Tap** — In the transcript reader (UX 3), a passage is underlined with a thin amber seam. Long-press: *"Andrew Huberman said the opposite on Lex Fridman, Mar 2024."* Two clips, side by side, A/B scrubbable.

3. **Evolution Over Time** — User taps a guest's name in the speaker chip (UX 13 territory but threading owns the surface). A vertical evolution column appears: *"Tim Ferriss on keto, 2019 → 2024."* Five quotes, chronological, color-shifted from cool to warm to telegraph drift.

4. **Library-Wide Topic Recall** — From search (UX 7), user opens a topic thread directly. The threading surface is the result: a scrubbable horizontal timeline of every clip across every podcast. Pinch to zoom from years to months to weeks.

5. **Briefing Anchor** — In a briefing (UX 8), agent says "this is the third time Ezra Klein has discussed AI doomerism this month." That sentence is tappable; it opens the inline thread without leaving the briefing.

---

## 3. Information Architecture

Threading is **never a tab**. A dedicated tab would betray the vision — patterns should arrive *to* the user, not be hunted. Threading lives in three layers:

- **Layer A — Now Playing Context Ribbon**: a thin, dismissible glass strip pinned to the bottom of the player above the transport controls. Appears only when the agent has detected an active topic with ≥3 prior mentions. Auto-fades after 6 seconds if ignored. A single counter glyph: *"7 ↺"*.
- **Layer B — Transcript Inline Citations** (coordinated with UX 3): topics get a thin underline in editorial serif; long-press reveals the threading peek as a bottom sheet at 40% detent.
- **Layer C — Thread Detail Sheet**: a full-screen sheet (not a destination page) reachable from any peek, ribbon, or briefing anchor. Modally presented; dismiss with swipe-down. This is *not* the wiki — it is the **timeline surface** for one specific thread.

The thread detail sheet has three tabs at its top, segmented control style: **Timeline · Contradictions · Evolution**. Default lands on whichever has the most signal.

Coordination boundaries:
- UX 3 owns the transcript text styling; we provide the underline token + long-press behavior.
- UX 4 owns the wiki "topic page"; from our detail sheet, a glass capsule footer reads *"Open the wiki entry →"* — single hand-off.
- UX 7 search results that are topic threads use our ribbon + timeline preview component.
- UX 8 briefings deep-link into our detail sheet at a specific clip.

---

## 4. Visual Treatment

**Materials.** All threading surfaces use Liquid Glass `regular` variant. The context ribbon uses `.glassEffect(.regular.tint(.adaptive).interactive(), in: .capsule)` — tint pulled from the currently playing episode's dominant artwork color, desaturated 40%. The detail sheet uses an unwrapped `GlassEffectContainer(spacing: 24)` so timeline pills morph as the user scrubs.

**Color semantics.**
- **Threading neutral**: warm parchment underline (#E6DCC8 light / #3A352B dark), 1px hairline.
- **Contradiction**: amber seam (#D9A441), 2px, animated shimmer-once on first appearance.
- **Evolution**: gradient from cool (#6B9BD1) to warm (#D88A5C) along chronological axis.
- **Confidence dim**: low-confidence threads render at 50% opacity with a dotted, not solid, underline.

**Typography.** Topic labels in editorial serif (New York or similar), 15pt. Episode meta in SF Pro Rounded 12pt, tabular numerals for dates. Clip waveforms render at 24pt height, pearl-grey strokes.

**Iconography.** A single custom glyph — a small loop with a tail — reads as "this comes back." Used on the ribbon counter and on every threading underline's terminal.

---

## 5. Microinteractions

- **Peek-pull**: ribbon tap opens a peek sheet at 40% detent. A second pull (or upward swipe on the grabber) expands to 90% detent — the full thread detail. Haptic: `.soft` on detent change.
- **Scrub-the-timeline**: in the timeline tab, a horizontal strip of clip pills. User drags a finger across the strip; a magnifier glass capsule (true Liquid Glass morph via `glassEffectID`) rides under the finger, expanding the hovered pill 1.4× and previewing 2 seconds of audio at low volume. Release to commit — opens player at that clip.
- **Swipe-between-versions-of-a-claim**: contradictions render as a stacked card pair. Horizontal swipe pages between the two stances; a translucent A/B label rides the top edge. Pulling down both at once dismisses.
- **Long-press to silence**: any thread can be muted. Long-press the ribbon → glass menu *"Stop surfacing this"*. Mutes for 30 days. Subtle, no confirmation — undoable from a settings rabbit hole.
- **Confidence pulse**: when a thread is uncertain, the loop glyph pulses very slowly (3s cycle) at 0.6 → 1.0 opacity. Confident threads are still.

---

## 6. ASCII Wireframes

### 6.1 Now Playing Context Ribbon (collapsed, above transport)

```
╭───────────────────────────────────────────────────╮
│                                                   │
│       [ Episode Artwork — large, centered ]       │
│                                                   │
│         The Tim Ferriss Show · Ep #742            │
│         "Keto, Fasting, and Mitochondria"         │
│                                                   │
│                                                   │
│   ╭─ ◌ "keto" · heard 7× before ──────────  × ─╮ │  ← glass ribbon
│   ╰───────────────────────────────────────────╯ │     (tinted)
│                                                   │
│        ◁◁    ▷ pause     ▶▶    1.0×              │
│                                                   │
╰───────────────────────────────────────────────────╯
```

### 6.2 Transcript Inline Citation — Long-press Expanded (40% detent)

```
╭───────────────────────────────────────────────────╮
│  ← Transcript       Ep #742       •••             │
│                                                   │
│  TIM:  "I've been doing strict ╔══════════╗      │
│         ̱ḵe̱ṯo̱ ̱f̱o̱ṟ ̱s̱i̱x̱ ̱w̱e̱e̱ḵs̱ now ◌  and the         │
│         mental clarity is..."                     │
│                                                   │
│  PETER: "There's a study that..."                 │
│                                                   │
│ ════════════════════════════════════════════════  │
│ ╭─ ◌ keto · across your library ─────────────╮   │  ← peek sheet
│ │                                              │   │     glass, 40%
│ │  ▂▃▅▇▅▃ ──── ▃▅▇▅▃▂ ──── ▂▃▅▇▅              │   │
│ │  Ep 712     Ep 698     Ep 644      ⋯ +4     │   │
│ │  Mar '25    Jan '25    Aug '24               │   │
│ │                                              │   │
│ │  [Timeline]  [Contradictions·1] [Evolution] │   │
│ ╰──────────────────────────────────────────────╯   │
╰───────────────────────────────────────────────────╯
```

### 6.3 Topic Timeline (full detail, 90% detent)

```
╭───────────────────────────────────────────────────╮
│  ⌃              keto                       ✕      │
│  Across 7 episodes · 4 podcasts                   │
│                                                   │
│  ─[Timeline]─ ─Contradictions·1─ ─Evolution─      │
│                                                   │
│   2024 ──────────●──●─────●────────────── 2026    │
│                  │  │     │                       │
│   ┌──────┐  ┌──────┐  ┌──────┐  ┌──────┐         │
│   │▂▃▇▅▂│  │▃▅▇▃ │  │▂▇▃▂ │  │▅▇▅▃▂│  ▸▸        │
│   │Ferris│  │Huber │  │Attia │  │Lex   │         │
│   │#742  │  │#318  │  │#198  │  │#412  │         │
│   │ 0:42 │  │12:08 │  │24:51 │  │ 8:33 │         │
│   └──────┘  └──────┘  └──────┘  └──────┘         │
│      ▲                                            │
│   ╰─◌ scrub finger; magnifier morph follows       │
│                                                   │
│   ╭────────────────────────────────────────────╮ │
│   │   Open wiki entry for "ketogenic diet" →   │ │  ← hand-off to UX 4
│   ╰────────────────────────────────────────────╯ │
╰───────────────────────────────────────────────────╯
```

### 6.4 Contradictions Detail (A/B stacked cards)

```
╭───────────────────────────────────────────────────╮
│  ⌃     contradiction · keto + cardiac risk    ✕   │
│                                                   │
│  ─Timeline─ ─[Contradictions·1]─ ─Evolution─      │
│                                                   │
│   ╭─ A ──────────────────────────────────────╮   │
│   │  ANDREW HUBERMAN  ·  Mar 14, 2024         │   │
│   │  Huberman Lab #318                        │   │
│   │                                            │   │
│   │  "The data on long-term keto and          │   │
│   │   cardiac markers is genuinely concerning" │   │
│   │                                            │   │
│   │  ▶ play 14s clip   ◌ confidence: high      │   │
│   ╰────────────────────────────────────────────╯   │
│         ↕  swipe between stances                   │
│   ╭─ B ──────────────────────────────────────╮   │
│   │  PETER ATTIA  ·  Aug 02, 2024             │   │
│   │  The Drive #198                           │   │
│   │                                            │   │
│   │  "Properly formulated keto improves       │   │
│   │   nearly every cardiac biomarker we test" │   │
│   │                                            │   │
│   │  ▶ play 18s clip   ◌ confidence: high      │   │
│   ╰────────────────────────────────────────────╯   │
│                                                   │
│   ⚠ Agent's read: these may not be a true clash — │
│     they discuss different formulations. Tap to   │
│     see full reasoning.                            │
╰───────────────────────────────────────────────────╯
```

### 6.5 Evolution of Stance

```
╭───────────────────────────────────────────────────╮
│  ⌃    Tim Ferriss on keto · over 5 years     ✕   │
│                                                   │
│  ─Timeline─ ─Contradictions─ ─[Evolution]─        │
│                                                   │
│   2019 ●  "I'm all in. Best mental clarity        │
│      │    of my life."          ▶ 12s             │
│      │                                            │
│   2021 ●  "Cycling now. Five days on,             │
│      │    weekends off."        ▶ 9s              │
│      │                                            │
│   2023 ●  "Honestly, I'm less religious           │
│      │    about it than I was."  ▶ 14s            │
│      │                                            │
│   2024 ●  "Mostly Mediterranean now.              │
│      │    Keto for cuts only."   ▶ 11s            │
│      │                                            │
│   2026 ●  "Keto, fasting, mitochondria —          │
│           full circle."           ▶ now playing   │
│                                                   │
│   gradient rail: cool ───────────────► warm       │
╰───────────────────────────────────────────────────╯
```

### 6.6 Briefing Anchor (in-context, from UX 8)

```
╭───────────────────────────────────────────────────╮
│   Daily Briefing · Tuesday                        │
│                                                   │
│   ●●●●○○○○○○  4:12 / 11:30                        │
│                                                   │
│   "...and that's the third time Ezra Klein         │
│    has discussed ̱A̱I̱ ̱ḏo̱o̱m̱e̱ṟi̱s̱m̱ ◌ this month..."   │
│                                                   │
│              ╭─────────────────╮                  │
│              │ ◌ open thread → │   ← inline pop   │
│              ╰─────────────────╯                  │
│                                                   │
│   ◁◁    ▷ pause    ▶▶                            │
╰───────────────────────────────────────────────────╯
```

---

## 7. Edge Cases

- **False-positive contradictions.** The agent's confidence in a contradiction must always be visible. Below 0.75 confidence, contradictions render with a dotted amber underline and a footer reading *"Agent's read — may not be a true clash."* Above 0.9, solid seam, no caveat. We never assert contradiction with certainty unless verbatim quotes oppose on the same noun phrase.
- **Sparse evidence (< 3 mentions).** No ribbon, no underline. The thread does not surface inline at all. It still exists in search and the wiki, but threading is reserved for *patterns*, not coincidences.
- **Single-episode topic.** Threading collapses to a "you've heard this once" empty-state inside the detail sheet only if the user navigates there explicitly from search — never auto-surfaced.
- **Recency collision.** If the same topic was just mentioned in the prior 90 seconds of the same episode, the ribbon suppresses to avoid noise. Internal repetition is not a thread.
- **Speaker self-quotation.** When a host quotes their own past episode, evolution view is preferred over timeline. Agent classifies and routes accordingly.
- **Politically/medically sensitive contradictions.** The agent never editorializes. Cards present quotes verbatim, with metadata; no "winner" is implied. The amber color is identical regardless of subject.

---

## 8. Accessibility

- **VoiceOver**: ribbon announces *"Threading hint. Topic: keto. Heard seven times. Double-tap to peek, three-finger swipe to dismiss."* Each timeline pill is its own rotor item with episode, podcast, date, and duration.
- **Dynamic Type**: timeline pills reflow vertically at AX2+; horizontal scrub falls back to a vertical list with a "play" button per row. The scrub-magnifier microinteraction has a non-scrub equivalent (tap a pill).
- **Reduce Motion**: morphing transitions between glass elements collapse to crossfade; the magnifier becomes a static highlight; gradient rail in evolution view becomes discrete chronological dots.
- **Reduce Transparency**: glass surfaces become opaque parchment with a 1px hairline border; tints are preserved.
- **Color independence**: contradictions use both amber color *and* the "≠" glyph; evolution uses both gradient *and* explicit dates. Confidence uses both opacity *and* dotted-vs-solid stroke.
- **One-handed driving / screen-off**: the ribbon's only audio cue is a single soft chime when first surfaced (off by default). All threading is silent unless the user opts in. Voice mode (UX 6) can read a thread aloud on request: *"Hey, what's the contradiction on keto?"*
- **Touch targets**: every interactive pill is ≥44×44pt. Ribbon dismiss button is 44pt despite visual size of 24pt (extended hit region).

---

## 9. Open Questions / Risks

1. **Communicating LLM uncertainty without breaking trust.** Our chosen vocabulary — "Agent's read", confidence dim, dotted strokes — is research-debt. We need user testing on whether listeners distinguish "high confidence threading" from "low confidence threading" in the wild, or whether all threading reads as gospel. Risk: a wrong contradiction destroys credibility.
2. **Notification fatigue from the ribbon.** Even at 6-second auto-dismiss, an interruption every few minutes would be ruinous. Proposal: cap at 1 ribbon per 10 minutes of listening, and never within 30 seconds of a chapter boundary or a user action. Needs telemetry to tune.
3. **Cross-podcast attribution and rights.** Showing a 14-second clip from another publisher's podcast inline raises a legal posture question. Likely needs a short-clip fair-use ceiling (≤20s) and clear publisher attribution on every pill. Confirm with legal.
4. **Embedding drift across podcast vocabularies.** "Keto" in a medical podcast vs. a fitness podcast may not cluster cleanly. Threading quality is a function of the embedding model + clustering; agent should expose a *"Why was this threaded?"* affordance from any pill.
5. **Where does "Stop surfacing this" live globally?** A muted-threads list belongs in settings, but threading has no settings page of its own. Recommend a small section under Agent settings (UX 14 territory).
6. **Evolution view for guests, not just hosts.** Guest appearances are sparser; an evolution view with 2 data points feels thin. Proposal: minimum 3 chronological mentions before evolution mode unlocks; below that, contradictions or timeline only.
7. **Conflict with UX 4 wiki destination.** If the user can scrub a timeline of every keto mention here, why ever go to the wiki page? Boundary must hold: threading is *episodic recall*; wiki is *synthesized knowledge*. Hand-off button at the bottom of every detail sheet is the primary reinforcement.

---

**File**: `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-09-cross-episode-threading.md`
