# UX Brief 02 — Library & Subscriptions

> Surface: the user's own collection. Browsing and management, not querying.
> Companion briefs: #1 Now Playing, #7 Semantic Search, #14 Proactive Agent, #15 Liquid Glass System.

---

## 1. Vision

Library is the **calm room** of the app. Where the agent surfaces are alive — pulsing, suggesting — Library is editorial: still, generous, confident. A user opens Library to *orient*, not to *interrogate*. Querying belongs to Search (#7); chatting belongs to Agent (#5). Library answers one question with care: *what's here, and what's ready for me?*

Artwork is the protagonist; show it large, square, generously framed. Liquid Glass is structural only — tab bar, mini-player chrome, OPML sheet, the transcription progress capsule — never wallpaper.

---

## 2. Key User Moments

1. **"What's new on the shows I love?"** — User opens Library, sees their subscriptions in a clean grid; a soft red dot on three of them indicates unplayed episodes. One tap to a show, the freshest episode is the hero card.
2. **"Pick up where I left off."** — Continue Listening rail at the top of the Library tab, three cards max, with progress arcs over artwork. Resumes the exact second.
3. **"I want to add 200 shows from my old player."** — OPML import: drag-drop or file picker, parses, previews shows in a checklist (deselect any), then imports with a real progress bar plus per-show transcription queue indicator. Backgroundable.
4. **"Is the transcript ready yet?"** — On any episode row, a small status capsule: `Downloaded · Transcribing 64%` or `Ready`. The capsule is tappable and reveals queue position when transcription is pending.
5. **"What should I listen to next?"** — Discover sub-tab inside Library shows agent-curated rails: *Because you finished X*, *New from voices you trust*, *One short episode for your commute*. These are **recommendation cards the agent produced offline** — not live nudges (those belong to #14).

---

## 3. Information Architecture

**Proposed global shell** (3 tabs + persistent mini-player):

```
┌─ Tabs (Liquid Glass tab bar, bottom) ─────────────────┐
│      Library         Listen          Agent            │
└───────────────────────────────────────────────────────┘
       ▲ mine          ▲ now/queue     ▲ ask/voice/brief
```

- **Library** — subscriptions, shows, episode lists, downloads, transcription status, OPML, smart playlists, local filters, **and Discover as a sub-tab**. This brief. Discovery without subscriptions is a directory; with them it's a recommendation surface — library-shaped.
- **Listen** — Now Playing + Queue (#1). Mini-player docks above the tab bar across all tabs; contents owned by #1. Library only declares position.
- **Agent** — chat (#5), voice (#6), briefings (#8), proactive feed (#14), search (#7) entered via the persistent ask-bar at the top of this tab.

**Library tab internal IA:**

```
Library
├── Continue Listening (rail, 0–3 cards)
├── Segmented control: [ Subscriptions | Downloads | Discover ]
│
├── Subscriptions (default)
│   ├── View toggle: Grid (artwork-forward, 3-col) | List (dense)
│   ├── Sort: Recent activity · Alphabetical · Most unplayed
│   ├── Filter chips: All · Unplayed · Downloaded · Has transcripts
│   └── Smart Playlists (collapsed section): "Short commutes", "Saved clips", "Tech this week"
│
├── Downloads
│   ├── Storage meter (X of Y GB)
│   ├── Episodes grouped: Ready · Transcribing · Queued · Failed
│   └── Per-row: Cancel / Retry / Delete
│
└── Discover (agent-curated, offline-generated)
    ├── Rails of recommendation cards
    └── "Why this?" tap → expands agent rationale (one paragraph, sourced)
```

Show detail is reached from any subscription. Episode detail (#3) and Wiki (#4) are linked *out* via affordances; Library does not render them.

**Local filter ≠ semantic search.** Library's filter chips and saved-search smart playlists operate on *structured fields* (status, duration, show, date, transcription state). The "ask anything" semantic search bar lives in tab #7.

---

## 4. Visual Treatment

- **Liquid Glass usage:** structural only — tab bar, mini-player chrome, sticky filter bar, OPML sheet, transcription progress capsule. Cards and rows are matte so artwork breathes.
- **Color:** `Color(.systemBackground)` dominant. Show-detail header inherits a dominant tint extracted from artwork, fading to background by 30% height. Dark mode is the target; light mode is a faithful inversion.
- **Typography:** `AppTheme.Typography.largeTitle` for the wordmark; `.title` for show names; `.headline` for episodes; `.caption` mono for durations. Headlines never truncate above 2 lines.
- **Motion:** spring (0.42 / 0.86) for grid → detail with matched-geometry on artwork. Scroll-edge top set to `.automatic`. No bounce on horizontal rails.
- **Density:** 1:1 artwork cards, 3-up iPhone / 4-up iPad. Episode rows = 72pt, respecting Dynamic Type.

---

## 5. Microinteractions

- **Long-press a show** → glass context menu morphs out of the artwork (matched-geometry): Mark all played, Download next 3, Share subscription, Unsubscribe.
- **Swipe left on episode row** → Download / Mark played / Save clip; swipe right → Add to queue. Haptic `.soft` on threshold cross.
- **Tap transcription capsule** → expands inline to a 3-row queue preview with cancel-per-item.
- **Share a subscription** → `pcst://` deep link + QR sheet (Liquid Glass); receiving instance opens a subscribe-confirm modal.

---

## 6. ASCII Wireframes

### A. Library tab — Subscriptions (default, grid)

```
┌─────────────────────────────────────────┐
│  Library                          ⌥  ⊕  │ ← large title, condenses on scroll
│                                         │
│  Continue listening                     │
│  ┌──────┐  ┌──────┐  ┌──────┐           │
│  │ ▓▓░░ │  │ ▓▓▓░ │  │ ▓░░░ │   →       │ ← progress arcs over art
│  │ Art  │  │ Art  │  │ Art  │           │
│  └──────┘  └──────┘  └──────┘           │
│  Lex 412   Tim 803   Dwarkesh 91        │
│                                         │
│ [ Subscriptions | Downloads | Discover ]│ ← segmented, glass-tinted
│  All · Unplayed · Downloaded · Trans.   │ ← filter chips
│                                         │
│  ┌────┐ ┌────┐ ┌────┐                   │
│  │Art●│ │Art │ │Art●│                   │ ← red dot = unplayed
│  └────┘ └────┘ └────┘                   │
│  Show1  Show2  Show3                    │
│  ┌────┐ ┌────┐ ┌────┐                   │
│  │Art │ │Art │ │Art │                   │
│  └────┘ └────┘ └────┘                   │
│                                         │
│  ▾ Smart playlists (3)                  │
│                                         │
├─────────────────────────────────────────┤
│ ▶  Tim Ferriss · Keto deep dive  12:04  │ ← mini-player (#1's content)
├─────────────────────────────────────────┤
│   ◉ Library      ○ Listen      ○ Agent │ ← Liquid Glass tab bar (3-up)
└─────────────────────────────────────────┘
```

### B. Show detail

```
┌─────────────────────────────────────────┐
│  ← Back                          ⋯      │
│                                         │
│      ┌──────────────────┐               │
│      │                  │               │ ← 220pt artwork, gradient
│      │     ARTWORK      │               │   bleeds from extracted tint
│      │                  │               │
│      └──────────────────┘               │
│      Tim Ferriss Show                   │
│      812 episodes · Subscribed          │
│      ✺ Wiki ready  ✓ Transcripts on    │ ← affordances → #4, #3
│                                         │
│   [▶ Play latest]   [⬇ Download next 3] │
│                                         │
│  Episodes  ─  All · Unplayed · Down…    │
│  ┌─────────────────────────────────┐    │
│  │ #812  Keto with Dr. Attia       │    │
│  │ 2:14:30 · ✓ Played · ⌬ Trans    │    │
│  └─────────────────────────────────┘    │
│  ┌─────────────────────────────────┐    │
│  │ #811  ● Building resilience     │    │
│  │ 1:47:02 · ⬇ 64% · ⌬ Queue #3   │    │
│  └─────────────────────────────────┘    │
└─────────────────────────────────────────┘
```

### C. OPML import flow (sheet)

```
┌─────────────────────────────────────────┐
│  Import from OPML            ✕         │
│  ─────────────────────────────────────  │
│  subscriptions.opml · 247 feeds parsed  │
│                                         │
│  Select shows to import (247 / 247)     │
│  [Select all] [Deselect all]            │
│                                         │
│  ☑ Tim Ferriss        812 ep · existing │
│  ☑ Lex Fridman        458 ep            │
│  ☑ Dwarkesh Patel      94 ep            │
│  ☐ Joe Rogan         2104 ep · large    │
│  ☑ ...                                  │
│                                         │
│  Transcription: ◉ Auto for new   ○ Off  │
│                                         │
│  [ Import 246 shows  →  ]               │ ← glassProminent button
└─────────────────────────────────────────┘
```

### D. Mass-import in progress

```
┌─────────────────────────────────────────┐
│  Importing your library                 │
│  ▓▓▓▓▓▓▓▓▓▓░░░░░░░░  138 / 246          │
│  Fetching feeds · 12 in flight          │
│                                         │
│  Transcription queue                    │
│  ▓▓▓░░░░░░░░░░░░░░░░  47 / 312 episodes │
│  Est. 4h 12m on Wi-Fi                   │
│                                         │
│  [Run in background]   [Pause]          │
│                                         │
│  Recently added:                        │
│   ✓ Tim Ferriss     ✓ Lex Fridman       │
│   ⌬ Dwarkesh (transcribing 3)           │
│   ⌬ Acquired (transcribing 1)           │
└─────────────────────────────────────────┘
```

### E. Downloads & transcription status

```
┌─────────────────────────────────────────┐
│  Downloads          4.2 of 32 GB used   │
│  ▓▓░░░░░░░░░░░░░░░░░░░░░░░░░░░░         │
│                                         │
│  Transcribing (3)                       │
│  ┌─────────────────────────────────┐    │
│  │ Lex 458 · How AI changes…   72% │    │
│  │ ⌬ ──────────●───────────        │    │
│  └─────────────────────────────────┘    │
│  ┌─────────────────────────────────┐    │
│  │ Dwarkesh 94 · Gwern         13% │    │
│  └─────────────────────────────────┘    │
│                                         │
│  Ready (12)                             │
│  ┌─────────────────────────────────┐    │
│  │ Tim 812 · Keto              ✓   │    │
│  └─────────────────────────────────┘    │
│                                         │
│  Failed (1)                             │
│  ┌─────────────────────────────────┐    │
│  │ Acquired 198 · Costco       ↻   │    │
│  └─────────────────────────────────┘    │
└─────────────────────────────────────────┘
```

---

## 7. Edge Cases

- **Empty library** — Editorial hero: rounded wordmark, one sentence ("Your shows live here."), three stacked affordances: *Import OPML*, *Paste RSS*, *Browse Discover*. The agent does **not** speak here.
- **No network** — Subscriptions render from cache with an `Offline` glass chip top-right. New-episode badges hidden. Downloads remain fully functional.
- **Mass-import in progress** — Persistent glass progress capsule docks below the mini-player, tap-expands to wireframe D. Backgrounding hands off to an iOS Live Activity.
- **Transcription queue full / quota exhausted** — Non-modal banner in Downloads: *"Transcription paused: monthly quota reached."* Actions: *Use on-device* / *Upgrade*. Never blocks playback.
- **OPML malformed** — First 5 parse errors with line numbers; offer *Import what we could parse (212 of 247)*.
- **Subscription removed by feed** — Show dimmed with `Feed gone` label; episodes still listenable; *Find replacement* hands off to #7.

---

## 8. Accessibility

- **Dynamic Type** to AX5; grid reflows to 2-up then 1-up. No truncation of show titles below `.title`.
- **VoiceOver** rotor: episodes grouped per show; status capsule reads as compound label: *"Episode 812. Keto with Doctor Attia. Two hours fourteen minutes. Played. Transcript available."*
- **Audio-first / screen-off** — every list row reachable via Siri Shortcut intents (`Open show <name>`, `Resume <show>`). Transcription status announced via a single haptic when complete (opt-in).
- **Color independence** — unplayed dot has a redundant SF Symbol (`circle.fill` + bold weight on title). Status icons (downloaded, transcribed) all carry text in their accessibility label.
- **Contrast** — all text on artwork-tinted gradient meets WCAG AA (4.5:1); we measure at gradient midpoint and clamp tint luminance.
- **Reduce Motion** — matched geometry transitions become cross-fades; rails do not parallax.
- **Hit targets** — minimum 44×44pt; swipe actions have a one-second undo toast.

---

## 9. Open Questions / Risks

1. **Recommendation surface boundary with #14** — recommendations are *pull* (user navigates to Discover); nudges are *push* (#14). Same agent, different surface. Need explicit handshake on regeneration cadence and provenance display.
2. **Three-axis episode status** (downloaded × played × transcribed) — risks visual noise. Current proposal: icon row with text-on-tap. Validate with usability test.
3. **Smart Playlists scope** — local structured filters here; agent-generated playlists live in Discover with provenance, never mixed.
4. **Sharing a subscription via Nostr** — Nostr event (per template's friend system) or `pcst://` universal link? Coordinate with #12.
5. **OPML at 2k+ feeds** — perf budget for parse + initial fetch + transcription queueing. Needs engineer-agent input.

---

File: `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-02-library.md`
