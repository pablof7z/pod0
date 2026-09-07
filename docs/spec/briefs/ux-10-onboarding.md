# UX-10 — Onboarding & First Run

## 1. Vision

The first five minutes must feel like the app was *waiting for this user specifically*. Not a tutorial. Not a setup wizard. A short, cinematic invitation that ends with the user hearing a TLDR briefing **built from their own podcasts**, narrated in a chosen voice, interruptible by speech. By the time the briefing fades, the user has already used four of the app's deepest features — import, identity, agent, voice — without ever feeling configured.

Onboarding's job is not to explain the app. It's to demonstrate that the app already understands them.

## 2. The Discriminating Decision (resolve before build)

The vision says *briefing within 3 minutes of install*. The spec says *OpenRouter BYOK, no default key*. These cannot both be true at install time — OpenRouter signup + key paste is realistically a 3–8 minute side quest with significant drop-off. We must pick:

- **A. Trial budget (recommended).** First briefing runs on a small house-funded LLM+TTS budget (~$0.05/user). BYOK is gently introduced *after* the user is hooked, gated at the second substantial agent action.
- **B. Text-first briefing.** First "briefing" is a written digest from RSS metadata only. BYOK unlocks audio. Honest, ships fast, less magical.
- **C. BYOK before the magic.** Death spiral. Do not.

This brief assumes **A**. If product disagrees, the wireframes for Steps 4 and 6 swap order and the "magical moment" moves later.

## 3. Key User Moments

1. **The Hello.** A single editorial sentence over a moving gradient — *"Talk to all your podcasts."* No logo splash, no carousel.
2. **The Detection.** OPML imports and 47 shows fan out as the user watches. The app *already knows them.*
3. **The Quiet Promise.** Identity is generated invisibly. One line: *"This stays on your device."* No keys shown unless asked.
4. **The First Word.** The agent speaks first — by name, about the user's actual podcasts. *"Pablo, I read everything from this week. Want the 4-minute version?"*
5. **The Interrupt.** Mid-briefing, a subtitle pulses *"tap to interrupt — or just talk."* The user does. The agent answers. The briefing resumes. This is the moment the app stops being a podcast player.

## 4. Information Architecture (state machine)

```
[Launch]
   ↓
[S1 Welcome] ── required
   ↓
[S2 Import]  ── required (OPML / clipboard / skip-to-empty)
   │   └─ branch: empty → suggested-shows mini-picker
   ↓
[S3 Identity] ── required, auto, ~3s
   ↓
[S4 Agent]   ── required-but-trial (trial budget on; BYOK deferred)
   ↓
[S5 Voice persona] ── optional, default = "Aria"
   ↓
[S6 First Briefing] ── magical moment; mic permission requested in-context here
   │   └─ on deny: graceful text-only mode
   ↓
[S7 All Set] ── 1 screen, dismissible
   ↓
[Home / Now Playing]
```

**Required:** S1, S2, S3, S6.
**Optional in-flow:** S4 BYOK upgrade, S5 voice persona.

Power-user escape: a low-contrast *"I know what I'm doing"* link on S1 jumps to a 30-second condensed flow (paste OPML, paste OpenRouter key, done).

## 5. Visual Treatment

Onboarding has its own pacing — slower than the app proper, more cinematic. Editorial typography (NY Times Magazine register), generous negative space, hero text in the 48–64pt range. Backgrounds are slow-drifting mesh gradients that shift hue per step (deep indigo → warm amber → pearl) — mirroring the audio register from intro to invitation to arrival.

Liquid Glass is **restrained** here. The CTA is a single `.glassProminent` capsule, anchored bottom-center, that morphs between steps via `glassEffectID` in a shared `@Namespace`. No cards, no toolbars — onboarding is *content-first*, glass-second. Step transitions cross-fade with a 0.5s ease-out and a subtle parallax on the hero text. No carousel dots. No progress bar — progress is implicit in the rhythm.

Typography: SF Pro Display for hero lines, New York for body callouts ("This stays on your device"). The agent's first line appears in italicized New York with a typing-on cadence, then is read aloud — visual and sonic arrive together.

## 6. Microinteractions

- **OPML detection.** Show titles materialize as small ovals that drift into a soft cluster, count animating *0 → 47*. Each oval is a real artwork crop, not a placeholder. Total: 1.4s. Haptic light-impact on first detection, soft success on completion.
- **Key generation.** A single thin line draws itself across the screen, breaks into a constellation of dots, settles. No "generating..." spinner. 800ms. The text *"Identity created"* fades in beneath. A muted "Reveal key" link sits below for the curious — never the focal point.
- **First agent line.** Text streams in glyph-by-glyph at ~40 chars/sec, *while the TTS voice speaks the same words*. Sync, don't race. When the line ends, the CTA morphs from *"Continue"* to *"Play briefing →"*.
- **Briefing intro.** A horizontal timeline draws itself: dots for each show included. Tapping a dot shows the show name. The play button breathes (subtle scale 0.98 ↔ 1.00, 2s cycle) until tapped.
- **Barge-in indicator.** While briefing plays, a thin glass pill at the bottom reads *"Tap to interrupt"* with a quiet waveform behind. Pressing it OR speaking triggers a soft duck of the briefing audio + a haptic.

## 7. ASCII Wireframes

### S1 — Welcome
```
┌─────────────────────────────┐
│                             │
│                             │
│   Talk to all your          │
│   podcasts.                 │
│                             │
│   One conversation. Every   │
│   show you've ever loved.   │
│                             │
│                             │
│      ╭──────────────╮       │
│      │   Begin  →   │       │
│      ╰──────────────╯       │
│                             │
│   I know what I'm doing →   │
└─────────────────────────────┘
   bg: indigo→violet drift
```

### S2 — Import + Detection
```
┌─────────────────────────────┐
│   Let's find your shows.    │
│                             │
│      ◉  ◉  ◉   ◉            │
│   ◉  ◉  ◉  ◉  ◉  ◉          │
│      ◉  ◉  ◉   ◉            │
│         47 shows            │
│                             │
│  Imported from Overcast.    │
│  Tim Ferriss, Lex Fridman,  │
│  Acquired, +44 more.        │
│                             │
│   ╭─────────────────╮       │
│   │   Looks right → │       │
│   ╰─────────────────╯       │
│   Add another source        │
└─────────────────────────────┘
```

### S3 — Identity
```
┌─────────────────────────────┐
│                             │
│      · · · · · · · ·        │
│       (constellation)       │
│                             │
│   Identity created.         │
│                             │
│   We made you a private     │
│   identity. It lives on     │
│   this device. We never     │
│   see it.                   │
│                             │
│   Reveal key (advanced)     │
│                             │
│      ╭──────────────╮       │
│      │  Continue →  │       │
│      ╰──────────────╯       │
└─────────────────────────────┘
```

### S4 — Agent (trial-on, BYOK soft)
```
┌─────────────────────────────┐
│   Meet your agent.          │
│                             │
│   It's read every episode   │
│   you subscribe to. Ask it  │
│   anything.                 │
│                             │
│   ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─     │
│   You're on the house for   │
│   your first week. After,   │
│   bring your own key        │
│   (OpenRouter — we'll show  │
│   you how).                 │
│                             │
│   ╭──────────────────╮      │
│   │  Hear it speak → │      │
│   ╰──────────────────╯      │
│   I have a key already      │
└─────────────────────────────┘
```

### S5 — Voice persona (optional)
```
┌─────────────────────────────┐
│   Pick a voice.             │
│                             │
│   ╭───╮  ╭───╮  ╭───╮       │
│   │ ▶ │  │ ▶ │  │ ▶ │       │
│   ╰───╯  ╰───╯  ╰───╯       │
│   Aria   Kai    Sage        │
│   warm   crisp  measured    │
│                             │
│   (tap to preview)          │
│                             │
│      ╭──────────────╮       │
│      │   Use Aria → │       │
│      ╰──────────────╯       │
│   Skip — surprise me        │
└─────────────────────────────┘
```

### S6 — First Briefing (mic permission folded in)
```
┌─────────────────────────────┐
│   Your week, in 4 minutes.  │
│                             │
│   ●─●─●─●─●─●─●─●           │
│   8 episodes from this week │
│                             │
│        ╭──────────╮         │
│        │    ▶     │         │
│        ╰──────────╯         │
│                             │
│   Tap to interrupt — or     │
│   just speak. (We'll ask    │
│   for the mic when you do.) │
│                             │
│   Skip for now              │
└─────────────────────────────┘
```

### S7 — All Set
```
┌─────────────────────────────┐
│                             │
│   You're set.               │
│                             │
│   • 47 shows imported       │
│   • Agent active (trial)    │
│   • Daily briefing: 7:30am  │
│     (change anytime)        │
│                             │
│      ╭──────────────╮       │
│      │   Open app → │       │
│      ╰──────────────╯       │
│                             │
└─────────────────────────────┘
```

## 8. Edge Cases

- **No podcasts to import.** S2 shows a curated 12-show starter pack ("If you like X, try Y") drawn from a static editorial list. User picks 3+ to proceed. Empty-state never blocks.
- **No internet.** S2 fails gracefully — *"We'll find them when you're back online"* — and onboarding routes to a stripped first run that ends at S3 (identity), with S6 deferred. A soft notification fires on next connection.
- **OPML import fails (malformed file).** Silent retry once; on second fail, fall back to clipboard URL paste with one-line instruction. Never show a parser error.
- **Mic permission denied at S6.** Briefing plays through normally. The *"Tap to interrupt"* pill becomes *"Tap to ask"* and opens text input. A single line: *"Want voice later? Settings → Voice."* No nag.
- **BYOK declined / trial expired.** App enters **Quiet Mode** — playback, transcripts, and library work fully; agent and briefings are read-only summaries from cached metadata. A persistent but unobtrusive banner offers the BYOK walkthrough. No feature is hidden, only *agent intelligence* is paused.
- **"Auto-detect from Apple Podcasts" is platform-impossible.** iOS provides no subscriptions API. Be honest in copy: list Overcast / Pocket Casts / Castro as supported (OPML export), and offer Apple Podcasts users a clipboard-paste flow ("copy a show link, we'll add it"). Do not promise what the OS forbids.

## 9. Accessibility

Onboarding must be fully usable under VoiceOver, with the screen off, one-handed.

- Every step is reachable by swipe; CTAs are the rotor's first stop.
- Hero text is set as `accessibilityLabel` with a longer, conversational read ("Welcome. This app lets you talk to all your podcasts.").
- The detection animation announces *"47 shows imported from Overcast"* once, not per oval.
- The constellation animation is `accessibilityHidden`; the line *"Identity created on device"* carries the meaning.
- The briefing screen is fully operable with mic denied — text input is keyboard-accessible, Dynamic Type up to AX5 reflows hero typography down to 24pt minimum.
- Reduce Motion: gradients become solid tints, parallax disabled, glass morphs become cross-fades.
- Contrast: hero text ≥ 7:1 against gradient (verified at hue extremes), CTA ≥ 4.5:1.

## 10. Open Questions / Risks

- **Trial budget economics.** Need finance sign-off on per-user ceiling and abuse vectors (multi-install farming). Recommend: device-attested + capped at one briefing + ~2K agent tokens.
- **OpenRouter onboarding handoff.** When a trial user opts into BYOK, do we deep-link to OpenRouter signup with our referral, or open in-app web view? In-app keeps the thread; deep-link improves trust.
- **Voice persona at first run.** Risk: choosing a voice cold (no context) feels arbitrary. Mitigation: make it skippable, default to Aria, allow change after first briefing when the voice is *embodied*.
- **The 3-minute target.** Honest measurement needed. With trial budget on and OPML detection cached, S1→S6 play-start is reachable in ~90 seconds. Briefing generation (LLM + TTS) is the long pole — must stream first audio chunk within 6 seconds or the magic dies.

---

**File:** `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-10-onboarding.md`
