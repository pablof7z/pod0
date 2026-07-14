# UX-15 — Liquid Glass Design System & Motion Language

> Ground truth for every surface designer. If a token, material, motion curve, haptic, or signal is not here, it does not exist. If a surface contradicts this brief, the surface is wrong.

---

## 1. Vision

This is an **editorial podcast player with a living agent inside it**. Visually, it borrows from periodicals — generous whitespace, considered typography, photographic restraint — and from cinema — depth, parallax, motion that *means* something. The Liquid Glass material is not decoration; it is the *seam* between content (audio, transcript, wiki) and intent (the agent, the player, the friend). Light bends around what is alive — the now-playing bar refracts the artwork beneath it, the agent orb refracts the page it sits on, a friend's incoming message refracts a warm-amber gradient that's only theirs. The app should feel **calm by default, alive on demand**: silent on the home screen, breathing during playback, electrified during voice conversation.

---

## 2. Color

Two-source palette: **System neutrals** for chrome/text, **Brand semantics** for identity (agent, friend, now-playing, wiki). All hex values are sRGB. Light/dark pairs target WCAG AA (4.5:1) for body text against their natural surface — every pair must be verified with a contrast checker before shipping; any failures get nudged toward the canvas extremes.

### 2.1 Neutrals (chrome, text, hairlines)

| Role            | Light            | Dark             | Notes                                        |
|-----------------|------------------|------------------|----------------------------------------------|
| `bg.canvas`     | `#FAFAF7`        | `#0B0B0E`        | App background (warm-paper / near-black ink) |
| `bg.elevated`   | `#FFFFFF`        | `#16161B`        | Cards, sheets — the "lifted" plane           |
| `bg.sunken`     | `#F2F1EC`        | `#08080A`        | Inset wells, search field, code blocks       |
| `text.primary`  | `#0E0E12`        | `#F5F4EE`        | Body text                                    |
| `text.secondary`| `#5C5C66`        | `#A8A8B2`        | Metadata, captions                           |
| `text.tertiary` | `#9A9AA4`        | `#6E6E78`        | Timestamps, hints                            |
| `hairline`      | `rgba(0,0,0,.10)`| `rgba(255,255,255,.12)` | 0.5pt strokes between rows            |
| `divider.bold`  | `rgba(0,0,0,.18)`| `rgba(255,255,255,.20)` | Section breaks                          |

### 2.2 Identity tints (semantic — never mix)

| Role             | Light            | Dark             | Used for                                       |
|------------------|------------------|------------------|------------------------------------------------|
| `accent.player`  | `#E94B2B`        | `#FF6A4A`        | Now-playing identity — *warm copper*           |
| `accent.agent`   | `#5B3FE0` → `#2872F0` (gradient) | `#7A5BFF` → `#4D8FFF` | Agent identity — *electric indigo→azure*  |
| `accent.wiki`    | `#1F6E55`        | `#46C29A`        | LLM wiki citations, knowledge surfaces — *moss*|
| `accent.friend`  | `#D9892F`        | `#F2B45C`        | Nostr friend / friend-agent action — *amber*   |
| `accent.live`    | `#C72D4D`        | `#FF5577`        | Recording / "agent listening" — *signal red*   |

### 2.3 Semantic state

| Role     | Light     | Dark      | Used for                    |
|----------|-----------|-----------|-----------------------------|
| `success`| `#2E8F5C` | `#5BD699` | Download complete, saved    |
| `warning`| `#C68A14` | `#F0BE4B` | Slow network, partial sync  |
| `error`  | `#C8302A` | `#FF6F66` | Failure, destructive        |
| `info`   | `#2B6FD6` | `#6FA9FF` | Toasts, neutral notices     |

### 2.4 Glass tint overlays (alpha applied over blur)

| Tier            | Light tint                  | Dark tint                   | Use                                |
|-----------------|----------------------------|-----------------------------|------------------------------------|
| `glass.clear`   | `rgba(255,255,255,.06)`    | `rgba(255,255,255,.04)`     | Pure refraction, no color signal   |
| `glass.player`  | `rgba(233,75,43,.10)`      | `rgba(255,106,74,.12)`      | Player surfaces                    |
| `glass.agent`   | `rgba(91,63,224,.10)`      | `rgba(122,91,255,.14)`      | Agent surfaces                     |
| `glass.friend`  | `rgba(217,137,47,.10)`     | `rgba(242,180,92,.12)`      | Friend-agent message              |

---

## 3. Typography

**Primary face: SF Pro (system).** Editorial display headings use **New York (system serif)** for hero titles only — episode title on Now Playing, wiki article titles, briefing intros. SF Pro Rounded is reserved for chips, badges, and the agent voice — it carries the "warm" register. Mono is **SF Mono** for timestamps and code.

| Token                  | Face          | Weight     | Size / Leading / Tracking | Use                                |
|------------------------|---------------|------------|---------------------------|------------------------------------|
| `display.hero`         | New York      | Semibold   | 34 / 38 / -0.4           | Now Playing title, wiki article H1 |
| `display.large`        | New York      | Medium     | 28 / 32 / -0.3           | Episode detail title               |
| `title.lg`             | SF Pro        | Bold       | 22 / 26 / -0.2           | Section titles, sheet titles       |
| `title.md`             | SF Pro Rounded| Semibold   | 19 / 23 / -0.1           | Card titles                        |
| `headline`             | SF Pro Rounded| Semibold   | 17 / 22 / 0              | Row headlines, agent messages      |
| `body`                 | SF Pro        | Regular    | 17 / 24 / 0              | Reading text                       |
| `body.emphasis`        | SF Pro        | Semibold   | 17 / 24 / 0              | Inline emphasis                    |
| `subhead`              | SF Pro        | Regular    | 15 / 20 / 0              | Show name, secondary metadata      |
| `caption`              | SF Pro        | Medium     | 13 / 17 / +0.1           | Timestamps, chips, captions        |
| `caption.small`        | SF Pro        | Medium     | 11 / 14 / +0.2           | Pill labels, badges                |
| `mono.timestamp`       | SF Mono       | Medium     | 13 / 17 / 0              | `00:42:18` transcript timestamps   |
| `mono.code`            | SF Mono       | Regular    | 13 / 19 / 0              | Code/data, debug                   |

Dynamic Type: every token must scale. New York is reserved for sizes ≥ 19pt (it loses character at smaller sizes). No tracking values below -0.4 — lower than that becomes muddy on retina at small sizes.

---

## 4. Iconography

- **Primary set: SF Symbols 6**, `.regular` weight at body sizes, `.semibold` inside chips and on the player. Hierarchical rendering by default; multicolor only for the now-playing waveform and the agent orb.
- **Stroke width**: 1.5pt for custom glyphs at 24pt frame, 1.25pt at 20pt, 2pt at 32pt+. **Never** mix outline and filled icons in the same row.
- **State convention**: outline = inactive, filled = active. Play→Pause toggles via `play.fill` ↔ `pause.fill` (both filled — they're both *active* states; the empty-circle play is for "muted/unloaded" only).
- **Custom glyphs**: agent orb (live), waveform-with-cursor (transcript scrubber), nostr-zap (friend), wiki-leaf (knowledge).
- **Icon padding inside chips**: icon + 6pt + label + 4pt edge — never less.

---

## 5. Materials — Liquid Glass Tier System

Five tiers. Choose by **what is behind the glass** and **how alive the surface is**.

> Blur radius is **not an API parameter** in iOS 26 — `.glassEffect()` computes it from system context. The "character" column below describes visual depth qualitatively for designer reference; do not pass numeric blur values in Swift.

| Tier              | API                                              | Visual character     | Tint                                | When to use                                                                 |
|-------------------|--------------------------------------------------|----------------------|-------------------------------------|------------------------------------------------------------------------------|
| **T0 Hairline**   | none — solid `bg.elevated` + 0.5pt hairline      | flat, paper          | none                                | Pure reading surfaces (transcript body, wiki body) — glass would distract   |
| **T1 Clear**      | `.glassEffect(.regular, in: rect)`               | shallow, refractive  | `glass.clear`                       | Default toolbars, segment controls, secondary chips                          |
| **T2 Tinted**     | `.glassEffect(.regular.tint(c), in: rect)`       | shallow + identity   | identity tint (player/agent/friend) | Now-playing mini-bar, agent reply bubble, friend incoming                   |
| **T3 Interactive**| `.glassEffect(.regular.tint(c).interactive())`   | shallow + light reflect | identity tint                     | Buttons, agent orb, draggable scrubber thumb                                |
| **T4 Cinematic**  | `GlassEffectContainer` + tinted children + parallax | deep, layered     | layered identity tints              | Now Playing full screen, voice mode, briefing player                        |

**Rules:**
1. **Always wrap multiple T2/T3 elements in `GlassEffectContainer(spacing:)`** — required for morph and perf. Default spacing 24pt; bump to 40pt when elements should *not* merge.
2. **Never stack T2 over T2** — the second blur turns to mud. If you need depth, T0 underneath, T2 on top.
3. **Refraction is auto** in iOS 26 — do not fake it with manual gradients. Glass already responds to content beneath.
4. **Light response (auto-mode)** — the system handles light/dark adaptation. Do not hardcode opacities; use the tint tokens above which already encode the right alphas per scheme.
5. **Tint saturation budget**: never exceed the alphas in §2.4. Over-tinting kills the "glass" character.
6. **Edges**: glass surfaces use `rect(cornerRadius:)` from the corner scale — `Corner.lg` (16) for cards, `Corner.xl` (24) for sheets, `Corner.bubble` (18) for chat bubbles, `Corner.pill` (14) for chips. Never custom values.

---

## 6. Components

### 6.1 Buttons

> **Copper is reserved.** `accentPlayer` (copper) only ever appears on now-playing surfaces and the playerOrb. Every other primary CTA in the app uses neutral onyx/paper or — when the action is agent-mediated — the agent gradient.

| Style              | Spec                                                                                       | Use                                |
|--------------------|--------------------------------------------------------------------------------------------|------------------------------------|
| `.glassProminent`  | T3 clear over solid `text.primary` fill, white label, `headline`, 14h / 22v / 14r          | Primary CTAs (Subscribe, Save, Continue) |
| `.glassAgent`      | T3 + `accentAgent` gradient tint, white label, `headline`, 14h / 22v / 14r                 | Agent-initiated CTAs ("Ask the agent", "Generate briefing") |
| `.glass`           | T3 clear, body-emphasis, 12h / 20v / 14r                                                   | Secondary in toolbars              |
| `.pressable`       | No glass; 0.96 scale + 0.80 opacity press feedback, snappy curve                            | List rows, cells inside cards      |
| `agentOrb`         | 56pt circle, T4 with agent gradient + breathing ring, `.interactive()`                     | Floating agent invocation          |
| `playerOrb`        | 64pt circle, T3 with `accentPlayer` tint, play/pause filled glyph — **only place copper appears as a button** | Now-playing primary control |
| `chip`             | T1 clear or T2 tinted, `caption.small` weight, 6h / 10v / pill corner                      | Filters, suggestions, transcript hits |
| `destructive`      | Solid `error` fill, white label — **never** glass                                          | Delete, unsubscribe                |

### 6.2 Cards

| Card                  | Tier | Corner | Padding   | Content                                              |
|-----------------------|------|--------|-----------|------------------------------------------------------|
| `episode.card`        | T0   | `lg`   | 16        | Artwork 56pt + headline + subhead + meta row + chevron |
| `clip.card`           | T2 player tint | `lg` | 16  | Mini-waveform + start/end timestamps + caption + share|
| `wiki.citation`       | T0 + wiki hairline | `md` | 12 | Leaf glyph + 2-line excerpt + episode + timestamp link|
| `speaker.chip`        | T1   | `pill` | 6h/10v   | Avatar 20pt + name + speaker color dot               |
| `agent.message`       | T2 agent tint | `bubble` | 14 | Orb 28pt + body + tool-call ribbons (if any)         |
| `friend.message`      | T2 friend tint | `bubble` | 14 | Friend avatar + body + nostr zap glyph              |

### 6.3 Surfaces

| Surface          | Tier | Corner          | Notes                                                    |
|------------------|------|-----------------|----------------------------------------------------------|
| Mini player      | T2 player | top-only `lg` | Always-on dock; refracts content scrolling beneath      |
| Now Playing full | T4   | `xl` top corners| Hero artwork blurs into ambient backdrop; controls T3 over it |
| Sheet (medium)   | T1   | top `xl`        | Detents 0.5/large; drag handle 36×4                      |
| Modal (full)     | T0 + T1 toolbar | top `xl` | Reading content gets paper, chrome gets clear glass     |
| Voice mode       | T4   | full-bleed      | Background = animated agent gradient + orb              |
| Toast            | T2 contextual tint | `pill` | Top-anchored, 8s auto-dismiss, swipe-up to clear   |
| Banner           | T1   | `md`            | Inline within scroll, never overlay                      |
| Ribbon (tool)    | T1   | `pill`          | Below agent message — "🔧 search_episodes(…)"           |

---

## 7. Motion Language

**Philosophy: motion communicates causality.** Every animation answers *who did this and where did it come from?* No motion is decorative.

### 7.1 Curves (preferred eases)

| Curve               | Spec                                          | Use                                              |
|---------------------|-----------------------------------------------|--------------------------------------------------|
| `motion.snappy`     | `spring(duration: 0.22, bounce: 0.12)`        | Press feedback, chip toggles, scrubber ticks     |
| `motion.standard`   | `spring(duration: 0.35, bounce: 0.15)`        | Default — sheet open, card expand, glass morph   |
| `motion.considered` | `spring(duration: 0.55, bounce: 0.10)`        | Now-playing transitions, agent surface entrance  |
| `motion.cinematic`  | `spring(duration: 0.85, bounce: 0.05)`        | Full-screen player open, voice-mode entrance     |
| `motion.bouncy`     | `spring(duration: 0.45, bounce: 0.32)`        | Celebratory only (briefing complete, save)       |
| `motion.linear`     | `linear(duration: continuous)`                | Scrubbers, progress bars, waveform draw          |

### 7.2 Durations (semantic)

- **instant** ≤ 100ms — selection state, chip press
- **snappy** 200–250ms — toggles, swipes, dismissals
- **considered** 350–550ms — sheet/card transitions, content reveals
- **cinematic** 700–900ms — full-screen takeovers, voice-mode entrance, briefing intro
- **ambient** 4–8s loops — breathing orb, idle waveform shimmer

### 7.3 Choreography rules

1. **Stagger, don't simultaneous.** When multiple elements animate together, offset by 40–60ms each. Up to 5 elements; beyond that, fade the group.
2. **Out before in.** Outgoing elements finish 80% of their exit before incoming starts.
3. **Hero anchors.** In transitions between surfaces sharing an element (artwork from card → full player), use `matchedGeometryEffect` + `glassEffectID`. The hero element morphs; everything else cross-fades.
4. **Parallax**: artwork on Now Playing scrolls at 0.6× the content; transcript drifts at 1.0×. Maximum parallax delta 24pt.
5. **Scrubbing is linear** — never spring. Springs feel laggy on continuous user input.
6. **Glass merges only inside containers.** Outside `GlassEffectContainer`, glass elements never morph — they cross-fade.

### 7.4 Signature transitions

- **Card → Now Playing**: artwork hero-morphs (cinematic), background paper fades to ambient gradient (considered), controls fly up from bottom (snappy, staggered 50ms).
- **Agent invocation**: orb expands from FAB into chat surface, glass IDs unite, background dims to 40% (considered).
- **Voice mode**: orb scales to 240pt, page content blurs to T4 ambient backdrop, mic glyph fades in (cinematic).
- **Briefing intro**: 3-2-1 ticker (snappy), title types in word-by-word (60ms per word), waveform draws from left (linear, 800ms).

---

## 8. Haptic + Sound Vocabulary

Haptics already implemented in `Haptics.swift`. **Extensions required** (add to that file):

### 8.1 New haptic patterns to add

| New pattern        | Composition                                    | When                                       |
|--------------------|------------------------------------------------|--------------------------------------------|
| `playStart`        | medium (0.7)                                   | Tap play, episode begins                   |
| `playPause`        | soft (0.5)                                     | Pause                                      |
| `scrubTick`        | selection (gated to ≥150ms intervals)          | Each 10s/chapter boundary while scrubbing  |
| `agentListenStart` | soft (0.4) + 80ms + soft (0.4)                 | Voice mode opens                           |
| `agentSpeakStart`  | light (0.6)                                    | Agent TTS begins                           |
| `agentInterrupt`   | rigid (0.85)                                   | User barges in over TTS                    |
| `bargeAccepted`    | light (0.5) + 60ms + soft (0.4)                | Agent stops, listening                     |
| `clipMarked`       | medium (0.8) + 100ms + light (0.5)             | Clip start/end mark set                    |
| `friendIncoming`   | light (0.5) + 200ms + light (0.5) + 200ms + light (0.5) | Friend-agent message arrives    |
| `briefingStart`    | success                                        | Briefing audio begins                      |
| `briefingComplete` | bulkAction                                     | Briefing finished                          |

### 8.2 Sound cues

All cues are short (≤450ms), -18 LUFS, ducked under any active audio. Each has a "subtle" 50% gain variant for the in-podcast experience (don't bash through the show).

| Cue                  | Character                               | Length | Trigger                              |
|----------------------|-----------------------------------------|--------|--------------------------------------|
| `agent.listen.up`    | Soft inhale, two-tone rising (G→D)      | 280ms  | Voice mode opens, mic hot            |
| `agent.thinking.loop`| Single sub-bass pulse, 0.8Hz, looped    | loop   | Agent processing > 600ms             |
| `agent.speak.in`     | Warm fade-in chime, single note (D5)    | 220ms  | TTS begins                           |
| `agent.barge`        | Brief reverse-swell, descending         | 180ms  | User barges in                       |
| `transcribe.done`    | Two-note arpeggio (A4→E5)               | 320ms  | Transcript ready                     |
| `briefing.intro`     | Editorial signature — 4-note ascending  | 1.4s   | Briefing playback begins             |
| `briefing.outro`     | Mirror of intro, descending             | 1.2s   | Briefing finishes                    |
| `clip.snap`          | Short tape-stop / shutter hybrid         | 90ms   | Clip mark in/out                     |
| `friend.knock`       | Two soft taps, warm                      | 240ms  | Friend message arrives               |
| `error.glass`        | Soft glass tap, dampened                 | 140ms  | Failed action                        |

**Rule**: never play a sound *and* fire a haptic for the same event unless explicitly listed; the body double-counts.

---

## 9. Agent / Now-Playing / Nostr Visual Signals

These three signals must be **distinguishable in 200ms peripheral vision**. They never share the same hue family; they never share the same shape.

### 9.1 Agent identity — **the Orb**

- Form: 56pt soft sphere (FAB), 240pt sphere (voice mode hero).
- Material: T4 cinematic glass, layered with the `accent.agent` indigo→azure gradient, `.interactive()`.
- Behavior:
  - **Idle**: 4-second breathing scale 0.97↔1.03, gradient slowly rotates 360° / 30s.
  - **Listening**: outer ring pulses at user's voice amplitude (FFT-driven), `accent.live` ring at 60% saturation.
  - **Thinking**: gradient accelerates rotation to 360° / 4s, slight inner shimmer.
  - **Speaking**: ring waveform synced to TTS amplitude, `accent.agent` outer halo.
- Never use the agent gradient on anything that is not the agent.

### 9.2 Now-playing identity — **the Copper Bar**

- Form: a 4pt copper progress line running along the *top edge* of the mini-player surface (the line is a sub-element of the mini-player, not a separate dock). Beneath it: artwork thumb, title, transport.
- Material: mini-player surface is T2 with `glass.player` tint; the line itself is solid `accentPlayer`.
- Behavior: line continuously fills left-to-right at playback rate. Tapping the surface morphs (matched geometry) into the full player.
- **Copper is exclusive**: `accentPlayer` appears only on (a) this line, (b) the full Now Playing player chrome, (c) the `playerOrb` button, and (d) the home-screen "now playing" mini-thumbnail badge. Nothing else.

### 9.3 Nostr / friend-agent signal — **the Amber Seam**

- Form: a 2pt amber hairline that runs along the leading edge of any element initiated by a friend (incoming message bubble, "Maya sent you a clip" toast, friend-agent suggested action chip).
- Material: solid `accent.friend`, glows at 8pt blur radius for 400ms on appear, then settles to 2pt static seam.
- Avatar treatment: 24pt circle with a 1pt amber ring; never the standard hairline.
- **Critical**: amber seam appears *only* when origin is a Nostr event (real friend or friend's agent). Never used for system suggestions.

The three signals are *mutually exclusive per element* — a card cannot be both "from a friend" and "agent-generated." If the agent forwards a friend's message, the bubble is friend (amber), with a small agent orb badge.

---

## 10. ASCII Component Sheet

```
┌─────────────────────────────────────────────────────────────┐
│  EPISODE CARD (T0)                                          │
│  ┌──────┐  Tim Ferriss · #712                               │
│  │ ART  │  How keto rewires metabolism            ›        │
│  │ 56pt │  2h 14m · Yesterday                               │
│  └──────┘  ─── 38% ──────────                               │
└─────────────────────────────────────────────────────────────┘

┌──────────────────────────────────┐
│  AGENT MESSAGE (T2 indigo·azure) │
│  ◉  I found the keto segment in  │
│     yesterday's Tim Ferriss at   │
│     00:42:18. Want me to play?   │
│  ┌────────────────────┐          │
│  │ 🔧 search_episodes │ ribbon   │
│  └────────────────────┘          │
└──────────────────────────────────┘

┌──────────────────────────────────┐
│║ FRIEND MESSAGE (T2 amber, seam) │   ← 2pt amber hairline left
│║ ◐  Maya · just now              │
│║                                  │
│║  Listen to 14:02, she mentions   │
│║  exactly what you asked about.   │
│║  ⚡ via Nostr                    │
└──────────────────────────────────┘

  CHIP RAIL (T1 clear)
  ╭──────╮ ╭──────────╮ ╭────────╮ ╭──────╮
  │ All  │ │ This week│ │ Saved  │ │ +Tag │
  ╰──────╯ ╰──────────╯ ╰────────╯ ╰──────╯

┌─────────────────────────────────────────────────────────────┐
│  NOW PLAYING MINI (T2 copper)              ▬▬▬ 38% ▬▬▬     │
│  ┌──┐ How keto rewires metabolism                  ⏸  ⏭   │
│  │● │ Tim Ferriss · 00:42 / 2h 14m                         │
│  └──┘                                                       │
└─────────────────────────────────────────────────────────────┘
                        ╭──────╮
   AGENT ORB (T4) →     │  ◉   │  ← breathing, indigo→azure
                        ╰──────╯

  BUTTON STYLES
  ╭───────────────╮  ╭───────────────╮  ╭─────────────╮  ╭─────────────╮
  │   Subscribe   │  │ Ask the agent │  │    Share    │  │   Delete    │
  │ (glassProm.)  │  │ (glassAgent▼) │  │  (.glass)   │  │ (no glass)  │
  │  onyx fill    │  │ indigo→azure  │  │  T1 clear   │  │  red solid  │
  ╰───────────────╯  ╰───────────────╯  ╰─────────────╯  ╰─────────────╯
  copper appears ONLY on now-playing surfaces — never on a CTA outside the player.
```

---

## 11. Implementation Notes for `AppTheme.swift` Extension

> **Token-name translation**: dot-paths in this brief (`accent.player`, `display.hero`, `motion.snappy`, `glass.clear`) are *conceptual addresses*, not Swift identifiers. The existing codebase uses flat camelCase (`largeTitle`, `springFast`, `agentSurface`). Translate accordingly: `accent.player` → `Identity.player`, `display.hero` → `Typography.displayHero`, `motion.snappy` → `Animation.snappy`, `glass.clear` → `GlassTint.clear`. Do not ship dot-paths in Swift source.

Specific Swift token names to add. Split across new files when crossing the 300-line soft limit.

**`AppTheme+Colors.swift` — additions**

```swift
extension AppTheme {
    enum Identity {
        static let player        = Color("AccentPlayer")        // copper — RESERVED for now-playing
        static let agentStart    = Color("AccentAgentStart")    // indigo
        static let agentEnd      = Color("AccentAgentEnd")      // azure
        static let agent         = agentStart                   // single-color shorthand
        static let wiki          = Color("AccentWiki")          // moss
        static let friend        = Color("AccentFriend")        // amber
        static let live          = Color("AccentLive")          // signal red
        static let agentGradient = LinearGradient(
            colors: [agentStart, agentEnd],
            startPoint: .topLeading, endPoint: .bottomTrailing
        )
    }
    enum GlassTint {
        static let clear  = Color.clear
        static let player = Identity.player.opacity(0.10)
        static let agent  = Identity.agentStart.opacity(0.10)
        static let friend = Identity.friend.opacity(0.10)
    }
}
```

**`AppTheme+Typography.swift` — additions**

```swift
static let displayHero  = Font.custom("NewYork-Semibold", size: 34, relativeTo: .largeTitle)
static let displayLarge = Font.custom("NewYork-Medium",   size: 28, relativeTo: .title)
static let titleLg      = Font.system(.title2, design: .default, weight: .bold)
static let monoTimestamp = Font.system(.footnote, design: .monospaced).weight(.medium)
```

**`AppTheme+Animation.swift` — additions**

```swift
static let snappy     = SwiftUI.Animation.spring(duration: 0.22, bounce: 0.12)
static let considered = SwiftUI.Animation.spring(duration: 0.55, bounce: 0.10)
static let cinematic  = SwiftUI.Animation.spring(duration: 0.85, bounce: 0.05)
```

**New file: `AppTheme+Glass.swift`**

```swift
extension AppTheme {
    enum Glass {
        enum Tier { case clear, player, agent, friend }
    }
}
extension View {
    func glassTier(_ tier: AppTheme.Glass.Tier,
                   corner: CGFloat = AppTheme.Corner.lg,
                   interactive: Bool = false) -> some View { /* dispatch */ }
}
```

**New file: `Sounds.swift`** — mirrors `Haptics.swift` ergonomics:

```swift
@MainActor enum Sounds {
    static func agentListenUp() { play("agent_listen_up") }
    static func agentThinkingStart() / agentThinkingStop()
    static func transcribeDone() / briefingIntro() / clipSnap() / friendKnock()
}
```

**Extensions to `Haptics.swift`** — add the patterns from §8.1: `playStart`, `playPause`, `scrubTick`, `agentListenStart`, `agentSpeakStart`, `agentInterrupt`, `bargeAccepted`, `clipMarked`, `friendIncoming`, `briefingStart`, `briefingComplete`. Keep existing API; append, don't restructure.

**`AgentOrb.swift`** (new shared component) — single source of truth for the orb across FAB, voice mode, and agent message badges.

---

**File path**: `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-15-liquid-glass-system.md`
