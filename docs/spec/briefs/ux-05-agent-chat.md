# UX-05 — Agent Chat (text-mode)

> Owner: Designer (Aditi). Coordinates with #6 Voice Mode, #4 Wiki, #12 Nostr, #14 Proactive, #15 Liquid Glass.
> Inherits from: `App/Sources/Features/Agent/AgentChatView.swift`, `AgentChatBubble.swift`, `ChatMessage.swift`, and `Design/GlassSurface.swift`.

---

## 1. Vision

The Agent Chat is **iMessage's composure crossed with a magazine's voice**. It is not a chatbot UI. It is a *reading and listening surface* where you talk to every podcast you've ever subscribed to — and it talks back in artifacts: episode cards that play in place, transcript pull-quotes set in editorial type, wiki peeks that feel like Wikipedia at its calmest, citations that hover like footnotes. Tool calls don't intrude; they whisper underneath the answer.

Three principles drive every decision:

1. **The answer is the hero, the mechanism is the footnote.** Tool calls collapse by default. The user reads prose, taps an episode card, listens. They never *should* see a JSON blob unless they ask.
2. **The chat knows where the playhead is.** "This passage" always resolves. The composer carries an ambient *Now Playing* chip the user can tap-attach.
3. **Threads are conversations with podcasts, not sessions with a bot.** Each subscribed show gets an evergreen thread; cross-podcast questions live in *General*; briefings get their own thread that doubles as a player.

This is the chat surface that finally makes podcast knowledge *grabbable*.

---

## 2. Key User Moments

1. **Ground while listening.** User is mid-episode. Pulls up chat from the player's swipe-up handle. Composer already shows a small *attached* chip: `Tim Ferriss · 47:12`. They type "wait, what study was this?" Agent replies inline with a wiki peek + a 14-second clip card the user can scrub without leaving chat.
2. **Recall a half-remembered episode.** "Last week, the one about stamps." Agent runs `search_episodes`, replies with two episode cards stacked, ranked by confidence. Tap → opens at the timestamp. Long-press → "Send to friend" or "Add to wiki."
3. **Generate a briefing.** "Catch me up on this week, 12 minutes." Tool-call shimmer ("composing your briefing… 6 episodes, 3 sources"), then a single hero **Briefing card** with play affordance and a chapter list. Tapping play hands off to thread #8 surface but keeps chat above for interruptions.
4. **Cross-podcast question.** "What does Huberman vs. Attia say about Zone 2?" Agent answers with a *comparison block*: two columns of pull-quotes, each with speaker label and source-episode chip. Action chips: *Play both back-to-back · Save comparison to wiki · Share*.
5. **Peek a tool call.** A skeptic taps the small chevron under the answer. The Tool-Call Inspector slides in: a list of tools (`query_transcripts`, `query_wiki`, `perplexity_search`) with timing, args (pretty-printed), and a *Show evidence* link to the exact transcript chunks used. This is the trust surface.

---

## 3. Information Architecture

```
Agent Chat root
├── Thread List (left in iPad, modal sheet in iPhone)
│   ├── Pinned: General · Now Playing thread (auto-pinned while playing)
│   ├── Per-podcast threads (one per subscription, sorted by recent activity)
│   ├── Briefings (auto-created threads for each generated briefing)
│   └── Friend agent DMs (delegated to #12 Nostr; appear here with a small relay glyph)
├── Single Thread
│   ├── Header: thread title, podcast art (or General glyph), context-drawer affordance
│   ├── Message stream (mixed media, see §4)
│   ├── Action-chip rail (sticky just above composer, contextual to last agent reply)
│   └── Composer
│       ├── Now-playing attachment chip (auto-suggested when audio is active)
│       ├── User attachments (episode, clip, wiki page, screenshot)
│       ├── Voice-note record (long-press, separate from full Voice Mode)
│       └── Send / dictate
├── Context Drawer (pull down on header, see §5)
│   ├── What the agent currently "sees": now-playing, recently opened episodes,
│   │   wiki pages in scope, time window
│   └── Toggles: include online (Perplexity), include wiki, include unlistened
└── Tool-Call Inspector (sheet, summoned per-message)
    ├── Tool timeline with durations
    ├── Args / result JSON (pretty)
    └── Evidence: transcript chunks with speaker + timestamp
```

---

## 4. Visual Treatment

**Bubbles vs. unbubbled editorial.** User messages live in subtly tinted glass capsules, right-aligned, single-line padding. Agent messages are **unbubbled** — set in editorial serif (New York or a licensed serif), left-aligned, generous leading, like a column of *The Atlantic*. This single decision separates us from every other chat app on iOS.

**Embedded media as Liquid Glass cards.** Episode cards, clip cards, wiki peeks, comparison blocks, and briefing cards all share a common glass shell (`GlassEffectContainer` with `spacing: 32`, `cornerRadius: 22`) but differ in interior:

- **Episode card**: 56pt artwork, show title (caption), episode title (headline), a thin progress bar if listened, a play glyph that opens at the cited timestamp. Tap the card body → episode detail (#3). Tap the play glyph → starts playback inline at timestamp.
- **Clip card**: waveform thumbnail (rendered from amplitude envelope), in/out timestamps, inline play head with scrub. Long-press → "Save clip / Share / Add to wiki."
- **Wiki peek**: hairline rule, monospace small caps eyebrow ("Wiki · Zone 2 Training"), two-line excerpt, "Open wiki" chevron. Hands off to #4.
- **Transcript excerpt**: speaker label as small caps, body in italic editorial, timestamp tag right-aligned. A subtle vertical glass rule on the leading edge marks it as quoted material.
- **Perplexity citation**: numbered footnote chip `[1]` inline, a small *Sources* row at the bottom of the answer, each source a glass pill with favicon + domain.
- **Action-chip rail**: capsule glass buttons, `.glassProminent` for the primary action, `.glass` for the rest. Max 4 visible, horizontal scroll for more.

**Typography.** Editorial serif at 17/24 for agent prose; SF Rounded for chip labels; SF Mono Small for timestamps and tool-call args; San Francisco for user messages. Dynamic Type fully respected up to AX5.

**Color & light.** Background is the system Liquid Glass canvas — adapts to wallpaper. Show artwork in episode cards subtly tints the glass behind the card via a `tint(.color(showAccent))` at 18% opacity. Avoid hard surfaces; let blur and refraction do the work.

**Motion.** Messages enter with a 280ms ease-out spring, vertical 12pt rise, opacity 0→1. Glass cards morph using `glassEffectID` + `@Namespace` when streaming text reveals new attachments — the card *grows* out of the prose, never pops in. Tool-call collapsed badge pulses gently while streaming, settles into a static chevron when done.

---

## 5. Microinteractions

- **Pull down on header** → Context Drawer reveals what the agent currently "knows." Releases like a magazine table-of-contents.
- **Swipe left on any message** → reveals *Share* and *Save to wiki*. Swipe right → *Quote-reply* (drops a snippet into composer).
- **Long-press a transcript excerpt** → "Create clip from this quote." Opens clip-trim sheet pre-seeded with the surrounding ±15s.
- **Tap-and-hold the composer** → records voice note (separate from full Voice Mode #6); waveform appears live; release to send, slide-up to cancel.
- **Drag an episode from Library (#2) into chat** → composer accepts it as an attachment chip; user types question; agent receives it as grounding context.
- **Now-Playing chip** appears in composer when audio is active; tap to attach `{episode_id, timestamp}`; tap-and-hold to attach a *range* (use scrubber to pick).
- **Two-finger tap on agent message** → instant Tool-Call Inspector (power-user shortcut).
- **Haptic vocabulary**: `.soft` on chip select, `.rigid` on send, `.success` when a tool call completes, `.warning` on tool failure.

---

## 6. ASCII Wireframes

### 6.1 Thread List (iPhone modal sheet)

```
╭─────────────────────────────────────╮
│  ◀ Threads                    + new │
├─────────────────────────────────────┤
│ 📌 Now Playing — Tim Ferriss        │
│    "wait, what study was this?"     │
│                          2m · ●●○   │
├─────────────────────────────────────┤
│ 📌 General                          │
│    Briefing ready · 12 min          │
│                          14m · ●    │
├─────────────────────────────────────┤
│ 🎙 Huberman Lab                     │
│    Cross-ref'd Zone 2 across 4 eps  │
│                          1h         │
├─────────────────────────────────────┤
│ 🎙 Acquired                         │
│    "TLDR the TSMC episode"          │
│                          yesterday  │
├─────────────────────────────────────┤
│ 🪞 Briefings                        │
│   This Week · 12:04                 │
│   Last Week · 09:33                 │
├─────────────────────────────────────┤
│ 🤝 Friends (Nostr)                  │
│   Maya's agent shared a clip        │
╰─────────────────────────────────────╯
```

### 6.2 Single Thread — Mixed Media (iPhone)

```
╭─────────────────────────────────────╮
│ ◀  Tim Ferriss · Now Playing  ⌄ ⓘ  │  ← pull ⌄ for Context Drawer
├─────────────────────────────────────┤
│                                     │
│                ╭─────────────────╮  │
│                │ wait, what       │  │  ← user bubble, glass
│                │ study was this? │  │
│                ╰─────────────────╯  │
│                              9:42pm │
│                                     │
│  Patel et al., 2021 — a small      │  ← agent prose, editorial serif
│  randomized trial of 38 endurance  │
│  athletes comparing Zone 2 vs      │
│  threshold work over 12 weeks. ¹   │
│                                     │
│  ┌───────────────────────────────┐ │  ← wiki peek glass card
│  │ WIKI · Zone 2 Training        │ │
│  │ "Sustained effort below the   │ │
│  │  first lactate threshold..."  │ │
│  │                       Open ›  │ │
│  └───────────────────────────────┘ │
│                                     │
│  ┌───────────────────────────────┐ │  ← clip card, inline play
│  │ ▶  ░▒▓▒░▒▓▓▒░▒▓▒░  47:08–47:46│ │
│  │    Tim Ferriss · ep #682      │ │
│  └───────────────────────────────┘ │
│                                     │
│  Sources: [1] patel-2021.pdf       │
│                                     │
│  ▾ used 3 tools                    │  ← collapsed tool-call badge
│                                     │
├─────────────────────────────────────┤
│ [ Play this clip ] [ Save to wiki ] │  ← action chip rail
│ [ Find similar ]   [ Share ]        │
├─────────────────────────────────────┤
│ 🔗 Tim Ferriss · 47:12   ✕         │  ← now-playing chip in composer
│ ┌───────────────────────────┐  ◉   │
│ │ Ask anything…             │  ↑   │
│ └───────────────────────────┘      │
╰─────────────────────────────────────╯
```

### 6.3 Tool-Call Inspector (sheet)

```
╭─────────────────────────────────────╮
│ ✕  Tool calls for this answer       │
├─────────────────────────────────────┤
│                                     │
│  ● query_transcripts        184 ms  │
│    scope: "tim-ferriss"             │
│    query: "Zone 2 study citation"   │
│    → 4 chunks  [ Show evidence › ]  │
│                                     │
│  ● query_wiki               42 ms   │
│    topic: "Zone 2 Training"         │
│    → 1 page   [ Open page › ]       │
│                                     │
│  ● perplexity_search        912 ms  │
│    query: "Patel 2021 Zone 2 RCT"   │
│    → 3 sources   [ View › ]         │
│                                     │
│  Total · 1.14 s   Model: gpt-…      │
╰─────────────────────────────────────╯
```

### 6.4 Episode Card Embed (zoom)

```
┌──────────────────────────────────────────┐
│ ┌────┐  TIM FERRISS SHOW                 │
│ │ ▦▦ │  #682 · The Zone-2 Conversation  │
│ │ ▦▦ │  ▶  47:12 of 2:14:33   ░▒▓▒░░░  │
│ └────┘  [ Open ]              [ Play ▶ ]│
└──────────────────────────────────────────┘
```

### 6.5 Clip Card Embed (zoom)

```
┌──────────────────────────────────────────┐
│  ▶  ▁▂▄▆█▇▅▃▂▁▂▄▆█▇▅▃▂▁  00:38         │
│      ────●─────────────────              │
│   Tim Ferriss · ep #682 · 47:08–47:46    │
└──────────────────────────────────────────┘
```

### 6.6 Action-Chip Rail (states)

```
[● Play this clip ]  ← prominent (primary)
[ Save to wiki ]  [ Find similar ]  [ Share with… ]
                                      ▶ overflow
```

---

## 7. Edge Cases

- **Agent thinking long (>2s)**: replace typing dots with a *progress chip* showing the active tool name ("searching transcripts… 1.2s"). After 8s, append a cancel affordance. No spinner-only states.
- **Tool failure**: inline glass *whisper* in muted red — "Couldn't reach the wiki for *Zone 2* — retrying in 5s. [Retry now]". Never wall-of-text errors. Failed tool stays in inspector with a red dot for transparency.
- **Rate-limited / quota**: a one-time banner at top of thread: "You've used 80% of today's deep searches. Lighter answers below." Tone: matter-of-fact, never apologetic-corporate.
- **No internet**: composer disables online tools; chip "Offline · using on-device transcripts only" appears above composer in muted glass. Already-cached threads remain fully readable.
- **Very long answers**: cap initial reveal at ~12 lines + "Read more" disclosure; the rest expands inline with a 200ms reveal. Action chips stay sticky.
- **Low-confidence wiki entry**: cite with a dashed border on the wiki peek and an italic eyebrow "Wiki · *Draft entry*". Tap → wiki opens with edit/verify CTAs in #4.
- **Agent contradicts itself across sources**: surface as a comparison block (see §2.4), not as ambivalence in prose. Disagreement is content, not a bug.
- **Streaming interrupted mid-tool**: show partial answer with a "resume" pill; do not delete what's already been written.

---

## 8. Accessibility

- **VoiceOver threading**: each message is a single rotor stop with an accessibility label that *summarizes* embedded cards ("Agent message. Three sentences. One wiki peek: Zone 2 Training. One clip: 38 seconds from Tim Ferriss episode 682."). Cards are reachable via the `.containerNode` rotor for users who want depth.
- **Dynamic Type**: support all sizes through AX5; editorial serif body uses `.body` text style; cards reflow vertically at AX3+. Action chips wrap to two rows rather than truncate.
- **Voice input field**: dictate button sits left of send, distinct from voice-note record (long-press) and full Voice Mode (#6). Provide a one-tap "speak this answer aloud" affordance on every agent message — critical for eyes-off use while the user is driving or cooking.
- **Reduce Motion**: replace morphing transitions with cross-fades; tool-call shimmer becomes a static dot.
- **Reduce Transparency**: glass cards switch to solid surfaces using `AppTheme.Colors.surfaceElevated`; legibility is non-negotiable.
- **Contrast**: text-on-glass passes WCAG AA at all wallpaper conditions — verified via worst-case dark/bright wallpaper sampling. If sampled background contrast falls below 4.5:1, the card adds a `.regularMaterial` underlayer automatically.
- **Hit targets**: minimum 44pt; chip rail respects 8pt safe spacing; clip-card scrubber uses an enlarged invisible touch region.
- **Haptics**: every tactile cue has a non-haptic visual analog so users with haptics off aren't penalized.

---

## 9. Open Questions / Risks

1. **Per-podcast thread proliferation.** A user with 60 subscriptions gets 60 threads. Do we lazy-create on first interaction, or pre-create all? *Recommendation*: lazy-create, with a global search across threads.
2. **Now-Playing context attachment — opt-in or opt-out?** Default-attached respects "this passage means current playhead" but can leak unintended context. *Recommendation*: default-attach with a single tap to remove; never silent.
3. **Briefings — thread or its own surface?** Overlap with #8. *Coordinate with agent #8*: chat hosts the *card*, the briefing player is its own modal owned by #8.
4. **Friend agent DMs in chat list?** Coordinate with #12. Proposal: yes, with a distinct relay glyph and a separate section, but the message stream UI is identical so users don't have to learn two surfaces.
5. **Tool-call inspector — power feature or default?** Risk of intimidation. *Recommendation*: collapsed badge by default; first-run coachmark explains it.
6. **Editorial serif licensing** for agent prose. New York is system-supplied and free; consider as default. Risk if we want a custom serif — needs licensing review.
7. **Streaming + glass morphing perf** on older A-series chips. Test on iPhone 12 mini equivalent; degrade morph to fade if frame budget exceeded.
8. **Quote-reply UX** — should it visually link the new message to the quoted source, iMessage-style? Likely yes, but adds complexity to thread layout. Defer to v2.

---

**File**: `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-05-agent-chat.md`
