# UX-11 — Ambient Surfaces

> Where the app meets the user when they aren't holding the phone. Lock Screen, Live Activities, Dynamic Island, Home/Lock widgets, CarPlay, Watch, AirPods, App Intents, Focus filters.
>
> Coordinates with: #6 Voice (renders into CarPlay/Watch/AirPods), #8 Briefings (surfaces in widgets/Live Activities), #14 Proactive Agent (owns notifications; we own the persistent surfaces).

---

## 1. Vision

Ambient surfaces should *do the right thing* without opening the app. Phone, watch, car dashboard, and AirPods are one product — a single agent always one squeeze away. The screen-off experience is not a stripped-down app; it is the app's *resting state*. Calm by default; alive exactly when audio plays, a briefing renders, or the agent is thinking. Three actions are always one gesture away: **play**, **ask**, **brief me**. Bar: a driver starts the morning briefing, interrupts to ask "who was that guest?", gets an answer, and returns — without looking at the phone.

## 2. Key User Moments

1. **Morning commute** — CarPlay launches, agent offers today's 12-min briefing. One tap, audio starts. Mid-briefing voice barge-in is the marquee moment.
2. **Walking with AirPods** — phone in pocket. Long-press squeeze enters voice mode; soft chime, just speak.
3. **Workout, Watch only** — phone at home. Watch plays a queued briefing over Bluetooth, crown scrubs, complication shows what's next.
4. **Lock-screen glance** — Live Activity shows the current transcript line scrolling with chapter context and an ask affordance.
5. **Hands-busy** — Action Button or "Hey Siri" fires an App Intent that returns a spoken answer without unlocking.
6. **At the desk** — medium Home widget surfaces "Today's threads" — three cross-episode topics, deep-linked into #9.
7. **Multi-device handoff** — phone briefing → CarPlay picks up at the same timestamp with a Liquid Glass continuity banner.

## 3. Information Architecture

| Surface | Primary | Secondary |
|---|---|---|
| Live Activity — playing | Episode title + scrolling transcript line | Scrub, chapter, art, ask |
| Live Activity — briefing rendering | "Briefing for Wed, May 9" + progress % | Episode count, cancel |
| Live Activity — agent thinking | "Searching transcripts…" + query echo | Spinner |
| Dynamic Island compact | Art (L), waveform (R) | — |
| Dynamic Island expanded | Title, transcript line, scrub | ±15s, play/pause, ask |
| Home widget S / M / L | S: now playing or "Brief me" · M: + 2 briefings + top thread · L: + 3 threads + ask box | — |
| Lock Screen widget | Rect: "Ask agent" deep-link · Circular: play/pause or briefing-ready badge | — |
| CarPlay Now Playing | Art, title, scrub, transport | Chapters, queue, persistent voice button |
| CarPlay Voice | Big mic, last answer transcript | Suggested prompts |
| CarPlay Library | Subscription grid, briefings shelf | Voice-only search |
| Watch player | Title, crown scrub, transport | Chapter, voice |
| Watch briefing | Briefing + segment list | Skip-segment, ask |
| Watch complication | Next briefing or now-playing waveform | — |

## 4. Visual Treatment

**Lock Screen Live Activity** uses `GlassEffectContainer(spacing: 24)`. Art in a `.rect(cornerRadius: 14)` glass tile; transcript line in 15pt SF Pro Text on `.regular` glass so wallpaper bleeds through. Tint sampled from album art (dominant + complement) so the activity feels native to *this* episode, not generic chrome.

**Dynamic Island** uses `glassEffectID` + a single `@Namespace` so compact ↔ expanded ↔ thinking states morph rather than cut. Thinking state = three orbs unioned via `glassEffectUnion`, pulsing L→R. No chrome; the morph *is* the affordance.

**Home widgets** respect `widgetRenderingMode`. In `.accented`, artwork uses `widgetAccentedRenderingMode(.monochrome)`; briefing waveform stays accent-tinted; prompt copy is primary. Container background: 6% opacity gradient sampled from art in `.fullColor`, thin material in `.accented`.

**CarPlay** is our strictest contrast environment. 22pt min body, 34pt title, Semibold. Flat dark `#0A0A0F` backgrounds — no glass blur (the API doesn't expose Liquid Glass, and driver attention demands flat). Same color system, solid panels.

**Apple Watch** — tonal SF Symbols, full-bleed art on the player, corner complication uses `.accentedRenderingMode(.full)` on supported faces.

## 5. Microinteractions

- **Dynamic Island state machine**: idle → playing → expanded (tap) → thinking (shimmer) → answer-ready (transcript line slides up) → playing. Transitions use `withAnimation(.smooth)` and `glassEffectID` so shapes morph; nothing fades.
- **CarPlay voice button** is pinned bottom-right of every screen. Tap = barge-in even mid-sentence. Pulses only while agent generates — not while listening (driver feedback is auditory).
- **Watch crown** scrubs at two rates by rotation velocity: shallow = ±5s per detent, aggressive = ±30s (chapter-jump). Haptic per detent, double-haptic on chapter boundary.
- **AirPods squeeze**: single = play/pause (system default, do not override); double = skip; triple = previous; **long press = voice mode**. Soft chime plays in the buds when listening — no screen feedback needed.
- **Live Activity tap targets** — only play/pause and "ask" are interactive. Transcript line is non-tappable (would conflict with the unlock gesture).
- **Widget refresh** is push-driven via the playback engine (no timeline polling) so the waveform stays in sync.

## 6. ASCII Wireframes

```
[1] LOCK SCREEN LIVE ACTIVITY — playing
┌──────────────────────────────────────────────┐
│  ░░░░ wallpaper / glass blur ░░░░             │
│  ┌────────────────────────────────────────┐  │
│  │ ╭────╮  Tim Ferriss · #742              │  │
│  │ │ ART│  "…the keto thing was, for me,    │  │
│  │ │    │   really about cognitive…"  ◂───  │  │
│  │ ╰────╯  ──●─────────────────  31:08      │  │
│  │  ⏮  ⏯  ⏭                       [ask ◎]  │  │
│  └────────────────────────────────────────┘  │
└──────────────────────────────────────────────┘

[2] DYNAMIC ISLAND — three states (compact / expanded / thinking)
  compact:    ( ◉   ▁▂▃▂▁ )      art L, waveform R
  expanded:  ╭────────────────────────────╮
             │ ◉  Tim Ferriss · #742       │
             │    "the keto thing was…"    │
             │    ──●──────────  31:08     │
             │    ⏮   ⏯   ⏭        [ask]  │
             ╰────────────────────────────╯
  thinking:   ( ◉  · · ·  )       3 unioned glass orbs pulse L→R

[3] HOME WIDGET — medium (4×2)
┌──────────────────────────────────────────────┐
│  NOW PLAYING                  ▁▂▃▄▃▂▁         │
│  Tim Ferriss · #742 · 31:08                   │
│  ──────────────────────────────────────────   │
│  ◎ Brief me (12 min)        ▶               │
│  ◎ Today's threads:                           │
│      • Ozempic across 4 shows →               │
└──────────────────────────────────────────────┘

[4] CARPLAY — Now Playing
┌──────────────────────────────────────────────┐
│  ◀  Library   Briefings   Voice              │ tabs
│ ┌────────────┐                               │
│ │            │  TIM FERRISS                   │
│ │    ART     │  Episode #742                  │
│ │            │  Chapter: "Keto & cognition"   │
│ └────────────┘                               │
│  ⏮     ⏯     ⏭     ⟲15    ⟳15              │
│  ──────●─────────────────────  31:08 / 1:42   │
│                                       [ 🎤 ]  │ persistent voice
└──────────────────────────────────────────────┘

[5] CARPLAY — Voice mode
┌──────────────────────────────────────────────┐
│  ◀  back to Now Playing                       │
│                                               │
│              ╭──────────╮                     │
│              │   ◉ ◉ ◉   │   listening         │
│              ╰──────────╯                     │
│                                               │
│   "What did Huberman say about magnesium?"    │
│                                               │
│   Suggested:                                  │
│     • Brief me (12 min)                       │
│     • Resume #742                             │
│     • What's new this week?                   │
└──────────────────────────────────────────────┘

[6] APPLE WATCH — player
   ╭──────────────────╮
   │ Tim Ferriss #742 │
   │ ▁▂▃▄▅▄▃▂▁        │
   │  ⏮   ⏯   ⏭       │
   │  ──●──────  31:08 │
   │     [ 🎤  ask  ]  │
   ╰──────────────────╯

[7] APPLE WATCH — briefing
   ╭──────────────────╮
   │ BRIEFING · Wed   │
   │ 12 min · 5 segs  │
   │ ▶ 1. Ozempic ●   │
   │   2. AI agents   │
   │   3. Keto recap  │
   │   4. Markets     │
   │   5. Outro       │
   │  ⟲    ⏯    ⟳→seg │
   ╰──────────────────╯

[8] AIRPODS SQUEEZE AFFORDANCE  (no screen — onboarding card)
┌──────────────────────────────────────────────┐
│  Your AirPods                                  │
│  ─────────────                                 │
│   ●   single squeeze    play / pause           │
│   ●●  double squeeze    skip                   │
│   ●●● triple squeeze    previous               │
│  ▬▬▬  long press        ▶ talk to agent       │
│                                                │
│  When you talk, you'll hear a soft chime —     │
│  then just speak. Squeeze again to interrupt.  │
└──────────────────────────────────────────────┘
```

## 7. Edge Cases

- **No audio** — Live Activity dismisses; Home widget shows "Brief me" CTA with agent avatar; Dynamic Island stays empty (no squatting).
- **Briefing rendering** — dedicated Live Activity with 0–100% progress and a Cancel tap. Stays under the 4KB ActivityKit payload by sending only progress %, never script. Dynamic Island shows the shimmer.
- **Agent thinking** — surfaces in the Dynamic Island only if invoked from a widget or Shortcut. 8s timeout falls back to "still working — open app" deep link.
- **Multi-device handoff** — CarPlay connects mid-playback → one-time Liquid Glass continuity card on phone Lock Screen. Watch flips its complication to "Playing on iPhone".
- **Offline / no transcripts** — Live Activity falls back to chapter title. Never show empty quotes.
- **Briefing + incoming call** — briefing pauses; Live Activity persists in a paused state with Resume CTA.
- **Focus mode** — register a Focus filter so users can hide the agent-prompt widget during "Sleep" while keeping playback controls.

## 8. Accessibility

- **VoiceOver on Live Activity** reads "Tim Ferriss 742, playing, current line …, ask the agent button." The transcript line is `.updatesFrequently` so VO doesn't re-announce per word.
- **CarPlay captions** — optional always-on caption strip above the transport, toggled in Settings → Accessibility, for hearing-impaired or noisy environments.
- **Watch glanceability** — player passes the 1-second test: title, progress, transport visible without scrolling. No text under 14pt.
- **Dynamic Type** — every widget has dedicated `.accessibility1` and `.accessibility5` layouts; medium collapses "today's threads" rather than truncating.
- **Reduce Motion** disables the Dynamic Island shimmer; thinking becomes a static dot row.
- **Contrast** — CarPlay text passes WCAG AAA (≥7:1) on flat dark. Lock Screen transcript falls back to `Color.primary` if the wallpaper sample drops below 4.5:1.
- **AirPods voice chime** — distinct from Siri's so blind users can disambiguate which agent is listening.

## 9. Open Questions / Risks

1. **Live Activity battery cost** — transcript-line scrolling is novel and risky. ActivityKit budgets ~16 high-frequency updates/hr. Mitigation: throttle to one update *per transcript-segment boundary* (~every 6–10s), not per word. Battery soak test required before ship.
2. **Live Activity 8–12h ceiling** — long sessions will hit it. End gracefully, fall back to system Now Playing chrome, surface a "resume" notification.
3. **CarPlay API limits** — no Liquid Glass, no custom fonts, restricted templates. Confirm with #15 that the persistent voice tab fits under `CPTemplateApplicationScene`. Fallback: voice mic in the Now Playing template's bar button.
4. **AirPods long-press claim** — only one app can claim it system-wide. Product decision needed: override Siri for our users, or off-by-default with a Settings opt-in.
5. **Widget freshness vs. battery** — push-driven updates via WidgetCenter must stay under background-budget thresholds; needs measurement.
6. **Privacy on Lock Screen** — transcript lines are visible to anyone holding the phone. Settings: "Hide transcript on Lock Screen" (default ON for episodes the user flags sensitive).
7. **Briefing-rendering copy** — non-technical users shouldn't see "agent generating." Lean toward "Preparing your briefing…".
8. **Watch standalone RAG** — almost certainly not in v1. Watch voice mode degrades to "ask anyway, syncs when phone reachable" with clear UX.

---

**File:** `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-11-ambient-surfaces.md`
