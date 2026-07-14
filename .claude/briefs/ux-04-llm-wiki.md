# UX-04 — LLM Wiki Browser

> Owner: Designer agent · Coordinates with #9 (Threading), #13 (Speaker Profiles), #15 (Glass System)
> Source ethos: [nvk/llm-wiki](https://github.com/nvk/llm-wiki) — LLM-compiled, dual-linked, confidence-scored, immutable-source-paired knowledge bases. We translate that philosophy into a *visual* artifact.

---

## 1. Vision

The Wiki is the **second brain of the listener's library**. Where the player is hot — playing, talking, listening — the Wiki is cool: a quiet, beautifully set reference work the user retreats into to *understand* what they have heard. Britannica energy. *The Whole Earth Catalog* density. *Edward Tufte* respect for evidence. *Stripe Press* typography. But native to iOS 26, alive with Liquid Glass, and rebuilt every time a new episode lands.

The llm-wiki ethos we inherit:

- **Immutable raw + synthesized layer** → in our app, the audio + transcript is raw and untouchable; the wiki article is the synthesis. The wiki never paraphrases without provenance.
- **Dual-linking** → every entity is a link, but typed (Topic / Person / Episode / Claim). Hover/long-press reveals the type.
- **Confidence scoring** → every claim renders its evidence weight (high / medium / low) in the margin, not buried in a footer.
- **Thesis-driven evidence lanes** → contradictions get equal visual weight to consensus.

The user feels: *"This is my library, written into a book about itself."*

---

## 2. Key User Moments

1. **The discovery** — finishes an episode, taps "Wiki this" in Now Playing, lands on a fresh topic page distilled from what they just heard, surrounded by every other time the topic surfaced in their library.
2. **The cross-reference** — reading a topic page, a citation says "Huberman, ep 218, 47:12 — *contradicts*". Tap. The audio peeks open in a glass sheet, plays the 30-second clip, dismisses.
3. **The conjure** — types "mitochondrial uncoupling" into the wiki bar; nothing exists yet. Pulls a "Generate Page" handle down. Watches the page write itself, paragraph by paragraph, citations resolving as they land.
4. **The argument** — opens *Ozempic*, swipes to **Time view**, sees a horizontal stratum of takes from 2023 → 2025, color-shifting from skeptical-amber to mainstream-blue.
5. **The correction** — a claim feels wrong; long-press, "This is wrong", record a 5-second voice memo. The wiki marks the claim *contested-by-user*, sends to the agent, regenerates with the user's note as additional evidence.
6. **The map** — pinches the Wiki home; library zooms out to a constellation of topics, sized by listening minutes, edges weighted by co-occurrence. Useful once a quarter, not a daily driver.

---

## 3. Information Architecture

```
Wiki Root
├── Library Wiki (cross-podcast, default home)
│   ├── Topics index (A–Z, by recency, by listening-time)
│   ├── People index (mentioned + speakers — links to #13 profiles)
│   ├── Debates (auto-clustered contradictions across feeds)
│   └── Recent edits (what the agent rewrote in the last 7 days)
├── Per-Podcast Wiki (scoped to one feed)
│   └── same structure, scoped
├── Topic Page  ←  the heart of the surface
│   ├── Definition (1 paragraph, italic, evidence-graded)
│   ├── Who's discussed it (avatars + episode count)
│   ├── Evolution timeline (compact horizontal strip)
│   ├── Consensus vs Contradictions (split column)
│   ├── Related topics (typed chips)
│   └── Citations (every episode + timestamp, sortable)
├── Person Page (thin variant of #13 — bio stub + their claims, links out)
├── Graph View (constellation, optional, opt-in zoom)
└── Generate Page (text field + agent compile-stream)
```

Search-within-wiki is a separate input from #7's semantic episode search: the wiki bar searches **page titles + claim text + citation context**, returns *pages* (not transcripts). The empty-state of the wiki bar surfaces the **Generate Page** affordance — typing a query that finds nothing reveals "Compile this →".

---

## 4. Visual Treatment

**Type stack.**
- Display & H1: *New York Large* (or licensed equivalent: *GT Sectra*) — editorial serif at 34/40, tight tracking.
- Body: *New York* serif, 17/26, optical sizing on. Generous measure (~62ch).
- Captions / citations: *SF Mono* 12, tabular figures, for timestamps and confidence scores.
- UI chrome: *SF Pro Text* — the only sans, used only for navigation.

**Color.**
- Paper: `#F6F2E9` light / `#0E0F12` dark — warm, paperlike, never pure white.
- Ink: `#161618` / `#EEEAE0`.
- Citation amber: `#B8741A` (tap target for timestamp links).
- Confidence: green-650 / amber-600 / rose-600 — used as 2px left margin rules on claim blocks, never as fill.
- Accent: episode-art-derived, sampled and softened to 18% chroma — the page subtly tints toward the dominant podcast.

**Glass treatment.**
- Page itself is **paper, not glass** — the wiki must read as printed matter.
- Glass is reserved for *floating* elements: the citation peek sheet, the wiki bar, the time-slider, the Generate handle, the contradiction popover. `GlassEffectContainer(spacing: 24)` groups them so they morph as the user scrubs through citations.
- All glass is `.regular.interactive()`, in `.rect(cornerRadius: 22)` — never capsule on the wiki side; capsules belong to the player.
- Scroll edge: `topEdgeEffect = .automatic`, `bottomEdgeEffect = .hard` so the wiki bar always reads against the page.

**Layout.**
- Single column at 62ch on phone, two-column at 78ch + 22ch margin on iPad — the margin holds confidence rules, citation pips, and timeline marks (Tufte-style).
- Section dividers are 1px hairlines in ink-30, never glyphs.
- A subtle 8pt baseline grid; everything aligns to it.

---

## 5. Microinteractions

- **Citation peek.** Hover/long-press a citation chip → glass sheet rises 1/3 from bottom, autoplays the cited 12 seconds, waveform animates, releasing dismisses. Pulling up expands to full transcript at that timestamp (hands off to #3).
- **Drag-to-compare.** Drag one topic chip onto another → page splits vertically, shared citations highlighted in amber, divergent claims rendered side-by-side. Feels like laying two transparencies on a light table.
- **Time scrub.** Topic pages have a tiny horizontal "evolution" strip at the top; dragging a thumb across it re-renders the page *as it would have read on that date* (claims are versioned by their source episode's publish date).
- **Confidence reveal.** Tapping the left-margin rule beside any claim expands an inline drawer with the actual evidence sentences pulled from transcripts.
- **Generate handle.** A small glass tab labeled "Compile" lives at the bottom of any empty search; pulling it down progressively unlocks the page — paragraph blocks slide up one at a time as the agent finishes each, citations resolving from `[…]` placeholders to amber pips.
- **Wrong button.** Long-press any sentence → contextual menu *(Cite · Compare · This is wrong · Quote)*. "This is wrong" opens a 5-sec voice memo glass card; submission marks the claim as `contested` until next regeneration.
- **Reading position halo.** Scrolling past a citation whose audio you've actually listened to puts a soft amber underline on the timestamp — *you have heard this*.

---

## 6. ASCII Wireframes

### 6a. Wiki Home (Library scope)

```
┌─────────────────────────────────────────────────────┐
│  ◐ Library Wiki              [⌕ search the wiki…]  │ ← glass bar
├─────────────────────────────────────────────────────┤
│                                                     │
│   The Library, Indexed.                             │ ← editorial H1
│   1,284 episodes · 312 topics · 187 people          │   serif, hairline
│                                                     │
│  ─────────────────────────────────────────────────  │
│                                                     │
│   THIS WEEK                                         │
│   • Ozempic           ▮▮▮▮▮▮▮▮ · 14 eps · ↑         │
│   • Stablecoins       ▮▮▮▮▯▯▯▯ ·  9 eps · →         │
│   • Mitochondria      ▮▮▮▯▯▯▯▯ ·  6 eps · ↑↑        │
│                                                     │
│   DEBATES                                  [view]   │
│   ⊕ keto vs zone-2  • 4 podcasts disagree           │
│   ⊕ AI doom timing  • 7 podcasts disagree           │
│                                                     │
│   PEOPLE                                   [view]   │
│   ◯ ◯ ◯ ◯ ◯ ◯ ◯ ◯  (avatars, by mention freq)      │
│                                                     │
│  ─────────────────────────────────────────────────  │
│   Recent edits  ·  Graph view  ·  Generate page →   │
└─────────────────────────────────────────────────────┘
```

### 6b. Topic Page — *Ozempic*

```
┌─────────────────────────────────────────────────────┐
│  ‹ Library                                  [⌕] [⋯] │
├─────────────────────────────────────────────────────┤
│                                                     │
│              Ozempic                                │ ← 34pt NY serif
│              Topic · 47 episodes · 9 podcasts       │
│                                                     │
│   ╭─ 2023 ─────────●●●───── 2024 ●●●●● ─ 2025 ●● ─╮ │ ← time strip
│                                                     │
│  │ Semaglutide-based GLP-1 agonist originally       │
│  │ approved for type-2 diabetes; off-label          │
│  │ weight-loss use exploded in 2023…   [hi-conf]    │ ← left rule
│                                                     │
│   WHO'S DISCUSSED IT                                │
│   ◯Huberman  ◯Attia  ◯Rogan  ◯Ezra Klein  +5       │
│                                                     │
│   CONSENSUS              │   CONTRADICTIONS         │
│   • Effective for         │   • Long-term safety    │
│     short-term loss       │     ─ Attia: cautious   │
│   • Reduces visceral fat  │     ─ Rogan: alarmed    │
│                          │     ─ NYT pod: optimistic│
│                                                     │
│   RELATED   [Metformin] [GLP-1] [Lean mass loss]    │
│                                                     │
│   CITATIONS                              [▾ sort]  │
│   • Huberman #218 · 47:12 · "…uncoupling…"      ▶   │
│   • Attia   #284 · 12:04 · "I'd want to see…"   ▶   │
│   • Rogan  #2103 · 1:22:08 · "This stuff is…"   ▶   │
│                                                     │
└─────────────────────────────────────────────────────┘
```

### 6c. Citation Peek

```
┌─────────────────────────────────────────────────────┐
│   …off-label weight-loss use exploded in 2023,      │
│   driven by social-media virality [Huberman #218 ▶].│ ← amber chip
│                                                     │
│   ╭───────────────────────────────────────────────╮ │
│   │  ░░░ glass sheet rising ░░░                  │ │
│   │  Huberman Lab · #218 · 47:12 → 47:24         │ │
│   │  ▮▮▮▯▮▮▯▯▮▮▮▯▮▮▯ ▶ playing                  │ │ ← waveform
│   │  "…the uncoupling effect on mitochondria…"   │ │
│   │  [Open episode]   [Quote]   [Add to brief]    │ │
│   ╰───────────────────────────────────────────────╯ │
└─────────────────────────────────────────────────────┘
```

### 6d. Graph View (opt-in)

```
┌─────────────────────────────────────────────────────┐
│  ‹ Wiki                                Graph · ⓘ    │
├─────────────────────────────────────────────────────┤
│                                                     │
│           ◌                ◯ Ozempic                │
│       ◌       ◯─────────────●                       │
│                ╲           ╱╲                       │
│              ◌  ╲ Metformin ╲                       │
│                  ●           ◯ GLP-1                │
│        ◯ Keto ───●─── Zone-2 ●                      │
│             ╲   ╱                                   │
│              ◌                ◯ Mitochondria        │
│                                                     │
│   [● = high listen-time   ◌ = mentioned only]      │
│   [edge weight = co-occurrence in same episode]     │
│                                                     │
│   Filter:  [topics] [people] [debates] [time ◐]     │
└─────────────────────────────────────────────────────┘
```

### 6e. Generate Page Flow

```
┌─────────────────────────────────────────────────────┐
│  ⌕  mitochondrial uncoupling                        │
│  ─────────────────────────────────────────────────  │
│  No page exists yet.                                │
│                                                     │
│            ╭──────────────────────────╮             │
│            │   ↓  Compile this page    │  ← glass handle
│            ╰──────────────────────────╯             │
│                                                     │
│   ── pulled down ──                                 │
│                                                     │
│   Mitochondrial uncoupling                          │ ← types in
│   ● searching transcripts   (3,142 chunks)          │
│   ● drafting definition…                            │
│   ● resolving citations  [▮▮▮▮▮▮▯▯]                 │
│   ✓ Section: Definition                             │
│   ✓ Section: Who's discussed it                     │
│   ◌ Section: Contradictions  (compiling…)           │
│                                                     │
│   [Cancel]                          [Read in full]  │
└─────────────────────────────────────────────────────┘
```

---

## 7. Edge Cases

- **Page being regenerated.** A 1px shimmer travels down the left margin while regeneration is live; the *prior* version remains fully readable until the new one finishes (atomic swap, not progressive overwrite). User can tap "Show diff" to see what changed.
- **Contradictions detected.** When the agent finds a new contradiction since last visit, the topic chip on the home gets a small amber dot; opening the page anchors to the contradiction column with a soft pulse.
- **Low-evidence claims.** Single-source claims render with an amber dotted left rule and the inline tag *(1 source)*. Claims with zero corroboration outside the user's library get *(uncorroborated)*.
- **Sources retracted.** If a podcast episode is removed from a feed, every citation pointing to it gets struck-through and replaced with a footnote: *"Source no longer available — claim retained for context, evidence weight reduced."*
- **Hallucination caught.** "This is wrong" submissions are queued; the *next* regeneration must either remove the claim, reduce its confidence, or surface a counter-citation. The user sees a small "you flagged this" badge until resolution.
- **Empty wiki.** New users see a "Your wiki will appear as you listen" state, with three example pages from a featured podcast as a teaser.
- **Offline.** Pages are cached as static markdown + waveform sprites; citation peek falls back to transcript-only with a "audio offline" note.

---

## 8. Accessibility

- All citation chips have `accessibilityLabel` like *"Huberman episode 218, 47 minutes 12 seconds, plays clip"*.
- Confidence margin rules are paired with `accessibilityValue` *(high / medium / low evidence)* — color is never the only carrier.
- Dynamic Type up to AX5 — body serif uses optical sizing, no text clipping at large sizes; the two-column layout collapses to single column at AX2+.
- VoiceOver rotor: *Citations*, *Claims*, *Sections*, *People* — first-class navigation modes.
- Reduce Motion: time-strip scrubbing snaps; generate-page paragraphs fade rather than slide; graph view becomes a sortable list.
- Contrast: paper/ink at 11.4:1; amber citation at 5.2:1 on paper. Tested both modes.
- Audio peek respects Mute Switch — visual waveform with captions plays muted.
- One-handed: wiki bar is bottom-anchored in glass; all primary actions reachable in the lower 1/3 of phone screens.

---

## 9. Open Questions / Risks

1. **Hallucination liability.** The wiki *will* sometimes invent. Our defense is provenance-or-it-doesn't-render: every sentence must trace to a citation or be marked `[unsourced]` and visually demoted. Open: do we ever allow unsourced sentences? Recommendation — only in *Definition* paragraphs, and only with `[general knowledge]` tag.
2. **Should the graph view exist?** **Yes — but as a *destination*, not a default.** The graph is dazzling in demos and shallow in daily use. Burying it one level deep (under "Wiki home → Graph") protects the editorial calm of the topic page while keeping the surface for power users and screenshots. Reject the temptation to make it the home.
3. **Per-podcast vs library scope.** Risk of overwhelming users. Recommendation: default to library scope; per-podcast scope is a chip filter, not a separate destination.
4. **Regeneration cadence.** Every new episode triggers a partial regen; full regens nightly. Open: do we show the user "this page is 3 hours stale"? Probably no — undermines trust without adding signal.
5. **Editing rights.** The user can flag, not edit directly. Direct edits would make the wiki diverge from its evidentiary source. Treat user notes as *additional evidence*, not overrides.
6. **Coordination with #9 Threading.** Their timeline-of-contradictions inside the player must deep-link into the wiki's contradiction column, not duplicate it. Joint spec needed.
7. **Coordination with #13 Speaker Profiles.** Person pages here show only: avatar, role, top-3 claims, "Open full profile →". No bio prose. Their surface owns the depth.
8. **LLM cost per regen.** Aggressive caching at the claim level — only re-synthesize claims whose underlying transcripts changed. Engineer agent to design.

---

**File:** `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-04-llm-wiki.md`
