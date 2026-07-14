# UX-01 — Now Playing

> The hero surface. The screen the user will stare at thousands of times. Not a player with a transcript bolted on — a transcript that breathes with the audio, with the artwork as its frame.

## 1. Vision

The Now Playing screen treats the **transcript as the primary surface, not a secondary panel**. As audio plays, the current sentence rises and lights up; speaker color washes the line; the artwork above it dims and softens like a stage cyclorama. Every line is a doorway: tap to jump, hold to clip, double-tap to ask the agent about it. The waveform under the scrubber isn't decorative — it shows the *shape* of the conversation: who's speaking, where the ad break is, where the silence sits. The whole surface is built from layered Liquid Glass over a color-extracted blur of the cover art, so the screen feels lit *by the show itself*. Calm by default. Alive on demand. The agent is one corner of glass away — never in the way, always in reach. This is the surface that makes someone close Spotify and not come back.

## 2. Key User Moments

1. **First glance after pressing play from Library.** The mini-bar rises into a full-screen hero with `matchedGeometryEffect`; cover art settles into place; the first transcript line fades in beneath it. *User feels: this is unmistakably premium.*
2. **A guest says something interesting.** The line lights, user instinctively reaches forward, holds the line — a glass ring fills around it, haptic ramps, releases into a clip-share sheet pre-trimmed to the sentence boundary. *User feels: the app read my mind.*
3. **"What's keto?" mid-listen.** User taps the agent chip in the lower-right glass cluster; a translucent prompt bar slides up over the lower third (audio keeps playing, ducked −6dB); types or speaks the question; agent responds inline as a card *above* the transcript without leaving the player. *User feels: I never had to break flow.*
4. **Scrubbing back to a half-remembered moment.** User drags the scrubber; the waveform expands to fill 1/3 of the screen, speaker stripes appear underneath it, transcript text *also* scrubs in sync (preview lines fly past the now-line). User releases at the right speaker color. *User feels: I can see my way back.*
5. **Returning after an interruption (call, CarPlay handoff).** A small glass pill appears top-center: *"Resume at 24:18 — 'and that's where the protocol changes.'"* One tap, audio resumes mid-sentence with context. *User feels: nothing was lost.*

## 3. Information Architecture

Single screen, vertically zoned, but the zones **redistribute their weight based on user intent** (artwork-dominant vs transcript-dominant vs scrub-dominant). All controls reachable in the lower 50% on iPhone 16 Pro Max — thumb territory.

```
TOP        ─── Status / Resume Pill (transient)
ZONE A     ─── Cover Art (collapses to 88pt thumbnail in transcript-focus)
ZONE B     ─── Show • Episode • Chapter (editorial header)
ZONE C     ─── Live Transcript (the protagonist) — auto-scrolling, speaker-tinted
ZONE D     ─── Waveform + Scrubber + time (compact 56pt; expands to 220pt while scrubbing)
ZONE E     ─── Primary Controls (skip-back-15, play, skip-fwd-30)
ZONE F     ─── Glass Action Cluster: speed • sleep • AirPlay • queue • share • bookmark
ZONE G     ─── Agent Chip (single floating glass capsule, always visible, lower-right)
```

**Default (artwork-dominant):** A:42% · C:30% · D+E+F:24% · G floating
**Transcript-focus (user swipes up on transcript):** A:12% · C:60% · D+E+F:24% · G floating
**Scrub mode (user touches scrubber):** A blurs 22pt · D expands to 220pt with speaker stripes · C dims to 30% opacity but keeps moving

**Handoffs (boundary discipline):**
- Tap show name → Episode Detail (UX-03 owns)
- Long-press transcript line → "Ask agent about this" → expands inline; "Open full chat" hands off to Agent Chat (UX-05)
- Double-tap a noun the agent has linked → Wiki peek sheet (UX-04 owns; we own only the invocation)
- Voice button (in Agent chip) → Voice Mode (UX-06)
- Swipe down on hero → mini-bar; user is in last viewed root tab
- Queue chip → Queue sheet (UX-02 owns; we own the chip)

## 4. Visual Treatment (iOS 26 Liquid Glass)

**Material strategy.** Background is a 60pt blur of the album art at 1.4× saturation, slowly drifting (Ken Burns, 40s loop, ±3% scale). Over that, **three glass tiers**:

- **Tier 1 — Hero glass (transcript card):** `.glassEffect(.regular, in: .rect(cornerRadius: 28))`, no tint. The transcript floats here.
- **Tier 2 — Control glass (waveform, transport row, action cluster):** `.glassEffect(.regular.interactive(), in: .rect(cornerRadius: 22))`, wrapped in a single `GlassEffectContainer(spacing: 14)` so the play button and side controls morph into one continuous shape when the user lifts off the play button.
- **Tier 3 — Agent chip:** `.glassEffect(.regular.tint(accent.opacity(0.35)).interactive(), in: .capsule)`. Sole tinted glass on the screen — it's the only thing that should glow.

**Color.** Extract two dominant colors from cover art (`UIImage.dominantColors`). Primary is the wallpaper accent; secondary tints the active speaker line. Speaker palette is **derived from cover art**, not from a fixed system palette — every show has its own palette so memory builds: *"the green-haired guest"*. Speaker color persists across episodes for the same diarized identity (cached by speaker_id).

**Typography.**
- Show: `SF Pro Display`, semibold, 13pt, tracking +0.6, uppercase — editorial dateline feel.
- Episode title: `New York` (serif), regular, 22pt, leading 26 — a *magazine* title, not an app title.
- Transcript: `SF Pro Text`, regular, 19pt body / 24pt active line / leading 1.45 — generous, readable, scales to xxxLarge without breaking.
- Speaker label: `SF Mono`, 11pt, tracking +0.4 — slightly technical, sits well next to colored chip.
- Time: `SF Pro Rounded`, tabular numerals — never jitters.

**Motion principles.** Calm by default; springs over eases; nothing linear except the playhead.

## 5. Microinteractions (curves, durations, haptics)

| Interaction | Curve / Spec | Notes |
|---|---|---|
| Mini-bar → full player | `matchedGeometryEffect` with `.interactiveSpring(response: 0.45, dampingFraction: 0.82)` | Cover art, title, play glyph, scrubber all share IDs |
| Transcript line activation | 220ms `ease-out` opacity 0.55 → 1.0 + 14pt size step | Triggered when audio timestamp crosses line start |
| Auto-scroll between lines | Continuous `easeOut` over the line's *audio duration*, not stepwise | Reads as breathing, not snapping |
| Manual scroll detected | Auto-scroll pauses; "Return to live" pill fades in 180ms `ease-out` | Pill persists until tapped or audio re-enters visible range |
| Scrubber engaged | Artwork scales 1.04 + wallpaper blur 60pt → 90pt, `.spring(response: 0.35, dampingFraction: 0.7)`; waveform expands 56pt → 220pt | Strong haptic `.soft` impact on touch-down |
| Scrub release | Snap to nearest **sentence boundary** within ±400ms tolerance | Avoids landing mid-word on drifty publisher SRTs |
| Hold-to-clip on transcript line | 600ms long-press; rising haptic intensity (`.light` → `.medium` → `.heavy`); glass ring fills `circular` around line as progress | Release before commit cancels |
| Agent chip tap | Capsule morphs into prompt bar via `GlassEffectContainer` + `glassEffectID("agent", in: namespace)` + `withAnimation(.spring(response: 0.5, dampingFraction: 0.78))` | Audio ducks −6dB, doesn't pause |
| Agent response card | Slides in from above transcript, glass tier 1, `.spring(response: 0.55, dampingFraction: 0.85)` | Pushes transcript down, doesn't overlay |
| Speed change (long-press play) | Hold play 280ms → speed dial appears (0.8/1.0/1.2/1.5/2.0), tilt thumb to choose | One-thumb operable, no menu dive |
| Chapter cross | Glass divider sweeps across the now-line, 320ms `easeInOut`, soft haptic `.rigid` | Names the new chapter inline for 2.4s |
| Bookmark add | Star fills with cover-art accent, `.spring(response: 0.3, dampingFraction: 0.6)` | Bookmark also writes a transcript-anchored note |
| Resume pill (returning) | Pill drops 220ms `ease-out`, breathes once, then settles | Shows current sentence text, not just timestamp |

## 6. ASCII Wireframes

### 6.1 Default (artwork-dominant)

```
┌─────────────────────────────────────────────┐
│   ◂                              ⋯           │  ← top bar (glass, scrolls away)
│                                              │
│           ┌───────────────────┐              │
│           │                   │              │
│           │     COVER ART     │              │  Zone A — 42%
│           │  (with shimmer    │              │
│           │   on tap)         │              │
│           └───────────────────┘              │
│                                              │
│   THE TIM FERRISS SHOW · #742               │  Zone B — show
│   The Keto Protocol With Dom D'Agostino     │       — episode (NY serif)
│   Chapter 3 · Mitochondrial efficiency      │       — chapter
│ ─────────────────────────────────────────── │
│   ░ TIM       So when you say "ketones"…   │  Zone C — transcript
│ █ ▓ DOM       I mean beta-hydroxybutyrate. │  ← active line, lit, tinted
│   ░ DOM       Which the brain prefers…     │
│ ─────────────────────────────────────────── │
│   ▁▂▅▇█▇▅▃▂▁▂▃▅▇█▇▅▃▁  24:18 / 1:12:04   │  Zone D — waveform + time
│                                              │
│         ⏮  15      ▶ ▌▌      30  ⏭         │  Zone E — transport
│                                              │
│   1.2×   🌙   ⌁ AirPlay   ☰ Queue   ↗ Share │  Zone F — actions (glass)
│                                       ┌──┐  │
│                                       │ ✦│  │  Zone G — agent chip
│                                       └──┘  │
└─────────────────────────────────────────────┘
```

### 6.2 Transcript-focus (user swiped up)

```
┌─────────────────────────────────────────────┐
│ [🎨] THE TIM FERRISS SHOW · #742            │  art shrinks to 88pt thumb
│      The Keto Protocol With Dom D'Agostino  │
│ ─────────────────────────────────────────── │
│   ░ TIM    Most people get this wrong…     │
│   ░ DOM    Right, because the literature…  │
│   ░ TIM    Wait, can you define ketosis?   │
│ █ ▓ DOM    Sure. Ketosis is a metabolic…   │  ← active, full color block
│   ░ DOM    state where the body shifts…    │
│   ░ DOM    primary fuel from glucose to…   │
│   ░ TIM    And how long until adaptation?  │
│   ░ DOM    Two to six weeks for most…      │
│ ─────────────────────────────────────────── │
│   ▁▂▅▇█▇▅▃▂▁▂▃▅▇█▇▅▃▁                     │
│         ⏮  15      ▶ ▌▌      30  ⏭         │
│   1.2×   🌙   ⌁   ☰   ↗            ┌──┐    │
│                                     │ ✦│    │
└─────────────────────────────────────────────┘
```

### 6.3 Scrubbing (waveform expanded, semantic stripes)

```
┌─────────────────────────────────────────────┐
│   [art blurred 90pt, scaled 1.04]            │
│                                              │
│   THE TIM FERRISS SHOW · #742  (dimmed)     │
│ ─────────────────────────────────────────── │
│   (transcript dimmed 30%, scrubbing past)   │
│ ─────────────────────────────────────────── │
│                                              │
│   ▁▂▅▇█▇▅▃▂▁▂▃▅▇█▇▅▃▂▃▅▇█▇▅▃▂▁▂▃          │  ← waveform 220pt
│   ████░░░░████████░░░░████████░░░██████    │  ← speaker A stripe (Tim)
│   ░░░░████░░░░░░░░████░░░░░░░░██░░░░░░    │  ← speaker B stripe (Dom)
│   ░░░░░░░░░░▓▓▓▓░░░░░░░░░░░░░░░░░░░░░░    │  ← ad break (muted gold)
│           ╳ 24:18 ─── snapping to "Ketosis │
│             is a metabolic state where…"   │
│                                              │
│         ⏮  15      ▶ ▌▌      30  ⏭         │
└─────────────────────────────────────────────┘
```

### 6.4 Agent quick-prompt invoked

```
┌─────────────────────────────────────────────┐
│   THE TIM FERRISS SHOW · #742               │
│ ─────────────────────────────────────────── │
│   ┌───────────────────────────────────────┐ │
│   │ ✦  beta-hydroxybutyrate is a ketone  │ │  ← agent inline answer
│   │    body produced when fat is broken  │ │     glass tier 1 card
│   │    down for fuel. Brain prefers it   │ │
│   │    over glucose under fasting.       │ │
│   │    [Open in Wiki ▸] [Ask follow-up]  │ │
│   └───────────────────────────────────────┘ │
│   ░ TIM   So when you say "ketones"…       │
│ █ ▓ DOM   I mean beta-hydroxybutyrate.     │
│   ░ DOM   Which the brain prefers…         │
│ ─────────────────────────────────────────── │
│   ▁▂▅▇█▇▅▃▂▁▂▃▅▇█▇▅▃▁                     │
│   ┌─────────────────────────────────────┐  │
│   │ ✦  what's keto?                  ◉│  │  ← prompt bar (was chip)
│   └─────────────────────────────────────┘  │  ← morphed via glassEffectID
│         ⏮  15      ▶ ▌▌      30  ⏭         │
└─────────────────────────────────────────────┘
```

### 6.5 Mini-bar (root tabs)

```
┌─────────────────────────────────────────────┐
│  [tab content above]                        │
│                                              │
│ ┌─────────────────────────────────────────┐ │
│ │[🎨] Ketosis is a metabolic…    ▌▌  ⏭   │ │  ← mini bar (glass tier 2)
│ │      ▁▂▅▇█▇▅▃▂▁▂  24:18                │ │     active line + waveform
│ └─────────────────────────────────────────┘ │
│   Library    Search    Agent    Settings    │
└─────────────────────────────────────────────┘
```

The mini-bar uniquely shows the **active transcript line, not just the title** — a 1-line ticker. This is the signature.

## 7. Edge Cases

- **No transcript yet:** "Generating transcript…" shimmer where lines would be; low-fi waveform from local AVFoundation analysis only (no speaker stripes); agent chip shows "limited" glyph but still answers from wiki/RAG of other episodes.
- **Streaming transcript (live ElevenLabs Scribe):** lines arrive in chunks; un-arrived region shows a faint blur and a small caret indicator; tapping ahead of the caret shows "Transcribing… tap to jump anyway".
- **Uncertain diarization:** speaker chip reads "Speaker 2" with a dotted outline; long-press → "Name this speaker" sheet. We never silently hide ambiguity.
- **Publisher transcript drift:** scrub-release snaps to nearest sentence boundary within ±400ms tolerance; if drift exceeds 800ms persistently, banner offers "Re-align transcript" (re-runs ASR locally on suspect region).
- **Very long episodes (4h+):** waveform downsamples; chapter markers become primary navigation; "Jump by chapter" appears in scrub overlay.
- **Slow network / buffering:** play button gains a thin progress ring; transcript continues to render from already-downloaded chunks; no full-screen spinner ever.
- **Interrupted by call / Siri:** on resume, pill shows last sentence + timestamp; one-tap resume.
- **AirPlay engaged:** route badge in glass cluster; transcript becomes optional (may degrade to chapter list on tvOS reflection); local controls still work.
- **CarPlay handoff:** UX-11 owns CarPlay; we ensure the player publishes `MPNowPlayingInfoCenter` with chapter, speaker, and line metadata so CarPlay's Liquid Glass surface can render speaker chip + current line.
- **Headphones removed:** auto-pause; pill: "Paused — headphones disconnected."
- **Background return after long absence:** if >30min, replay last 5s on resume (broadcast convention).

## 8. Accessibility

**VoiceOver narration order:** Episode context (show + title + chapter) → "Now playing, paragraph by [Speaker]" → current line → "Playback controls" → primary transport → secondary actions → agent.

**Custom rotors:** *Chapter*, *Speaker*, *Transcript line*, *Bookmark*. User can swipe between speakers with the rotor — a unique-to-this-app affordance.

**Dynamic Type:** Transcript scales fluidly to `xxxLarge`; at `accessibility5` the artwork hides automatically and transcript fills the screen (pre-built transcript-focus mode reused). Controls never shrink — they grow with text.

**Audio-only navigation:** Every action achievable with no visual reference. Tripled tap on play = "what is currently playing?" speaks the show, episode, chapter, and current sentence. Two-finger double-tap = "ask agent" voice-only flow (defers to UX-06).

**Screen-off parity:** All primary actions exposed via `MPRemoteCommandCenter` and Lock Screen Live Activity (UX-11 owns the surface; we own the data contract: episode, chapter, speaker, current line).

**One-hand reachability:** All primary controls live in the lower 50% of the screen on iPhone Pro Max; transcript scroll is initiated from the *upper* half so the thumb on transport never accidentally scrolls.

**Contrast:** Active transcript line maintains ≥ 7:1 (AAA) against the blurred wallpaper via a regular glass plate underneath, regardless of cover art. Speaker tint is *added* to a high-contrast base, never replaces it.

**Reduce Motion:** Ken Burns disabled; spring morphs become 180ms cross-dissolves; auto-scroll becomes step-on-line-change instead of continuous.

**Reduce Transparency:** Glass tiers fall back to solid `.thinMaterial` with stronger borders; speaker color fills line backgrounds at 0.18 opacity instead of acting as a tint.

## 9. Open Questions / Risks

1. **Auto-scroll lock-out duration after manual scroll** — we propose: indefinite, until user taps "Return to live" pill (Slack model). Alternative: 8s timeout. Needs user testing.
2. **Speaker color stability across episodes** — keyed on diarized speaker_id, but cross-episode identity matching is non-trivial. If we get it wrong, the "green-haired guest" memory hook breaks. Spec needs to specify the matching threshold and a manual override.
3. **Where does Nostr share live?** Inside the standard Share sheet as an extension, or as a primary chip alongside ↗ Share? Coordinate with UX-12. Recommendation: extension only — keep the surface uncluttered.
4. **Agent inline-answer vs full-chat threshold** — at what point does an inline answer escalate to opening UX-05 Agent Chat? Proposed rule: ≤3 turns inline; "Continue in chat" CTA appears on turn 3.
5. **Wiki peek invocation** — automatic noun-linking inside transcript could feel magical or noisy. Proposal: linked nouns are *invisible* until the user enters "explore mode" (long-press anywhere on transcript), then key entities surface as underlined glass.
6. **Clip-share trim UX** — auto-snap to sentence is great for speed but users will want to extend. Proposal: clip sheet opens with sentence pre-trimmed and two glass handles to extend ±. (May overlap with UX-03; coordinate.)
7. **What does the *non-active* portion of the waveform encode** when no diarization is yet available? Just amplitude? Risk of feeling incomplete. Proposal: amplitude only, with a hairline annotation ("speaker stripes appear as transcript completes").
8. **Performance budget for live glass + auto-scrolling transcript + waveform redraw** — must hit 120Hz on iPhone 15 Pro and gracefully degrade on iPhone 13. Engineering risk worth flagging for UX-15.

---

*Better than Spotify. Better than Overcast. Better than Pocket Casts. Better than Castro. The transcript is the surface; the agent is one corner of glass away; nothing breaks the flow.*
