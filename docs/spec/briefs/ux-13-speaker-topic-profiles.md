# UX Brief 13 — Speaker & Topic Profiles

> Surface: rich destination pages for the *people* and *concepts* that recur across the user's library.
> Companion briefs: #3 Episode/Transcript, #4 Wiki, #8 Briefings, #9 Cross-Episode Threading, #14 Proactive Agent, #15 Liquid Glass System.

---

## 1. Vision

A podcast library is not a collection of episodes — it is a collection of *voices* and *ideas*. The same names recur across years; the same arguments mutate from show to show. Today's apps are blind to this: each episode is an island, each guest a string of letters in a description field. We refuse that. **A speaker is a first-class noun in this app. So is a topic.** Both deserve a destination — a magazine-quality page the user can return to, follow, and reason about.

These profiles are *destinations*, not *summaries*. The wiki tab (#4) holds the encyclopedic, summarized page — three-line bio, headline links — and *opens into* the profile pages owned here. The wiki is the index; we are the article. When a user taps a guest name on a wiki page, a transcript chip, or a speaker rail in a show detail, they land here: portrait, bio, every appearance, every topic they keep returning to, how their position has *moved* on those topics, and a one-tap path to a personalized briefing. Topic profiles do the symmetrical job: definition, who discusses it, when it surfaced first, where speakers contradict each other, what subtopics it nests.

Editorial typography, generous whitespace, restrained Liquid Glass on chrome only. The profile page should feel like a long-form *New Yorker* contributor page or a Bloomberg topic dossier — not a social media profile.

---

## 2. Key User Moments

1. **"Wait, who is this guest?"** — User long-presses a speaker chip in the transcript (#3). A *peek sheet* rises with portrait, two-line bio, a single sentence of context ("3rd appearance on this show, 11 across your library"), and three actions: *Open profile*, *Brief me*, *Follow*. Releasing without choosing dismisses.
2. **"Catch me up on Andrej Karpathy."** — User opens the speaker profile, taps *Brief me on Andrej*. Hands off to #8 with the speaker pre-scoped; #8 generates a 6-minute audio briefing of his arc across the user's library.
3. **"Has Tyler Cowen changed his mind on AI risk?"** — User opens the topic *AI risk*, scrolls to *Stance evolution by speaker*, taps Cowen's row. A timeline of his quoted positions across episodes unfolds chronologically — each quote a card that plays the source clip on tap.
4. **"Tell me when this person shows up again."** — Toggle *Follow speaker* on the profile. New appearances surface in #14's proactive feed; a single haptic ping (configurable) on next detection.
5. **"What does my library actually say about Ozempic?"** — Topic profile *Ozempic* shows: definition, 23 episodes, 14 speakers, contradictions panel ("Attia: cautious endorsement / Lustig: skeptical"), parent topic *GLP-1 agonists*, subtopic *muscle loss concerns*.
6. **"This guest's name has a typo and the photo is wrong."** — User taps the (i) on the portrait, sees source attribution, can *Suggest correction* (writes a Nostr event per #12) or *Replace photo* with an upload.

---

## 3. Information Architecture

**Where these pages live.** Profiles are reached from anywhere a name or topic appears: transcript chips (#3), wiki cross-links (#4), agent chat citations (#5), search results (#7), briefing transcripts (#8), threading dots (#9), proactive cards (#14). They are *not* a top-level tab — they are pushed onto the navigation stack of whichever tab summoned them, with the iOS-26 back gesture and a glass-chrome top bar that condenses the speaker name into the title slot on scroll.

**Boundary with Wiki (#4).** The wiki tab renders a *summary card* for each person/topic: 3-line bio, the 5 most-cited episodes, a single "View full profile →" affordance. That affordance pushes onto the wiki tab's nav stack into our profile page. Same destination, different entry. We own the page; #4 owns the index. Concretely: wiki person cards never expand inline beyond the summary card; the moment a user wants depth, they are on our surface. This keeps #4 fast and skimmable.

**Speaker profile IA:**

```
Speaker · Andrej Karpathy
├── Header
│   ├── Portrait (3:4 hero, gradient bleed from extracted tint)
│   ├── Name · role pill ("Researcher · ex-OpenAI")
│   ├── Stats row: 11 appearances · 7 shows · followed since Apr
│   └── Actions: [Follow ●]  [Brief me]  [Share]
│
├── Bio (LLM-generated, sourced; Perplexity cite footer)
│
├── Most-discussed topics (chip cloud, weighted)
│
├── Stance evolution (per topic, expandable)
│   └── e.g. "On agentic systems → 4 quotes across 2023–2026"
│
├── Recent appearances (rail, reverse-chronological)
│
├── Best clips (agent-curated, 3–5)
│
└── Sources & corrections (footer)
```

**Topic profile IA:**

```
Topic · Ozempic
├── Header
│   ├── Hero illustration (generated, restrained)
│   ├── Title + parent topic crumb (← GLP-1 agonists)
│   └── Actions: [Follow ●]  [Brief me]  [Share]
│
├── Definition (LLM, sourced)
│
├── Speakers who discuss it (faces row, sortable by frequency / recency)
│
├── Episodes that discuss it (rail)
│
├── Timeline (when it first surfaced in your library, key inflection points)
│
├── Contradictions across speakers (paired-quote cards)
│
├── Subtopics & parent topics (graph chips)
│
└── Sources & corrections
```

**Follow / notification settings** live as a sheet from the *Follow ●* pill, not a separate screen:

```
Follow Andrej Karpathy
├── New appearance detected:  [On / Off]
├── Delivery: ◉ Proactive feed (#14)   ○ Push   ○ Both
├── Confidence threshold: ─●────── (low / medium / high)
└── Mute for: 1 day · 1 week · forever
```

The threshold maps to the speaker-identity confidence score (open question 1). Default is *medium*; a low setting will surface plausible matches you can confirm or reject, which feeds the resolver back.

---

## 4. Visual Treatment

- **Liquid Glass usage:** structural only — top nav bar, the *Follow* pill, the *Brief me* CTA (`.glassProminent`), the peek sheet from transcript long-press, the corrections sheet. Portraits, quote cards, timeline cards, and chip clouds stay matte. The page is paper; glass is the chrome on the paper.
- **Photo treatment:** hero portrait at 3:4, 280pt tall on iPhone, masked with a 24pt corner radius and a soft gradient bleed of the photo's extracted dominant color into the page background (clamp luminance for AA contrast, identical algorithm to #2's show-detail header). When no photo is available: an *initials monogram* (two letters, `.largeTitle` rounded bold) inside a circular glass disc, tinted from the speaker's most-frequent show's accent. Never use a generic silhouette stock graphic.
- **Typography:** wordmark name in `AppTheme.Typography.largeTitle` (rounded, bold, condenses to nav title on scroll); role pill in `.subheadline` mono; bio in `.body` at 1.45 line-height with a 64-character measure cap on iPhone, 72 on iPad. Pull-quote cards in `.title3` serif-leaning rounded with a 4pt accent rule on the leading edge tinted from the speaker's color. Source attributions in `.caption2` small-caps.
- **Color:** dominant surface is `Color(.systemBackground)`. Accent inherited from the speaker's portrait or the topic's parent-graph color, applied to the leading rule on quote cards, the *Follow* dot, and the timeline spine. One color per page, used with restraint.
- **Motion:** matched-geometry on the portrait when arriving from a transcript chip — the 28pt chip morphs to the 280pt hero across a spring (response 0.48, damping 0.84). Stance-evolution timeline cards stagger in at 32ms when the section enters viewport. Peek sheet uses `.glass` interactive with a haptic `.soft` on rise and dismissal. No parallax on rails; the page is editorial, not playful.
- **Density:** sections separated by 32pt vertical whitespace; quote cards 16pt internal padding, 20pt corner; faces row 56pt circles with 12pt gutter.

---

## 5. Microinteractions

- **Long-press a speaker chip in transcript (#3)** → glass *peek sheet* morphs out of the chip (matched geometry on the chip's face/initials), 240pt tall, three actions inline. Release without choosing dismisses; tap *Open profile* commits the push. Haptic `.soft` on rise.
- **Tap *Follow ●*** → pill morphs from outlined to filled, dot pulses once, and the settings sheet *peeks up 88pt* (just the toggle row visible) — pull up to expand to full settings, or release to accept defaults. This is the "follow without ceremony" pattern; users who want depth get it, users who don't aren't punished.
- **Tap *Brief me*** → CTA morphs into a progress capsule, hands off to #8 with `{scope: speaker:<id>, length: 6m}` pre-scoped. Returning from #8 restores the CTA.
- **Tap a stance-evolution quote card** → expands inline to play the source clip (12s pre-roll, 30s body, 5s post-roll); a small *Open at this moment* affordance pushes to #3 at the exact timestamp. Tapping outside collapses.
- **Long-press a topic chip** anywhere → same peek treatment as speakers.
- **Pull down on profile header** → reveals the *Sources & corrections* drawer without leaving the page; an editorial gesture, not a refresh.
- **Two-finger tap on portrait** → reveals attribution overlay (photo source, license, fetched date). Shortcut for power users; equivalent affordance lives in the (i) button.

---

## 6. ASCII Wireframes

### A. Speaker profile

```
┌─────────────────────────────────────────┐
│  ←                              ⓘ  ⋯    │ ← glass nav bar
│                                         │
│           ┌──────────────┐              │
│           │              │              │
│           │   PORTRAIT   │  3:4, tint   │
│           │              │  bleeds down │
│           └──────────────┘              │
│                                         │
│        Andrej Karpathy                  │ ← largeTitle, condenses
│        Researcher · ex-OpenAI           │ ← role pill
│        11 appearances · 7 shows         │
│                                         │
│   [● Follow]  [✺ Brief me]  [↗ Share]   │ ← glass pills
│                                         │
│  ─────────────────────────────────────  │
│  Bio                                    │
│  Andrej is a researcher focused on…     │
│  Source: Perplexity · May 7              │
│                                         │
│  Most-discussed topics                  │
│  ▢ agentic systems  ▢ tokenizers        │
│  ▢ self-driving     ▢ scaling laws      │
│                                         │
│  Stance evolution                       │
│   ▾ On agentic systems  (4 quotes)      │
│   ▸ On scaling laws     (3 quotes)      │
│                                         │
│  Recent appearances                     │
│  ┌─────┐ ┌─────┐ ┌─────┐                │
│  │ Ep  │ │ Ep  │ │ Ep  │   →            │
│  └─────┘ └─────┘ └─────┘                │
│                                         │
│  Best clips (3)                         │
│  ▶ "The bitter lesson, restated"  2:14  │
│  ▶ "Why agents are hard"          3:47  │
│                                         │
│  Sources & corrections        ▾         │
└─────────────────────────────────────────┘
```

### B. Topic profile

```
┌─────────────────────────────────────────┐
│  ←  GLP-1 agonists › Ozempic     ⋯      │ ← parent crumb in nav
│                                         │
│        ╭──────────────╮                 │
│        │  topic hero  │                 │
│        ╰──────────────╯                 │
│                                         │
│  Ozempic                                │
│  Discussed in 23 episodes · 14 voices   │
│  [● Follow]  [✺ Brief me]               │
│                                         │
│  Definition                             │
│  Brand name for semaglutide, a GLP-1…   │
│  Sources: 3                              │
│                                         │
│  Speakers who discuss it                │
│  (◉)(◉)(◉)(◉)(◉)(◉)(◉)  + 7  →          │ ← faces row
│                                         │
│  Timeline                               │
│  │ Jul '23  first surfaces · Huberman   │
│  ●                                       │
│  │ Nov '23  Lustig: skeptical            │
│  ●                                       │
│  │ Mar '24  Attia: cautious endorsement  │
│  ●                                       │
│                                         │
│  Contradictions                         │
│  ┌─────────────────────────────────┐    │
│  │ Attia: "the data is good, but…" │    │
│  │ ─────────────────────────────── │    │
│  │ Lustig: "we're medicating a…"   │    │
│  │ [Hear both]                     │    │
│  └─────────────────────────────────┘    │
│                                         │
│  Related                                │
│  ↑ GLP-1 agonists                       │
│  ↓ muscle loss · injection cadence      │
└─────────────────────────────────────────┘
```

### C. Follow settings (peek-up sheet)

```
┌─────────────────────────────────────────┐
│  ╭──╮                                   │ ← drag handle, glass
│                                         │
│  Follow Andrej Karpathy                 │
│  ─────────────────────────────────────  │
│  New appearance detected     [On  ●  ]  │
│                                         │
│  Deliver via                            │
│   ◉ Proactive feed                      │
│   ○ Push notification                   │
│   ○ Both                                │
│                                         │
│  Confidence threshold                   │
│   low ─────●────── high                 │
│   "Surface plausible matches I confirm" │
│                                         │
│  Mute for                               │
│   [1 day]  [1 week]  [forever]          │
│                                         │
│  [ Done ]                               │
└─────────────────────────────────────────┘
```

### D. Brief-me from profile

```
┌─────────────────────────────────────────┐
│  ←  Briefing                            │
│                                         │
│  Andrej Karpathy across your library    │
│  6 minutes · 11 episodes · 4 topics     │
│                                         │
│        ╭───────────────────╮            │
│        │   ◉  generating   │            │
│        │   ▓▓▓▓▓░░░░░  47% │            │
│        ╰───────────────────╯            │
│                                         │
│  Will cover:                            │
│   · arc on agentic systems              │
│   · scaling-laws position drift         │
│   · 2 best clips                        │
│   · 1 unresolved contradiction          │
│                                         │
│  [Cancel]              [Play when ready]│
└─────────────────────────────────────────┘
```

### E. Peek-from-transcript (long-press chip)

```
        ┌───────────────────────────────┐
        │  ╭──╮                         │
        │                               │
        │  ◉  Andrej Karpathy           │
        │  Researcher · ex-OpenAI       │
        │  3rd time on this show ·      │
        │  11 across your library       │
        │                               │
        │  [Open profile]               │
        │  [✺ Brief me]                 │
        │  [● Follow]                   │
        └───────────────────────────────┘
                  ▲
        long-press on speaker chip
        in transcript line
```

---

## 7. Edge Cases

- **Speaker has only one episode in your library** — the *Most-discussed topics* and *Stance evolution* sections are suppressed; we degrade to *Topics in this episode* (chip cloud from the single transcript) and *Bio* only. The page does not pretend to richness it does not have. *Follow* is still offered: "We'll surface their next appearance."
- **No photo available** — initials monogram disc as described in §4 (never a generic silhouette). The (i) button reads "No portrait sourced. Tap to add." A user-supplied photo writes locally, never to a shared cache (licensing risk).
- **Ambiguous identity (two people, same name)** — the resolver returns >1 candidate. Page opens to a *disambiguation chooser*: 2–3 candidate cards (portrait, role, last show context). The user's pick is remembered per-show context as a tiebreaker; the resolver is updated. If the chooser cannot be presented (incoming deep link), default to the highest-confidence candidate with a non-modal banner: *"Did we pick the right Sarah Chen? [Switch]"*.
- **Unknown guest** — when diarization detected a voice but identity resolution failed, the chip in transcript reads *Unknown speaker A*. Long-pressing offers *Help name this voice*: a 4s clip plays, three candidate names are suggested (from RSS show notes, Perplexity, fuzzy match against followed speakers), and the user can pick or type a name. A profile is then created lazily.
- **Topic with stale parent graph** — if a parent topic was deprecated in a wiki regeneration, the crumb gracefully renders as a flat title and a small *graph rebuilding* glass chip appears in the corner.
- **Followed speaker disappears for 90 days** — a one-time card surfaces in #14: *"Andrej hasn't appeared in your shows since Feb. Keep following?"* with *Yes / Mute*. We do not auto-unfollow.

---

## 8. Accessibility

- **Dynamic Type** to AX5; portrait shrinks gracefully to 200pt and the bio measure cap relaxes; never truncate names.
- **VoiceOver** rotor: page exposes *Header*, *Bio*, *Topics*, *Stance evolution*, *Appearances*, *Clips*, *Sources* as headings. Each stance-evolution card reads as: *"Quote, March 2024. On agentic systems. Andrej Karpathy. Two minutes fourteen seconds. Double-tap to play."* The peek sheet announces as *"Speaker preview. Andrej Karpathy. 3 actions available."*
- **Audio-first / screen-off** — *Follow*, *Brief me*, and *Open profile* exposed as Siri Shortcut intents (`Follow <name>`, `Brief me on <name>`, `Open <name>'s profile`). Stance-evolution timeline supports a linear *Read all quotes* rotor mode that plays each clip back-to-back with TTS interstitials.
- **Color independence** — *Follow* state distinguished by both color and glyph (outline ring vs. filled dot); contradiction pairs separated by a horizontal rule, not just color.
- **Contrast** — portrait gradient bleed clamps luminance so headline meets WCAG AA (4.5:1) at the gradient midpoint; quote-card accent rule is decorative only, never the sole signifier.
- **Reduce Motion** — matched-geometry chip→hero collapses to a cross-fade; timeline cards appear without stagger.
- **Hit targets** — *Follow* pill, *Brief me* CTA, peek-sheet actions all ≥44×44pt with 8pt outer hit slop.
- **Haptics** — *Follow* toggle: `.success` once. New-appearance ping (when followed): single `.soft`, opt-in.

---

## 9. Open Questions / Risks

1. **Speaker identity resolution.** Diarization gives us *voices*; matching a voice to a *named identity* across shows is hard (homonyms, nicknames, mid-show "I'm joined by…" cues, host/guest role flips). Proposal: a tiered resolver — RSS show-notes first, then transcript NER + co-reference, then voiceprint clustering across the library, with the user's disambiguation choices fed back. Confidence score surfaced via the threshold slider in §3. Coordinate with engineer + #9.
2. **Photo licensing.** Scraped portraits are a copyright minefield. Proposal: prefer Perplexity-cited Wikimedia/Commons sources with explicit license metadata, fall back to the speaker's verified social avatar (per-platform terms apply), fall back to monogram. Never mass-cache without per-image attribution. Owner's *Suggest correction* path lets a guest replace their own photo. Legal review required before ship.
3. **"Best clips" curation cost.** Generating clips per speaker requires a re-ranker pass over their transcript chunks; expensive at library scale. Proposal: lazy generation on first profile view, cached, regenerated on follow + new appearance. Coordinate with #8.
4. **Topic graph drift.** Wiki regeneration changes parent/subtopic edges; deep links to a topic page may land on a now-merged topic. Proposal: redirect table written on every wiki rebuild; old IDs resolve to canonical IDs with a banner.
5. **Privacy of "follow."** A followed speaker is local-only by default. If the user opts to share follows over Nostr (per #12), what does the event look like? Proposal: an opaque follow event keyed to a normalized speaker ID, no portrait, with an opt-in profile-publish step. Coordinate with #12.
6. **Self-stance feature creep.** *Stance evolution* is editorially powerful but risks misrepresentation (a quote out of context). Proposal: every quote card carries a *Hear in context* affordance that plays 12s of pre-roll audio; no quote ever appears stripped of its tap-to-source path.
7. **Coordination with #14.** Follow lives here; delivery lives there. We need a clean event schema (`speaker_appeared`, `topic_referenced`, with confidence and source-episode-id). I propose drafting it jointly with #14's owner.
8. **Wiki ↔ profile sync cost.** When wiki regenerates a person summary, our profile's bio should match (same source, same revision). Proposal: bio is a *view onto* the wiki's bio field; we render, we do not duplicate.

---

File: `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-13-speaker-topic-profiles.md`
