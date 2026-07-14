# UX-12 — Nostr Communication Surface

> Owner: Designer (Aditi). Coordinates with #5 Agent Chat, #6 Voice Mode, #10 Onboarding, #14 Proactive Agent, #15 Liquid Glass.
> Inherits from: `App/Sources/Domain/Friend.swift`, `NostrPendingApproval.swift`, `App/Sources/Services/NostrRelayService.swift`, `NostrKeyPair.swift`, `App/Sources/Features/Friends/FriendDetailView.swift`, `App/Sources/Features/Settings/Agent/AgentFriendsView.swift`, `AppStateStore+Nostr.swift`, `AppStateStore+Friends.swift`.

---

## 1. Vision

Nostr is **not a tab. It is a relay layer** that lets people — and the agents they trust — exchange podcast knowledge as if they shared one library. A clip you send a friend lands in their app as a *playable, transcript-aware artifact*, not a URL. A question your friend's agent asks of your library returns prose, not a payload. A command you fire from your laptop arrives at your phone as a normal agent reply, with a small "via desktop" glyph the only tell.

Three principles:

1. **Provenance is ambient, never a banner.** Every action carries a hairline "via @friend" or "via desktop" glyph; the message itself is the same as a local one.
2. **Trust is tiered, not binary.** A friend can be a *reader* (queries only), a *suggester* (drafts that need your tap), or an *actor* (full tool access, scoped). Tiers have visual weight that match their power.
3. **The wire is invisible until it matters.** Relay status, encryption posture, and pending approvals only surface when the user must decide. Otherwise, Nostr feels like iMessage with better memory.

---

## 2. Key User Moments

1. **Send a clip to a friend.** Long-press a 14-second passage in #3 Episode/Transcript → *Share → To a friend*. Sheet presents friends as glass cards (avatar, npub short, online dot if their relay has seen them recently). Pick Maya. Quick caption. Send. On Maya's phone the clip materializes as a *native* clip card in her Now Playing thread — same shell as locally generated clips, with a "from Pablo" chip in place of the timestamp.

2. **Friend's agent asks your library a question.** Maya types in her chat, "Ask Pablo's library what Huberman said about Zone 2 last month." Her agent fires a Nostr DM to my agent. On my device a non-blocking *Library Query* card appears in the **Agent Chat → Friends** thread: "Maya's agent is asking your library about *Zone 2*. Permission tier: Reader. Allow once / Always / Decline." If I've pre-allowed Reader queries from Maya, it auto-runs and just *logs* a hairline receipt. The answer streams back to Maya's chat as if her own agent answered it — but with a small "sourced from @pablo's library" footer.

3. **Drive my own agent from another device.** From a Nostr client on my desktop I send my own npub a DM: "Make a 12-min briefing for tomorrow's commute." My phone's agent receives it, runs `generate_briefing`, replies on the same thread. On the phone the chat shows the message with a `􀙗 desktop` glyph at the message's leading edge — same prose, different origin.

4. **See provenance after the fact.** In Library or Episode detail, any item modified via Nostr (a clip saved by Maya, a wiki annotation a friend's agent added, a queue item my desktop pushed) carries a hairline `via @maya` chip on hover/tap. In the Activity drawer (#15), a chronological log groups Nostr-originated actions under a "Off-device" section.

5. **Set permissions for a friend.** Friend Detail → *Permissions* card. Three large glass tiers stacked; current tier glows. Below: per-tool overrides (toggle `query_transcripts` off even for Actor; allow `send_clip` even for Reader). A *Recent activity* timeline shows what the friend's agent has done in the last 30 days, each row tappable for full payload.

6. **Reveal my npub for a new friend.** Profile → *My Identity* glass card → tap the npub. It morphs (`glassEffectID`) into a full-screen QR with the npub label below in mono. Long-press copies hex; share-sheet exports `nostr:npub…`. A small "Connected to 3 relays" status sits beneath, tappable for relay management.

---

## 3. Information Architecture

Nostr does **not** get a top-level tab. It distributes across surfaces:

```
Agent Chat (#5)
├── Thread list
│   ├── …local threads…
│   └── Friends section
│       ├── Per-friend threads (DM + agent-to-agent in one stream)
│       └── My Other Devices (auto-thread for self-DMs across devices)
└── Per-thread header carries a relay-state glyph (green / amber / grey)

Settings → Agent → Friends (existing AgentFriendsView, extended)
├── Friends list (sortable; pending-approval banner at top)
├── Friend Detail
│   ├── Profile header (avatar, npub, since-date)
│   ├── Permission tier + per-tool overrides
│   ├── Recent agent activity (last 30d)
│   ├── Open thread →  (deep-links into Agent Chat)
│   └── Remove
├── My Identity
│   ├── npub display + QR reveal
│   ├── My Other Devices (paired npubs, each with a label)
│   └── Relays (list, status, add/remove)
└── Pending Approvals (NostrPendingApproval feed)

Share Sheet (system + in-app)
└── "Send to a Nostr friend" action — friend picker grid

Provenance chips
└── Anywhere an item was created/modified via Nostr
```

The model leans on existing types: `Friend.identifier` is the hex pubkey; `NostrPendingApproval` becomes the queue for first-contact handshakes; the new `permissionTier: PermissionTier` and `toolOverrides: [String: Bool]` extend `Friend`.

---

## 4. Visual Treatment

**Two voices in one thread.** A friend thread mixes four message kinds; each gets a different visual register so the eye reads provenance without reading labels:

- **My human messages** — right-aligned, tinted glass capsule (system accent at 18%).
- **Friend's human messages** — left-aligned, neutral glass capsule, friend's avatar at 24pt as a leading marker.
- **My agent's messages** — left-aligned, **unbubbled** editorial serif (matches #5).
- **Friend's agent messages** — left-aligned, unbubbled serif, with a hairline vertical glass rule on the leading edge tinted to the friend's accent color, plus a small caps eyebrow `Maya's agent · 14:02`.

The eyebrow is the trust signal. Human-from-friend has no eyebrow (we trust faces). Agent-from-friend always carries one (we verify machines).

**Permission tiers as glass weight.** *Reader* is a thin, low-saturation capsule. *Suggester* has medium tint with a soft glow. *Actor* is `.glassProminent` with a tint matching the friend's accent — its visual weight matches its power. Switching tiers is a morph (`glassEffectID`) between the three capsules — never a segmented control.

**Share card aesthetics on the receiving end.** A clip shared from me to Maya renders in her thread as a clip card with:
- Show artwork tinting the glass behind it at 14% opacity.
- Speaker name in small caps, in/out timestamps in mono.
- A hairline `from Pablo` chip pinned to the top-leading corner; tap to see the full provenance card.
- The same in-place play / scrub affordance as a locally-clipped passage. Crucially, **no "open in app" friction** — the artifact is *already* native.

**Relay state.** Header carries a 4pt glyph: green (connected), amber (degraded, retrying), grey (offline). Tapping reveals a small popover with relay names, RTT, and "Switch primary." Never a banner. Never a modal. The thread continues to compose offline; sends queue with a soft pulse.

**Provenance chips.** A 9pt mono caption with a 6pt circular avatar, prefixed `via`. Always at the trailing edge of the metadata row, never in headlines. The chip is tappable → opens a sheet with the original Nostr event id (copyable), the tool call, and a *Revoke this action* button if reversible.

**Identity / npub QR.** The QR reveal is a *cinematic* moment: the npub card lifts off its row, expands to fill the screen, and the QR draws on with a 320ms staggered shimmer (rows of QR modules cascade in). Background dims to 40%. Tap anywhere outside dismisses with a reverse morph. Below the QR, a single line: `npub1…7q9` in mono with a copy glyph.

---

## 5. Microinteractions

- **Share sheet to Nostr friend.** Long-press a clip → sheet rises with a 2-column friend grid. Tapping a friend card *expands* it inline (no second screen) with a caption field and "Send" button. Send fires a 220ms checkmark morph in place, then the sheet auto-dismisses with a haptic `.success`. If the friend's agent acks within 1.5s, a tiny toast: *Delivered to Maya · seen 0s ago*.
- **Permission tier toggle.** Three-tier control with a draggable glass pill that snaps to *Reader / Suggester / Actor*. As you drag, the per-tool override list below dims/undims rows in real time so the user *sees* what changes. Releasing fires a subtle haptic and a one-line confirmation: *Maya can now suggest actions; you'll approve each one.*
- **npub QR reveal.** Tap → 320ms morph (matched-geometry from the npub card to the full-screen QR). QR modules cascade in. Long-press the npub instead → instant copy with a tiny "Copied" chip and `.notificationOccurred(.success)`.
- **Cross-device receipt.** When a message arrives from your other device, the chat row enters with a 200ms left-edge shimmer in a desaturated accent, then settles. The `􀙗 desktop` glyph fades in last. Subtle, but unmissable on a second look.
- **Pending approval handshake.** A new pubkey DMs your agent — a single non-modal pill rises from the bottom of Agent Chat: *New contact · maya@damus.io wants to talk.* Swipe right → Allow & open thread. Swipe left → Block. Tap → full approval card with their kind:0 metadata.
- **Permission revoked mid-action.** If the user revokes mid-stream, the in-flight tool result fades to 35% opacity with a hairline strike-through and a footer *Cancelled · permission revoked at 14:03*. The friend's side gets a graceful *"…declined to continue"* line.

---

## 6. ASCII Wireframes

### 6.1 Friends List (Settings → Agent → Friends)

```
┌──────────────────────────────────────────────┐
│ ‹ Settings           Friends              + │
│ ──────────────────────────────────────────── │
│ ⚠  Pending: noah@primal.net wants to connect│
│     [ Approve ] [ Block ]                    │
│ ──────────────────────────────────────────── │
│ 🜲 Maya          npub1…q9    Actor     2m   │
│    "always-on coach"                          │
│ 🜲 Pablo (me)    npub1…7k    Self       —   │
│    Desktop · Laptop                           │
│ 🜲 Tomás         npub1…3a    Reader    3d   │
│ 🜲 Alex          npub1…b2    Suggester 1w   │
│ ──────────────────────────────────────────── │
│  My Identity →                                │
│  Relays  •••  3 connected                     │
└──────────────────────────────────────────────┘
```

### 6.2 Friend Detail with Permissions

```
┌──────────────────────────────────────────────┐
│ ‹ Friends                Maya              ⋯ │
│ ┌──────────────────────────────────────────┐ │
│ │  ◐  Maya                                  │ │
│ │     npub1…q9   Friends since Mar 2026     │ │
│ │     "training partner · always-on coach"  │ │
│ └──────────────────────────────────────────┘ │
│                                                │
│  PERMISSION TIER                               │
│  ┌───────┐┌───────────┐┌──────────┐           │
│  │Reader ││ Suggester ││  Actor ✓ │           │
│  └───────┘└───────────┘└──────────┘           │
│  Maya's agent can run tools on your library.  │
│                                                │
│  TOOL OVERRIDES                                │
│   query_transcripts        ●● enabled          │
│   query_wiki               ●● enabled          │
│   play_episode_at          ●○ ask each time    │
│   send_clip                ●● enabled          │
│   generate_briefing        ○○ disabled         │
│                                                │
│  RECENT (30d)                                  │
│  • 14:02 · queried "Zone 2 in Huberman"        │
│  • Mon  · clipped 0:14 from Tim Ferriss        │
│  • Sun  · suggested an episode (declined)      │
│                                                │
│  [ Open thread → ]   [ Remove friend ]         │
└──────────────────────────────────────────────┘
```

### 6.3 Mixed Thread (DM with Maya, agent-to-agent + human)

```
┌──────────────────────────────────────────────┐
│ ‹ Friends      Maya       🟢 relay  ⋯       │
│ ──────────────────────────────────────────── │
│                       so curious about that ┤│
│                       Zone 2 stuff          ┤│
│                                       14:00 ▎│
│                                                │
│ ◐  yeah me too. ask my library?              │
│     14:01                                      │
│                                                │
│ ▎ MAYA'S AGENT · 14:02                         │
│ ▎ Across Pablo's library, Huberman frames     │
│ ▎ Zone 2 as the *aerobic base* — three        │
│ ▎ episodes converge on 180 minutes/week split │
│ ▎ over four sessions…                         │
│ ▎ ┌──────────────────────────────────────┐   │
│ ▎ │ ▶ Huberman · Cardio Foundations 47:12│   │
│ ▎ └──────────────────────────────────────┘   │
│ ▎  via @pablo's library · 2 sources           │
│                                                │
│                  saving this to my wiki   ┤   │
│                                  14:03 ▎      │
│ ──────────────────────────────────────────── │
│ [ 􀉪 ]  Message Maya…                  [ ↑ ] │
└──────────────────────────────────────────────┘
```

### 6.4 Share-Clip-to-Nostr Flow

```
Step A — long-press in transcript          Step B — friend picker rises
┌──────────────────────────────┐            ┌──────────────────────────────┐
│ "…the aerobic base is built  │            │   Send clip to a friend      │
│  in Zone 2, four sessions a  │            │   ┌────────┐ ┌────────┐      │
│  week, ninety minutes each…" │            │   │  Maya  │ │ Tomás  │      │
│  [ ▶ 0:14 selected ]         │            │   │ ◐  ●   │ │ ◐      │      │
│                               │            │   └────────┘ └────────┘      │
│  [Share ▾] Save · Wiki · ✕   │            │   ┌────────┐ ┌────────┐      │
│           ╲                   │            │   │  Alex  │ │  + new │      │
│            → To a friend      │            │   │ ◐      │ │        │      │
└──────────────────────────────┘            │   └────────┘ └────────┘      │
                                              │   caption: "this!"           │
                                              │              [ Send ↑ ]      │
                                              └──────────────────────────────┘

Step C — receiving end, Maya's thread
┌──────────────────────────────────────────────┐
│ ▎ from Pablo · 14:11                          │
│ ▎ ┌──────────────────────────────────────┐   │
│ ▎ │ ◐ Huberman · "the aerobic base…"     │   │
│ ▎ │ ▶━━━━━━━━○━━━━━━ 0:14                │   │
│ ▎ │ Save · Reply · Open episode          │   │
│ ▎ └──────────────────────────────────────┘   │
│ ▎  "this!"                                    │
└──────────────────────────────────────────────┘
```

### 6.5 Cross-Device Command Receipt

```
┌──────────────────────────────────────────────┐
│ ‹ Threads     My Other Devices    🟢        │
│ ──────────────────────────────────────────── │
│ 􀙗 from desktop · 09:14                      │
│   make a 12-min briefing for tomorrow's      │
│   commute                                    │
│                                                │
│ ◐ on it — 8 episodes scanned, drafting…     │
│   ┌──────────────────────────────────────┐   │
│   │ 􀊨 Tomorrow's Briefing · 12:00       │   │
│   │   ▶ play  ·  open  ·  send to phone │   │
│   └──────────────────────────────────────┘   │
│   09:15                                       │
│                                                │
│ 􀙗 from desktop · 09:16                      │
│   thanks, drop the third chapter              │
└──────────────────────────────────────────────┘
```

### 6.6 npub QR Reveal

```
                  (tap)                       (morph 320ms)
┌─────────────────────────┐         ┌──────────────────────────────┐
│  My Identity            │         │                              │
│  ┌───────────────────┐  │   ───▶  │      ▓▓▓ ▓ ▓▓ ▓▓▓            │
│  │ ◐ Pablo           │  │         │      ▓ ▓ ▓ ▓▓ ▓ ▓            │
│  │ npub1…7q9   [QR] │  │         │      ▓▓▓ ▓▓▓▓▓ ▓▓▓            │
│  └───────────────────┘  │         │      ▓ ▓ ▓ ▓ ▓ ▓ ▓            │
│  Connected · 3 relays   │         │      ▓▓▓ ▓ ▓▓▓ ▓▓▓            │
│                         │         │                              │
│                         │         │   npub1abcd…7q9     [copy]   │
│                         │         │   tap to dismiss             │
└─────────────────────────┘         └──────────────────────────────┘
```

---

## 7. Edge Cases

- **All relays down.** Header glyph turns grey. Composer still accepts messages — they queue locally with a hairline `queued` chip. On reconnect, queued messages send in order with a single soft pulse per delivery.
- **Friend's agent unavailable.** A query times out at 12s. The placeholder reply morphs into a small "*Maya's agent didn't answer · retry / send as DM*" card. The user can convert it to a human DM in one tap.
- **Slow delivery.** If a send takes >2s, the sent bubble shows a 1.5pt circular progress at its trailing edge. >10s adds a *Sending over Nostr…* caption. Never a spinner blocking input.
- **Permission revoked mid-action.** In-flight tool result fades to 35% with a hairline strike. A receipt row logs *Cancelled · permission revoked at 14:03* on both sides.
- **Pending approval ignored.** Pending approvals older than 14 days collapse to a single "3 older requests" row at the bottom of the friends list. Never auto-dismissed silently.
- **Untrusted incoming clip.** A clip from a non-friend pubkey is held in a *Holding* shelf in the friends list — visible, *not* in any thread, with metadata redacted to just the show name. User must promote sender to a friend before contents render.
- **Self-DM device unpaired.** If a paired device's npub is removed, its messages remain visible but the `􀙗 desktop` chip becomes a generic `􀉪 unknown device` and tapping it offers *Re-pair*.
- **Conflicting actions.** If my desktop and phone both fire `play_episode_at` within 2s, the phone wins (it owns playback) and a small *Reconciled · played from phone* footer appears on the losing message.

---

## 8. Accessibility

- **VoiceOver.** Every message announces sender + role: *"From Maya's agent: Across Pablo's library…"*. Provenance chips read as *"via Pablo, tap to see source event."* The QR reveal announces *"Your public key as QR code, double-tap to copy."*
- **Dynamic Type.** All chat surfaces respect AX5; the friend grid in the share sheet reflows from 2 cols → 1 col at AX3+. Permission tier capsules use `Layout` to stack vertically beyond AX3.
- **Reduce Motion.** QR reveal cross-fades instead of cascading. Glass morphs become opacity transitions. Cross-device shimmer becomes a static `􀙗` glyph.
- **Reduce Transparency.** Glass surfaces fall back to opaque `regularMaterial` with a 1pt hairline divider; provenance chips gain a contrasting outline.
- **Color independence.** Permission tiers also differ by *weight* and *icon* (Reader = 􀉭, Suggester = 􀈎, Actor = 􀊵), never color alone. Friend's accent color is *additive* to the leading rule, not the only marker.
- **Hit targets.** All chips ≥44pt; avatar markers in lists are tappable and not the only entry into Friend Detail (chevron present).
- **Audio-first usage.** Share-to-friend is reachable from voice mode (#6) — *"send that clip to Maya"* triggers the same flow with voice confirmation.

---

## 9. Open Questions / Risks

1. **Library privacy.** Even a Reader-tier friend can infer subscription habits via what their agent learns. Do we need a *redaction policy* (e.g. exclude Health, Finance shows from any cross-friend query)? Proposed: per-show *Shareable / Private* toggle in Library settings, defaulting to Shareable, with a sensitive-categories preset.
2. **Relay choice.** Default relays should be diverse and not Anthropic-/Apple-owned. Should we ship with a curated set + let the user add their own? Concern: a single bad relay can rate-limit critical agent-to-agent flows. Proposed: 3 relays minimum, parallel writes, first-ack reads.
3. **Nostr DM encryption.** NIP-04 is leaky (timing/metadata); NIP-44 is better. Anything sensitive (briefings derived from Health-tagged shows) should refuse to send unless the contact's relay supports NIP-44. Surface this as a one-time *upgrade your friend's app* prompt.
4. **Cross-device pairing UX.** How do we pair a desktop client to the same agent identity without leaking the seed? Proposed: phone generates a *delegation* key (NIP-26-style) per device, scoped to specific kinds, revocable from My Other Devices.
5. **Spam pubkeys.** First-contact handshakes can be abused. Add a soft rate limit (max 5 pending approvals/day) and a *proof-of-recent-activity* heuristic (only show approvals from pubkeys with kind:0 metadata + recent posts).
6. **Provenance permanence.** If a user removes a friend, do their past contributions still show *via @maya*, or anonymize to *via former friend*? Proposed: keep as historical truth, but allow a per-friend *Forget my actions* purge button on removal.
7. **Sensitive content over Nostr.** Some users will want to share clips that are politically sensitive. Should we provide an *expire after N days* option (using NIP-40)? Recommended: yes, default off, surfaced in the share sheet's caption row.
8. **Agent-to-agent loops.** Two Actor-tier friends could create unintentional recursion (their agents querying each other). Add a TTL on agent-originated DMs and a max hop count of 2.

---

File path: `/Users/pablofernandez/Work/podcast-player/.claude/briefs/ux-12-nostr-communication.md`
