# UX-07 — Semantic Search & Discovery

> The "find that thing" surface. This is where the user's vague memory becomes a tap-to-play moment.

---

## 1. Vision

The search bar should feel like **a librarian who has read everything you've ever subscribed to** — including the episodes you haven't gotten around to. Not a database lookup. Not a keyword grep. A patient, well-read intermediate who hears *"that one about stamps or something"* and silently walks to the right shelf.

Search is the lowest-effort surface in the app. It is **not** the agent (the agent argues, summarizes, generates briefings). Search **shows you the evidence** — episodes, clips, transcript moments, wiki entries — and gets out of the way. If the agent is the conversation, search is the index of the library. The user often does not want a synthesis; they want the receipt.

The search bar lives at the top of the **Ask** tab, always-on, never collapsed, never hidden behind a magnifying glass icon. It is a destination, not a utility.

## 2. Key User Moments

1. **Vague recall** — *"That podcast last week about stamps?"* User taps Ask, types `stamps`, sees a clip card from a Tim Ferriss episode where the guest mentioned stamp collecting for 90 seconds. Taps the card. Player opens at 47:12.
2. **Scoped search** — User is on a show's detail page, taps search, scope chip "In this show" is pre-active. They search `keto` and see only this show's results, ranked by relevance.
3. **Voice-driven search** — User holds the mic on the search bar, says *"the part about Ozempic from any health podcast last month."* Voice overlay shows the transcribed query as a typography-first card; results stream in beneath while still listening.
4. **Explore-by-topic** — User taps a topic chip *"Longevity"* under the search bar (semantic suggestion based on listening history). They land in a topic results page — wiki entry at the top, clips below, related shows on the side.
5. **No-results-but-Perplexity-knows** — User searches `Tegus AI memo July 2026`, no corpus match. Empty state offers a single, beautifully framed card: *"Not in your library. Search the open web?"* Tap → agent runs `perplexity_search`, returns inline.

## 3. Information Architecture

**The Ask tab** has three vertical zones:

- **Input zone** (always pinned at top, glass material, follows keyboard): search field, mic button, scope chip rail.
- **Suggestions zone** (visible while typing, before first result): typeahead semantic suggestions, recents, saved searches, topic chips.
- **Results zone** (replaces suggestions on commit): unified, multi-type result feed grouped by relevance, not by type.

**Result types — one feed, six card shapes:**

| Type | What it represents | Card affordance |
|---|---|---|
| **Clip** | a 15s–3min span inside an episode | waveform sliver, in-card play button, timestamp |
| **Episode** | whole episode | cover art, show name, duration, "Why it matched" snippet |
| **Show** | podcast subscription | logo, episode count, follow state |
| **Topic** | wiki page | typographic card, subject + 2-line synopsis |
| **Person** | speaker / guest profile | portrait or initial monogram, episode count |
| **Transcript moment** | a quoted line with surrounding context | pull-quote treatment, speaker attribution |

**Scope chips** (horizontal, scrollable, multi-select with one exclusive group):
`This show` · `This week` · `This month` · `Unlistened` · `By [Person]` · `On [Topic]` · `In transcripts` · `In wikis`

**Result detail** — tapping a card never leaves search. Clip cards play inline. Episode cards expand to a full editorial cover-and-summary preview with a Play button. Topic cards push to the wiki destination (UX-04).

## 4. Visual Treatment

- **Typography-first.** Results are not rows — they are editorial cards. Headlines in a high-contrast serif (New York or SF Serif), body in SF Pro Text. Generous leading. The query terms inside a result get a subtle highlight (a 4px-radius tinted background, never bold yellow).
- **Glass cards, varied materials.** Each result type gets a slightly different glass treatment so the eye learns to scan:
  - Clip → `regular` glass, capsule corners, faint waveform watermark
  - Episode → `regular` glass, 16pt corners, cover art bleed at the leading edge
  - Show → `regular` glass with a soft tinted base derived from the show's dominant cover color (8% opacity)
  - Topic / Wiki → uncolored, paper-feel — barely-glass with a hairline border, like a bookplate
  - Person → circular portrait punching out of a capsule glass card
  - Transcript moment → `regular` glass, oversized open-quote glyph, italicized body
- **One container, morphing.** Wrap all six in a `GlassEffectContainer(spacing: 12)` so cards near each other blend on scroll bounce — the "library" feels physically continuous.
- **Color discipline.** No category colors. Differentiation is shape, type, and material. The only color in the feed is the show's cover color, used sparingly.
- **Whitespace.** 24pt between cards. The feed should feel *under-populated*, not stuffed.

## 5. Microinteractions

- **Typeahead with semantic suggestions** — as the user types `keto`, the suggestion list shows two sections: *Literal matches* (3 episodes whose titles contain "keto") and *Semantic neighbors* (clips about ketosis, fasting, low-carb). The semantic section has a small `~` glyph to signal "approximately."
- **Scope chips morph in.** When the user types a name we recognize ("Tim Ferriss"), a `By Tim Ferriss` chip morphs into the chip rail using `glassEffectID` — it slides in from the right, the rail compresses left.
- **Swipe-to-play on clip cards.** Swipe right on a clip card → it expands into an inline mini-player (waveform fills the card width, transcript scrolls under) without leaving the results.
- **Hold-to-peek transcript context.** Long-press any clip or transcript card → a glass sheet rises 2/3 height showing 60 seconds of transcript surrounding the match, with the match line glowing. Release to dismiss; drag up to commit to the full transcript view.
- **Mic press feedback.** Holding the mic dims the rest of the screen to 30%, the search field becomes a glass capsule that pulses with the audio waveform. Releasing without speaking dismisses; speech commits the query.
- **Result streaming.** Results don't pop in all at once — they morph in one card at a time over ~250ms each, glass shapes merging into the container. Feels like the librarian is laying cards on a table.

## 6. ASCII Wireframes

### A. Empty Ask tab (just opened)

```
┌─────────────────────────────────────────┐
│  Ask                                    │
│ ┌─────────────────────────────────┐  ◉  │  ← search field + mic
│ │  Ask your library…              │     │
│ └─────────────────────────────────┘     │
│                                         │
│  Recent                                 │
│  ┌─────────────────────────────────┐    │
│  │ "stamps last week"              │    │
│  │ "ozempic"                       │    │
│  │ "the keto guest"                │    │
│  └─────────────────────────────────┘    │
│                                         │
│  Topics you've been listening to        │
│  ╭─Longevity─╮ ╭─AI safety─╮ ╭─Keto─╮   │
│                                         │
│  Saved                                  │
│   ☆ "anything by Andrew Huberman"       │
│   ☆ "GLP-1 across all health pods"      │
└─────────────────────────────────────────┘
```

### B. Mid-typing — semantic suggestions

```
┌─────────────────────────────────────────┐
│ ┌─────────────────────────────────┐  ◉  │
│ │  stam▏                          │     │
│ └─────────────────────────────────┘     │
│ ╭This show╮ ╭This week╮ ╭Unlistened╮   │
│                                         │
│  Literal                                │
│   "stamps"  · 1 transcript moment       │
│                                         │
│  ~ Semantic                             │
│   philately                             │
│   stamp collecting                      │
│   "that hobby episode"                  │
│   USPS / postal history                 │
│                                         │
│  Press ↵ to search ·  ◉ to speak        │
└─────────────────────────────────────────┘
```

### C. Mixed results page

```
┌─────────────────────────────────────────┐
│ ┌─stamps─────────────────────────┐  ◉  │
│ ╭This week ✓╮ ╭By Tim Ferriss╮ …       │
│                                         │
│  ▶ CLIP · 1:34                          │
│  ┌─────────────────────────────────┐   │
│  │ ▒▒▒░░▓▓▓▒░ Tim Ferriss Show     │   │
│  │ "…my grandfather's stamp        │   │
│  │  collection taught me…"  47:12  │   │
│  └─────────────────────────────────┘   │
│                                         │
│  EPISODE                                │
│  ┌──┬──────────────────────────────┐   │
│  │▓▓│ Acquired · Hermès            │   │
│  │▓▓│ "...like trading rare stamps"│   │
│  └──┴──────────────────────────────┘   │
│                                         │
│  ❝ TRANSCRIPT MOMENT                    │
│  ┌─────────────────────────────────┐   │
│  │  "philately is the canary in    │   │
│  │   the coal mine of memory"      │   │
│  │   — Lex Fridman · ep 412        │   │
│  └─────────────────────────────────┘   │
│                                         │
│  TOPIC                                  │
│   ▢  Philately — your library wiki      │
└─────────────────────────────────────────┘
```

### D. Clip card peek (long-press)

```
┌─────────────────────────────────────────┐
│  · · · · · · · · · ·  (results dimmed)  │
│ ╔═════════════════════════════════════╗ │
│ ║  Tim Ferriss Show · 47:12           ║ │
│ ║                                     ║ │
│ ║  46:50  …so when I was a kid my     ║ │
│ ║  47:00  grandfather had this whole  ║ │
│ ║  47:12 ▸my grandfather's stamp      ║ │ ← glowing
│ ║         collection taught me        ║ │
│ ║  47:22  patience and the value of   ║ │
│ ║  47:34  small details over time…    ║ │
│ ║                                     ║ │
│ ║  ▶ Play here   ⤴ Open transcript    ║ │
│ ╚═════════════════════════════════════╝ │
└─────────────────────────────────────────┘
```

### E. No-results — fallback to Perplexity

```
┌─────────────────────────────────────────┐
│ ┌─Tegus AI memo July 2026─────┐  ◉      │
│                                         │
│  Not in your library.                   │
│                                         │
│  ┌─────────────────────────────────┐   │
│  │  ◐  Search the open web         │   │
│  │     via Perplexity              │   │
│  │                                 │   │
│  │            [ Search ]           │   │
│  └─────────────────────────────────┘   │
│                                         │
│  Or ask the agent                       │
│   "Find this and add it to my queue"    │
│                                         │
└─────────────────────────────────────────┘
```

### F. Voice search overlay

```
┌─────────────────────────────────────────┐
│           (everything else: 30%)        │
│                                         │
│         ╭─────────────────────╮         │
│         │   ▁▃▆█▆▃▁▃▆█▇▅▂     │         │ ← live waveform
│         ╰─────────────────────╯         │
│                                         │
│   "the part about Ozempic from any     │
│    health podcast last month"           │
│                                         │
│         ◉  Listening · release to send  │
│                                         │
│   ─────────────────────────────         │
│   Streaming results…                    │
│   ▓▓ Peter Attia · 22:14   (matched)    │
└─────────────────────────────────────────┘
```

## 7. Edge Cases

- **Transcription backlog.** Episode is downloaded but transcript is still processing. Show a card with a hairline progress shimmer along the bottom edge: *"Transcript indexing — 73%. We'll re-rank when ready."* Don't hide the episode, don't lie about the match.
- **Ambiguous query** (`bezos` and we have 200 mentions). Top-card is a Person card for Bezos, second card is a Topic/wiki, then clips ranked by recency. Always offer a `Narrow…` chip that opens scope chips.
- **Query targeting unlistened episode.** Result card carries a small `· unplayed` badge in subdued tint. Clicking plays from match position; offer a secondary affordance: *"Start from beginning."*
- **Non-English podcast.** Transcript matches show in original language with a `Translate` glyph. Tap to translate the snippet inline; the underlying timestamp doesn't change.
- **Offline.** Search runs locally against the cached vector index. A subtle banner: *"Offline — searching downloaded episodes only."* Perplexity fallback card disables with the explanation.
- **Query too short** (1 character). Suppress results, show recents only.
- **Stale embeddings** after a model upgrade. Background re-index, but search still works on the old index. Never block the user.

## 8. Accessibility

- **VoiceOver:** every result card reads as `[Type], [Title], [Why-matched snippet], [Duration], double-tap to play, swipe up for actions.` Scope chips are toggleable buttons with `aria-pressed`-equivalent state.
- **Dynamic Type** up to AX5. Cards reflow vertically; cover art proportional, never clipped. No text inside images.
- **Contrast:** body text on glass meets WCAG AA (4.5:1) tested against the most translucent background variant. Highlight on matched terms uses both color *and* a subtle underline for color-blind safety.
- **Reduced Motion:** card streaming becomes a fade; morph transitions become opacity swaps; waveform on the voice overlay becomes a static "listening" indicator.
- **Reduced Transparency:** glass falls back to a solid tinted card with a hairline border. Hierarchy preserved by elevation tokens.
- **One-handed reach:** search input is at the top, but the mic button and submit are reachable by thumb on iPhone Pro Max via a "reach mode" hand-off — pulling down on the field brings the chip rail to the bottom half.
- **Voice control:** `"Search for X"`, `"Play first result"`, `"Narrow to this show"` are all named actions.

## 9. Open Questions / Risks

- **Search vs. Agent — when does the line cross?** A query like *"summarize what they said about Ozempic"* is pure agent territory. *"find clips about Ozempic"* is search. *"what did they say about Ozempic?"* is ambiguous. **Proposal:** detect intent at submit — if the query starts with a verb of synthesis (*summarize, compare, explain, what did, why does*), surface a single card at the top: `Ask the agent instead →`. Search results still render below. The user picks. This is the design's core argument: never auto-route, always show evidence and offer escalation.
- **Should clip results auto-play on tap, or expand inline first?** Risk: auto-play is fast but disorienting. **Proposal:** tap = inline preview play (continues in place); double-tap or swipe = open Now Playing. Validate with a user test of 8.
- **Recents privacy.** Recents are persisted in the App Group store. Add a per-tab clear and an "incognito search" toggle (no logging, no embedding cache). Surface the toggle in the chip rail when active so users don't forget they're in it.
- **Topic chips below the bar — algorithmic or curated?** If algorithmic from listening history, can feel surveillance-y. **Proposal:** label them with provenance — *"Because you listened to Huberman 5x this week"* on long-press. Gives the user an off-ramp.
- **Cross-language semantic search ranking.** A Spanish-language match for an English query needs to compete fairly against literal English matches. Risk of irrelevance. Needs a tunable language-bias parameter, exposed in dev settings before launch.
- **What if Perplexity returns something the user wants in their library?** Add a tertiary action on the Perplexity result card: *"Find a podcast that covers this"* → fires `find_similar_episodes` against the open-web result's topic.

---

**File:** `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-07-search-discovery.md`
