# UX-14 — Proactive Agent & Notifications

> Owner: Designer (Aditi). Coordinates with #1 Now Playing, #2 Library, #5 Agent Chat, #8 Briefings, #9 Threading, #10 Onboarding, #12 Nostr, #15 Liquid Glass.
> Inherits from: `App/Sources/Features/Agent/`, `Design/GlassSurface.swift`, `UNUserNotificationCenter`, ActivityKit, `BGAppRefreshTask`.

---

## 1. Vision

Most apps treat notifications as a megaphone. We treat them as a **front page** — one daily edition, edited by the agent, delivered with the calm of a paper landing on the porch.

1. **One push a day, by default.** Everything else pools in **Today**, the in-app digest. Spam is a design failure, not a settings problem.
2. **Today is editorial, not a feed.** Magazine cover: one hero, three to five cards, generous whitespace. *Finishable* in under a minute.
3. **Confidence is visible.** Solid rule for grounded; dashed rule and italic eyebrow for inferential. Trust is legibility, not enthusiasm.

After a week away, the user comes back to seven quiet editions and feels seen.

---

## 2. Key User Moments

1. **Morning porch-paper.** 7:30 AM. One push: *"Your Tuesday briefing — 9 minutes, 4 shows."* Tap → #8 player. The only push she'll get all day, and she knows it.
2. **Favorite show drop.** Acquired publishes at 2 PM — no push. At 6 PM she opens the app; **Today** has a hero card: *"Acquired dropped a new one. The agent listened; here's a 30-sec narrated preview."*
3. **Cross-podcast convergence.** Three shows mention semaglutide. *Threads* card: *"Ozempic came up across 3 podcasts you follow."* Tap → #9.
4. **Echo from a question past.** Last month she asked about Karpathy. He's on Lex this week. *Echo* card, dashed rule, italic eyebrow — inferential.
5. **Friend clip.** Maya's agent shares a 42-sec clip via Nostr (#12). Lands in **Inbox** under *From Friends*. Never pushed unless Maya is Priority.
6. **Transcript ready.** A 3-hour episode finishes transcribing. Small glass dot on the Library tile + a passive Today card. **Never a push** — we resist the temptation.

---

## 3. Information Architecture

```
Proactive root
├── Today  (cover · hero · 3–5 stack cards · "That's today.")
├── Inbox  (Pinned · This week by day · From Friends · Archive)
├── Notification Center surface
│   ├── Briefing push (if enabled)
│   ├── Live Activity for active briefing
│   └── Critical-only: scheduling failure, friend priority clip
└── Settings → Notifications
    ├── Daily Briefing  (time, weekday/weekend split, skip if quiet)
    ├── Smart Push Budget  (default 1/day; max 3; off)
    ├── Per-Type Toggles   (Drops · Threads · Echoes · Friends · Transcripts)
    ├── Quiet Hours        (system Focus mirror + custom override)
    └── Vacation Mode      (suspend N days; archive accumulates)
```

**Insight Card taxonomy** (used everywhere — Today, Inbox, push):

| Type | Eyebrow | Confidence | Default Push |
|---|---|---|---|
| Briefing | `MORNING EDITION` | solid rule | yes (1/day) |
| Drop | `NEW · <show>` | solid rule | no |
| Thread | `THIS WEEK · CROSS-EPISODE` | solid rule | no |
| Echo | `YOU ASKED · <date>` | dashed, italic | no |
| Friend | `FROM <name> ·` Nostr glyph | solid rule | priority-only |
| Transcript | `READY · TRANSCRIPT` | solid rule | never |

---

## 4. Visual Treatment

**Today is a magazine cover.** Generous date treatment, a small "Edition 142" tag in mono small caps, one-line read-time. No badges. No counters. The page *breathes.*

**Cards as Liquid Glass tiles.** Same `GlassEffectContainer` + `cornerRadius: 22` shell as chat embeds (#5), with deliberately more vertical padding (22pt top, 18pt bottom) — paced reading, not dense feed.

- **Hero card**: full-width, 220pt min, editorial serif, inline play affordance as a glass capsule tinted in show-accent at 22%.
- **Stack cards**: 124pt min. Eyebrow + headline + dek. Glyph cluster (play, save, dismiss) in muted glass; animates in only on long-press.
- **Echo cards** wear a **dashed leading rule** (1pt, 60% glass) and italic eyebrow — inferential should *feel* inferential.
- **Friend cards** add a small Nostr relay glyph (matches #5) and 36pt avatar; subtle tint toward friend's accent.

**Color & light.** System Liquid Glass canvas with a *time-of-day vertical gradient* at 6% opacity — predawn cool blue → midday neutral → evening warm amber. The only chrome that signals "morning paper." Show-art tints stay inside Drop cards.

**Typography.** Editorial serif (New York) for cover and hero (32/36, 22/26). Stack headlines 19/24 serif. Eyebrows SF Mono Small Caps 11/14 at 80%. Deks 15/22 serif at 75%. Counts in SF Mono. Dynamic Type to AX5 — single column at AX3+; eyebrow stacks above headline at AX4+.

**Motion.** Cards enter with a 60ms-staggered 240ms ease-out spring, 8pt rise — *a deck of cards being laid down*. Dismiss shrinks to 90% and fades (180ms). Pin pulls toward leading edge, settles with `.success` haptic.

**Push visual.** System-rendered, but we craft structure: title = eyebrow, body = headline, subtitle = dek with read-time. Long-press reveals inline play (Notification Service Extension + audio attachment).

---

## 5. Microinteractions

- **Swipe right** → *Pin* (anchors to Inbox top). `.success` haptic, leading pin glyph appears.
- **Swipe left** → *Dismiss*. `.soft` haptic; two-second undo toast in muted glass.
- **Long-press** → *Tell me more* sheet. Agent explains why it surfaced, which tools fired (mirrors #5's Tool-Call Inspector); chips: *Open thread · Save to wiki · Mute this kind 1 week · This wasn't useful*.
- **Two-finger tap on Today header** → cycle confidence filter (All · Grounded · Inferential).
- **Pull down on Today** → reveals *yesterday's edition* underneath, like flipping back a page. Capped at 7 days.
- **Pull up at footer** → "That's today." morphs into a glass button: *Show me Inbox*. Encourages closure.
- **Push action buttons**: *Listen now* (`.glassProminent`), *Snooze 1h*, *Skip today*. Sleep Focus → push silently rolls into Today.
- **Live Activity**: briefing-only. Dynamic Island compact view = scrubber + chapter. Non-briefing items never use Live Activities.

---

## 6. ASCII Wireframes

### 6.1 Today — full edition

```
╭─────────────────────────────────────╮
│  TUE · MAY 9 · EDITION 142          │
│                                     │
│  Today                              │  ← serif, 32pt
│  9 minutes to catch up              │
│                                     │
│  ┌───────────────────────────────┐ │
│  │ MORNING EDITION               │ │
│  │ Tuesday's briefing —          │ │  ← hero serif headline
│  │ 9 minutes, 4 shows            │ │
│  │ Featuring the new Acquired,   │ │
│  │ a Huberman recap, a Karpathy  │ │
│  │ thread.                       │ │
│  │ [ ▶  Listen now    9:04 ]    │ │
│  └───────────────────────────────┘ │
│                                     │
│  ┌───────────────────────────────┐ │
│  │ NEW · ACQUIRED          1h    │ │
│  │ Ben & David on Costco         │ │
│  │ 30-sec narrated preview ▶     │ │
│  └───────────────────────────────┘ │
│                                     │
│  ┌───────────────────────────────┐ │
│  │ THIS WEEK · CROSS-EPISODE     │ │
│  │ Ozempic across 3 shows        │ │
│  │ Huberman, Attia, Diamandis    │ │
│  │                     Open  ›   │ │
│  └───────────────────────────────┘ │
│                                     │
│  ┌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┐ │  ← dashed = inferential
│  ╎ YOU ASKED · APR 14           ╎ │
│  ╎ Karpathy is on Lex this week ╎ │
│  └╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘ │
│                                     │
│  That's today.   Inbox › Settings   │
╰─────────────────────────────────────╯
```

### 6.2 Inbox

```
╭─────────────────────────────────────╮
│ ◀  Inbox                  Filter ⌄ │
├─────────────────────────────────────┤
│ 📌 PINNED                           │
│  ┌───────────────────────────────┐ │
│  │ FROM MAYA · NOSTR             │ │
│  │ "the part about Brian Eno"    │ │
│  │ 0:42 clip · Rick Rubin show ▶ │ │
│  └───────────────────────────────┘ │
├─────────────────────────────────────┤
│ TODAY · MAY 9                       │
│  · Drop — Acquired                  │
│  · Thread — Ozempic ×3              │
│  · Echo — Karpathy on Lex           │
├─────────────────────────────────────┤
│ YESTERDAY · MAY 8                   │
│  · Briefing (listened ✓)            │
│  · Drop — Dwarkesh                  │
│  · Transcript ready — Hassabis ep   │
├─────────────────────────────────────┤
│ MAY 7                               │
│  · Friend clip — Sam                │
├─────────────────────────────────────┤
│  Archive ›   Search Inbox ›         │
╰─────────────────────────────────────╯
```

### 6.3 Notification settings

```
╭─────────────────────────────────────╮
│ ◀  Notifications                    │
├─────────────────────────────────────┤
│ DAILY BRIEFING                      │
│   Push the briefing       ●─── on  │
│   Weekday time            7:30 AM > │
│   Weekend time            8:30 AM > │
│   Skip on quiet days      ●─── on  │
├─────────────────────────────────────┤
│ SMART PUSH BUDGET                   │
│   Max pushes per day              1 │
│   ── Surplus pools in Today / Inbox │
├─────────────────────────────────────┤
│ WHAT TO SURFACE                     │
│   Drops (favorite shows)  ●─── on  │
│   Cross-podcast threads   ●─── on  │
│   Echoes (past questions) ●─── on  │
│   Friend clips (Nostr)    Priority>│
│   Transcript ready        Today only│
├─────────────────────────────────────┤
│ QUIET HOURS                         │
│   Mirror system Focus     ●─── on  │
│   Custom 22:00 → 07:30        Edit >│
├─────────────────────────────────────┤
│ VACATION MODE                       │
│   Pause Today & push     ◯── off  │
│   Resume on               May 19  > │
╰─────────────────────────────────────╯
```

### 6.4 "Tell me more" — agent insight expansion

```
╭─────────────────────────────────────╮
│ ✕  Why this surfaced                │
├─────────────────────────────────────┤
│  YOU ASKED · APR 14                 │
│  Karpathy is on Lex this week       │
├─────────────────────────────────────┤
│  You asked about Andrej Karpathy    │
│  on April 14. He's the guest on     │
│  this week's Lex Fridman, ep #471.  │
│                                     │
│  ▾ how the agent decided            │
│    · matched "Karpathy" in chat     │
│      history (last 60 days)         │
│    · matched in new episode meta    │
│    · confidence: 0.78 (medium)      │
│                                     │
│  ┌───────────────────────────────┐ │
│  │ Lex Fridman · #471            │ │
│  │ Karpathy on AGI               │ │
│  │ 2:48:11   [ Open ] [ Play ▶ ] │ │
│  └───────────────────────────────┘ │
│                                     │
│  [ Save to wiki ] [ Mute Echoes 1w ]│
│  [ This wasn't useful ]             │
╰─────────────────────────────────────╯
```

### 6.5 Briefing-arrived push (lock screen)

```
┌─────────────────────────────────────┐
│  🎙  Podcast Player        7:30 AM   │
│                                     │
│  MORNING EDITION                    │
│  9 minutes, 4 shows                 │
│  Featuring the new Acquired and     │
│  a Karpathy thread.                 │
│                                     │
│  [ ▶ Listen ]  [ Snooze 1h ]  [ × ] │
└─────────────────────────────────────┘
   ↳ long-press → inline play affordance
```

### 6.6 Empty Today — quiet day

```
╭─────────────────────────────────────╮
│  WED · MAY 10 · EDITION 143         │
│                                     │
│  Today                              │
│  Quiet day.                         │
│                                     │
│         ╭─────────────────╮         │
│         │   ◯ ◯ ◯         │         │
│         ╰─────────────────╯         │
│                                     │
│  Nothing new from your shows,       │
│  no threads worth surfacing.        │
│                                     │
│  Browse Library ›   Ask the agent › │
╰─────────────────────────────────────╯
```

---

## 7. Edge Cases

- **No signals.** *Quiet day* cover (§6.6). Never invent items. Empty is content.
- **Vacation mode.** Today and pushes pause; Inbox accumulates. On resume, hero is an *Editor's Note*: *"You were away 6 days. 23 items waiting; here are the 3 the agent thinks matter."* Gap-day editions remain via pull-down.
- **System DND / Sleep / Driving Focus.** Briefing routes silently into Today. Live Activity never fires under Sleep. Register a Focus Mode intent so users can build "Podcast Briefing" into morning Focus.
- **All notifications disabled.** Today still works. Single line on settings entry: *"Push is off. Today still updates each morning when you open the app."*
- **Hallucinated insight.** *This wasn't useful* silently downweights the type. Three taps in 30 days mutes for a week with confirmation. Negative signal is first-class ranking input.
- **Briefing fails overnight.** Push never fires with broken content. User opens to: *"Briefing didn't compose this morning — try generating one now?"* One-tap retry.
- **Budget spent on a friend clip.** Briefing demotes to in-app; cover note: *"You already heard from Maya today — briefing waited inside."*
- **CarPlay when push fires.** Tap opens straight to briefing player (#8) in car-mode UI; Today is not opened.

---

## 8. Accessibility

- **VoiceOver on Today.** Cover is one rotor stop ("Tuesday, May 9. Edition 142. 9 minutes. 4 cards below."). Each card is one stop summarizing type, headline, dek, and confidence ("Echo card. Lower confidence. You asked about Karpathy on April 14…"). Pin and dismiss exposed as VoiceOver actions, not gesture-only.
- **Notification sound design.** Three restrained tones — *Briefing* (low two-note chord, 380ms), *Friend Priority* (rounded tone, 220ms), *Critical* (descending three-note, scheduling-failure only). All under 65 dB SPL; haptic-only fallback for each. Never the system default chime.
- **Reduce Motion.** Card stagger becomes a 180ms cross-fade; page-flip pull-down becomes an instant section change with announce.
- **Reduce Transparency.** Glass cards switch to `surfaceElevated`; time-of-day gradient flattens. Confidence rules stay solid/dashed.
- **Dynamic Type.** AX5 verified. AX4+ stacks eyebrow above headline; hero capsule wraps onto its own line.
- **Color independence.** Confidence = rule-style + italic eyebrow, never color alone. Card type = icon + eyebrow text.
- **Hit targets.** All ≥ 44pt. Pin/dismiss also live in the long-press sheet for users who can't or won't swipe.
- **Calmer preset.** Onboarding (#10) offers a *Calmer* preset disabling push and biasing toward fewer, higher-confidence items. The "1 push/day" default is itself an accessibility feature.

---

## 9. Open Questions / Risks

1. **Alert fatigue.** Ship 1/day default; surface a tooltip after day 7 — *"Want more? Raise your push budget in Settings."* Never auto-raise.
2. **Hallucinated Echoes.** Confidence ≥ 0.65 to surface; ≥ 0.80 for solid rule. Track *This wasn't useful* taps as ground truth.
3. **Background scheduling reliability.** `BGAppRefreshTask` is opportunistic. Trigger briefing generation at 4 AM via APNs background push; on-device fallback composes lazily on first open if missed.
4. **Echo privacy.** Push body never reproduces the original question text — user must open Today. Coordinate with #10 on consent.
5. **Friend-clip priority abuse.** Priority allows ≤1 push per friend per 24h. Coordinate with #12.
6. **Empty Today shame.** *"Quiet day"* must read as relief, not failure. Copy-test in onboarding research; never the word "nothing."
7. **Live Activity churn.** Briefings >12 minutes risk system kill — hand off to Now Playing at 80% completion.
8. **Cross-device dedup.** APNs collapse-id keyed to {user, edition_id} so iPad + iPhone never double-push.
9. **Inbox infinite-scroll temptation.** Resist gamifying. Hard 30-day archive window; archive is searchable, not browsable.

---

**File**: `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-14-proactive-agent-notifications.md`
