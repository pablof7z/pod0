# Identity-05 — Principled Synthesis

> Decisive synthesis of the four sibling proposals (`identity-01-minimal`, `02-power-user`, `03-ios-native`, `04-newcomer`). Picks the strongest call from each, names the trade-offs, and lands a single implementation contract two engineers can ship in parallel.
>
> **Voice**: Decisive, principled, ready to implement.
>
> Authority: glass tiers in [`ux-15-liquid-glass-system.md`](ux-15-liquid-glass-system.md); first-launch beat in [`ux-10-onboarding.md`](ux-10-onboarding.md) §S3; cross-surface provenance in [`ux-12-nostr-communication.md`](ux-12-nostr-communication.md).

---

## 1. The four siblings, distilled

**01 Minimal/Editorial** — typographic restraint. One T0 paper page, one T3 affordance, one editorial pull-quote for `about`, footer-quiet power paths, Shuffle + Paste-URL avatar (no host problem). Strongest call: the **editorial register** — identity as a portrait, not a configuration screen.

**02 Power-User/Nostr-Native** — truth-in-labelling. Hex, fingerprint, kind numbers, relay RTTs, full audit log. Defends a `glass.agent` tint for the `Bunker via Amber` mode badge. Strongest calls: **the signer-mode badge** as a permanent fixture wherever npub appears, and the **audit-log surface** (deferred to v2).

**03 iOS-Native/Apple-Settings** — predictability. Apple Account row at the top, push-not-sheet sub-flows, native `.alert`, `PhotosPicker` (honestly disabled until a host exists). Strongest calls: **push over sheet** for nsec / NIP-46 / Edit, and **promoting `Nip46ConnectCard`** to a primary surface — drop inner card chrome, flow into a `Form`.

**04 Newcomer's Profile** — jargon hygiene. The user never signed in, so frame the destructive action as "Start a new account." The npub becomes **"Account ID"** with a one-sentence explainer granting permission to ignore it. Strongest calls: the **"Account ID" label**, **six curated dicebear styles** (random reads as lazy), and the copy *"Used to sync your account across apps. You can ignore this unless you know you need it."*

---

## 2. The contested decisions, picked

| Decision | Pick | Source | Rejected because |
|---|---|---|---|
| **Where does Identity live?** | Top of `SettingsView`, free-standing first row above Library | 03, 04 (also 01) | "Scoped to Feedback" (today) trains users to think of identity as a Feedback concept; it isn't. |
| **Surface name** | **"Identity"** for the surface; **"Account ID"** for the npub block | 01, 02, 03 (surface) + 04 (field label) | "Profile" undersells the keypair reality power users need to see; "Account" overloads with "OpenRouter account." Hybrid resolves both. |
| **First-launch reveal** | UX-10 §S3 stands; 01's refinement: surface the slug, remove "Reveal key (advanced)" | 01 + ux-10 | Silent (03, 04) misses an earned beat — the agent will say the user's name in S4; the user should have *seen* it in S3. Toast (02) is power-user-only. |
| **Avatar source** | Curated 6 dicebear styles **+** Paste URL (power path). **No** PhotosPicker until media-host brief lands. | 04 (styles) + 01 (URL) | PhotosPicker (03) without Blossom is dishonest UX (03 itself disables the rows); Shuffle-only (01) misses 04's correct insight that the trained eye reads random as lazy. |
| **npub visibility** | Prominent on the Identity root, labelled **"Account ID"** with the 04 one-sentence explainer | 04 | Hidden in Advanced (would be 04-pure) makes copying for friend-add inconvenient; full hex/fingerprint (02) belongs in Advanced. |
| **The word "Nostr"** | Appears **once** on the Identity root (in the Account ID explainer); allowed in Advanced sub-pages by name | 01 | 04's "never" forces clumsy euphemisms in the import flow; 02's everywhere alienates newcomers. |
| **nsec import** | **Push view** from a Settings row inside Advanced sub-router | 03, 04 | Sheet (01) feels transient for a credential gate; push surface signals "you're going somewhere." |
| **NIP-46 surface** | **Promote** existing `Nip46ConnectCard` to a primary push view (drop outer card chrome, flow into a `Form`-style page) | 03 | Sheet-on-sheet (01) is fragile; building a fresh primary surface (would be 02) duplicates working code. |
| **Mode badge** | **Adopt 02's "Local Key" / "Bunker via Amber" badge** wherever the npub appears (Settings row, Identity root, Audit Log v2) | 02 | The threat model and latency really do differ; refusing to surface the difference (01, 03, 04) misleads users who connected a bunker. |
| **Audit log** | **v2** — out of MVP. Surface the *concept* in the brief, defer the `SignedEventLog` infra. | 02 (deferred) | The infra cost (new local store, per-relay ack tracking) doesn't pay for itself for v1. The wiring contract in §5 is the v1 substitute. |
| **Multi-account** | **v2** — keychain migration noted, MVP ships single-account. | 02 (deferred) | Same reason. |
| **Sign-out copy** | **"Start a new account"** in Advanced, not "Sign Out" at the bottom of Identity root. | 04 | The user didn't sign in. Apple's "Sign Out" model (03) presumes a sign-in event that didn't happen. |

---

## 3. Information architecture

```
SettingsView (existing)
└── Identity row (NEW, top, above Library)             ← §4.1
    └── IdentityRootView (push)                        ← §4.2
        ├── [Edit profile] button → EditProfileView    ← §4.3
        │   └── "Change photo" sheet
        │       ├── Choose a style (6 curated)         ← §4.4
        │       └── Paste image URL
        ├── [Account ID] block — copy + share          ← §4.2
        └── Advanced row → AdvancedView (push)         ← §4.5
            ├── Use my own key (push)                  ← §4.6
            ├── Sign in with a remote signer (push)    ← §4.7  (promoted Nip46ConnectCard)
            ├── Account details (push)                 ← §4.8  (hex, fingerprint, mode, relays)
            └── Start a new account (alert)            ← §4.9
```

`UserIdentityView.swift` (today, scoped to Feedback) is **deleted**. Feedback's toolbar identity icon deep-links into `IdentityRootView` instead of presenting its own sheet.

---

## 4. Surfaces — wireframes & specs

### 4.1 Settings → Identity row

```
┌──────────────────────────────────────────────────────────┐
│  Settings                                                │
│                                                          │
│  ┌────┐                                              ›   │
│  │ ◐  │   Bright Signal                                  │
│  │ ◑  │   Local Key · npub1pl7…q9k4                      │
│  └────┘                                                  │
│                                                          │
│  LIBRARY                                                 │
│  ◍  Subscriptions                              47    ›   │
│   …                                                      │
└──────────────────────────────────────────────────────────┘
```

- **Glass tier**: T1 clear (one quiet lift in Settings — your face deserves it; 04's argument). 60pt avatar, hairline ring (turns `accent.live` if signer is `failed`, `warning` if last-acked age > 24h — 02's signal repurposed as a *passive* indicator, no chip).
- **Top line**: `display_name` (kind-0) → fallback `name` (slug) → fallback `npubShort`. SF Pro Rounded Semibold 17/22 (`headline`).
- **Second line**: **mode badge + npub fragment**, mono `caption`. `Local Key · npub1pl7…q9k4` for local, `Bunker via Amber · npub1pl7…q9k4` for remote. The mode badge is plain text here (no glass tint at this size — the tint pays off only at full Identity root size).
- **Tap**: pushes `IdentityRootView` with `motion.standard`.

### 4.2 IdentityRootView (push)

```
┌──────────────────────────────────────────────────────────┐
│  ‹ Settings              Identity                        │
│                       ╭───────────╮                      │
│                       │   ◐◑◐◑    │   ← 96pt T0, hairline│
│                       ╰───────────╯     no breathing     │
│                                                          │
│                    Bright Signal                         │
│                  bright-signal-a3f2                      │
│                  ╭─────────────╮                         │
│                  │  Local Key  │   ← T2 mode badge       │
│                  ╰─────────────╯     (Bunker→glass.agent)│
│                                                          │
│              ╭──────────────────────╮                    │
│              │     Edit profile     │   ← T3 onyx        │
│              ╰──────────────────────╯                    │
│                                                          │
│   ─────────────────────────────────────────────────      │
│   ABOUT                                                  │
│      "                                                   │
│      A new account, freshly minted.                      │
│      Tell people who you are.                            │
│                                                          │
│   ─────────────────────────────────────────────────      │
│   ACCOUNT ID                                             │
│   npub1pl7…q9k4                            [Copy] [QR]   │
│   Used to sync your account across apps that             │
│   use Nostr. You can ignore this unless you              │
│   know you need it.                                      │
│                                                          │
│   ─────────────────────────────────────────────────      │
│   ◌  Advanced                                       ›    │
└──────────────────────────────────────────────────────────┘
```

**Composition** (drawing from 01's editorial register, 04's "Account ID" block, 02's mode badge):

- **Background**: T0 paper (`bg.canvas`). No card stack. The page reads like front matter.
- **Avatar**: 96pt, T0, 1pt hairline ring. **No breathing**, no animated rotation — calm by default per ux-15 §9.
- **Display name**: New York Medium 28/32 (`display.large`). Centered.
- **Slug**: SF Mono Medium 13/17 (`caption`), `text.tertiary`. Centered.
- **Mode badge**: T2 capsule, `Corner.pill` (14pt). Tint = `glass.clear` for `Local Key`, `glass.agent` for `Bunker via X`. Label `caption.small` SF Pro Medium uppercase. **The mode badge is the full-page place where the 02 signer-mode call appears.** From this page outward, every place that shows the npub also shows the badge.
- **Edit profile**: `.glassProminent` (T3 onyx), 220pt centered, `headline` label.
- **About**: 01's editorial pull-quote — New York Medium 19/26, oversized opening quote glyph in `text.tertiary` baseline-shifted -8pt, hanging indent 24pt, no closing quote. T0 paper. Empty-state copy *"A new account, freshly minted. / Tell people who you are."* in italic.
- **Account ID block**: SF Mono Medium for `npub1…` truncated middle. Trailing actions: `[Copy]` (24pt SF Symbol `doc.on.doc`, 44pt tap target) and `[QR]` (`qrcode` glyph; tap presents the QR sheet specified in `ux-12` §6.6 — single source of truth, do not redesign). **Copy success** swaps glyph + label to `checkmark` + "Copied" for 1.4s with `Haptics.success` and `UIAccessibility.post(notification: .announcement)`.
- **Account ID explainer**: 04's exact copy. `subhead` (15/20), `text.secondary`, max 2 lines. **This is the only place the word "Nostr" appears on the Identity root.**
- **Advanced row**: standard list row at the bottom, `chevron.forward`, `text.tertiary`. Pushes `AdvancedView` (§4.5).

**Deliberately absent**: status chips ("Connected"), kind-0 publish progress, relay counts, copper anywhere (reserved per ux-15 §9.2), agent gradient as a fill (only as the 2pt selected-style ring in §4.4), QR inlined on the page (it's an action, not a fixture — 04's call), Sign-out button (lives in Advanced — §4.9, 04's "the user didn't sign in" reasoning).

### 4.3 EditProfileView (push)

Push from the `Edit profile` button. Title `Edit Profile`, inline. Trailing **Save** (disabled until dirty; spinner during publish; ghosts to `.glass` when clean).

```
┌──────────────────────────────────────────────────────────┐
│  ‹ Identity         Edit Profile               Save      │
│                       ╭───────────╮                      │
│                       │   ◐◑◐◑    │                      │
│                       ╰───────────╯                      │
│                  [ Change photo ]                        │
│   DISPLAY NAME                                           │
│   │ Bright Signal                                │       │
│   USERNAME                                               │
│   │ bright-signal-a3f2                           │       │
│   ABOUT                                                  │
│      " Listening from Madrid. Mostly tech and history.  │
│                                  140 characters left     │
└──────────────────────────────────────────────────────────┘
```

- **Form** with `.insetGrouped` body but **paper-on-paper field treatment** (01's hairline-only rule) — `bg.canvas`, no field fill, 0.5pt hairline that thickens on focus. Section caps in `caption.small` uppercase tracking +0.2.
- **Change photo**: chip-style button below the avatar preview, `caption` SF Pro Medium, T1 clear. Opens action sheet (§4.4).
- **Display name**: 0–48 chars, trimmed on save. Empty allowed (falls back to slug).
- **Username**: 1–32 chars, unicode allowed (01's anti-chauvinism call). Empty disallowed; cleared field restores prior on blur with `Haptics.light`.
- **About**: `TextEditor`, 0–280 chars, oversized opening quote as editor chrome (01's typesetting). Counter visible only when remaining ≤ 50.
- **Save semantics**: signs and publishes one kind-0 to `FeedbackRelayClient.profileRelayURLs`. On first relay ack: `Haptics.success`, pop. On total failure within 8s: inline `warning` banner *"Saved on this device. We'll try again when you're online."* — sheet stays open, local edit persists.
- **Cancel with dirty form**: `.alert` (per existing `UserIdentityView` comment about iOS 26 popover-elision) — *"Discard changes?"* / *"Discard"* (destructive) / *"Keep editing"*.

### 4.4 Change photo — action sheet → style picker

Action sheet with **two** entries (MVP):

| Action | Behaviour |
|---|---|
| **Choose a style** | Pushes the style picker (below). |
| **Paste image URL** | Opens an inline `TextField` below the avatar preview. Validates HEAD request (4s timeout); accept morphs avatar with `motion.snappy` cross-fade. |

`Take photo` and `Choose from library` are **not in MVP**. They require a media host (Blossom or equivalent) — separate brief. Both are listed in the action sheet **disabled** with a footer *"Photo upload arrives with a future update. For now, your photo is a generated style."* — 03's honesty.

**Style picker** (04's curated 6, on a horizontal rail):

```
┌──────────────────────────────────────────────────────────┐
│  ‹ Edit profile     Choose a style                       │
│   Each style is built from your account, so the          │
│   result is always yours.                                │
│   ╭──────╮ ╭──────╮ ╭──────╮ ╭──────╮ ╭──────╮ ╭──────╮  │
│   │ ◉◉◉●│ │ ✏✏✏ │ │ 🖼🖼🖼 │ │ ▲▼◆ │ │ ◌◌◌ │ │ ░▒░ │  │
│   │Person│ │Notion│ │Lorel-│ │Shapes│ │Glass │ │Ident-│  │
│   │as(✓) │ │ist   │ │ei    │ │      │ │      │ │icon  │  │
│   ╰──────╯ ╰──────╯ ╰──────╯ ╰──────╯ ╰──────╯ ╰──────╯  │
│              ╭──────────────────────╮                    │
│              │   Use this style     │                    │
│              ╰──────────────────────╯                    │
└──────────────────────────────────────────────────────────┘
```

- Tiles: 92pt avatar + 18pt label, T1 clear glass, `Corner.lg`. Selected tile: 2pt `accent.agent` ring (the **only** place the agent gradient touches the identity surface — 04's defended exception; choosing a style is a creative act).
- **Each preview is generated with the user's pubkey-derived seed** so the user sees *their* version of each style before committing.
- Tap = preview only. **Use this style** commits and pops. (Prevents accidental "I tapped to see" mistakes.)
- The 6 styles: `personas` (current default), `notionists`, `lorelei`, `shapes`, `glass`, `identicon` (04's curation, exact list).

### 4.5 Advanced (push)

```
┌──────────────────────────────────────────────────────────┐
│  ‹ Identity              Advanced                        │
│   Most people will never need anything on this page.     │
│   It's here for people coming from other apps that       │
│   use the same kind of account.                          │
│                                                          │
│   ◌  Use my own key                                  ›   │
│      Already have an account from another app?          │
│   ◌  Sign in with a remote signer                    ›   │
│      Keep your key in a separate signing app.           │
│   ─────────────────────────────────────────────────      │
│   ◌  Account details                                 ›   │
│      Full account ID, public key formats                │
│   ◌  Start a new account                             ›   │
│      Replaces the account on this device                 │
└──────────────────────────────────────────────────────────┘
```

04's verbatim copy. Lead paragraph in `body` `text.secondary` — *the difference between feeling respected and feeling lectured at* (04's own line). Hairline divider separates the two sign-in options from the account-management options. **Order matters**: "Use my own key" first (most common reason a power user lands here), "Start a new account" last (destructive, separated).

### 4.6 Advanced → Use my own key (push)

Mechanically: existing `UserIdentityStore.importNsec(_:)`. Visually: 04's gentler primary surface.

```
┌──────────────────────────────────────────────────────────┐
│  ‹ Advanced         Use my own key                       │
│   If you already use an app like Damus, Amethyst, or     │
│   Primal, you have a private key — it usually starts     │
│   with `nsec1`. Paste it here and Podcastr will use the  │
│   same account everywhere.                               │
│                                                          │
│   Your key is stored only in this device's iOS Keychain  │
│   — the same place that holds Wi-Fi passwords. We never  │
│   see it. We never send it anywhere.                     │
│                                                          │
│   YOUR KEY                                               │
│   │ nsec1•••••••••••••••••••••••••••• Show ⎘    │       │
│   ☐  I have this key saved somewhere safe.               │
│              ╭──────────────────────╮                    │
│              │     Use this key     │                    │
│              ╰──────────────────────╯                    │
│   Don't have one? You don't need one — your existing     │
│   account works fine.                                    │
└──────────────────────────────────────────────────────────┘
```

- T0 reading body. T1 clear toolbar.
- `SecureField` with mono typography. Inline trailing **Show** (`eye` ↔ `eye.slash`, 03's pattern) and **Paste** (`doc.on.clipboard`, only enabled when clipboard string starts with `nsec1` — auto-fills on tap with `Haptics.light`).
- **Confirm checkbox** (01's call): "I have this key saved somewhere safe." — affirmative training, not warning. SF Symbol `square` ↔ `checkmark.square.fill`. Required before button enables. The double gate (non-empty field **and** checked box) is intentional friction.
- **Button**: `.glassProminent`, disabled at 0.4 opacity until both gates pass.
- **Errors** (inline footer below field, never alert): *"That key doesn't look right. Check the start (it should begin with `nsec1`) and try again."* (04's translation of the existing `loginError` copy.)
- **Success**: `Haptics.success`, pop **two** views back to Identity root. Identity reloads with imported account's profile (or npubShort + dicebear-from-pubkey placeholder until kind-0 fetch resolves). Cross-fade `motion.considered`.

### 4.7 Advanced → Sign in with a remote signer (push)

**`Nip46ConnectCard` is promoted** (03's call): drop the outer `Color(.secondarySystemBackground)` rounded rect, drop the `link.icloud.fill` glyph, drop the "NIP-46" header. Replace with `display.large` *"Remote signer"*. Flow into a `Form` body.

```
┌──────────────────────────────────────────────────────────┐
│  ‹ Advanced     Sign in with a remote signer             │
│   Some people prefer to keep their key in a separate     │
│   signing app — like Amber or nsec.app — and let other   │
│   apps ask permission to post. Podcastr supports this.   │
│                                                          │
│   Open your signer app, find "connect a new app"         │
│   (it might say "bunker"), and paste the link here.      │
│                                                          │
│   CONNECTION LINK                                        │
│   │ bunker://abc…?relay=wss://…           Paste │       │
│              ╭──────────────────────╮                    │
│              │       Connect        │                    │
│              ╰──────────────────────╯                    │
│   Your private key never touches this device — every     │
│   signature happens inside your signer app.              │
└──────────────────────────────────────────────────────────┘
```

State machine (`RemoteSignerState`) is unchanged. Surface mapping (03's table, condensed):

| State | Surface |
|---|---|
| `.idle` | Form above (or, if `isRemoteSigner`, the Connected view below). |
| `.connecting` | `Connect` row → `ProgressView` + caption *"Connecting…"*. Field read-only. |
| `.reconnecting` | Same; caption *"Reconnecting…"*. Shown on launch when resuming. |
| `.awaitingAuthorization(url)` | Above `Connect`: section header `APPROVE IN YOUR SIGNER APP`, row `Approve in browser` (`safari` glyph, `.glassProminent`, blue tint, opens URL). Below: `Cancel connection` (`.glass`, `text.secondary`). |
| `.connected(pubkey)` | Identity-root-style block: avatar + display_name + npub + mode badge `Bunker via Amber` (T2 `glass.agent` capsule). Single action: `Disconnect` (`.glass`, **not** destructive-red — 01's call: keys live elsewhere; reserve red for the actual destructive moment). |
| `.failed(message)` | Form unchanged; below `Connect`, inline footer in `error` color with truncated message (max 200 chars). Tap `Connect` again retries. |

**Auto-paste**: existing `autoPasteBunkerIfPresent` keeps working — silent, no banner.

### 4.8 Advanced → Account details (push)

Power users go here for hex, fingerprint, mode, and (eventually) relay status. MVP defers 02's audit log + per-relay RTT table.

```
┌──────────────────────────────────────────────────────────┐
│  ‹ Advanced            Account details                   │
│   PUBLIC KEY                                             │
│   npub  npub1pl7q9k4xv8…2tn5q9k4              [Copy]     │
│   hex   3f2c1a8e7b…b41a9d0e                   [Copy]     │
│   fp    sha256:7c3e…a1f2                      [Copy]     │
│   SIGNER                                                 │
│   mode    Local Key                                      │
│   source  generated · 2026-04-12 18:04 UTC               │
│   PROFILE                                                │
│   kind:0 published to 2 of 3 relays            [Republish]│
│   [ Show as QR ]                                         │
└──────────────────────────────────────────────────────────┘
```

- T0 reading. Each row: `caption.small` uppercase label + mono value + trailing copy chip.
- `Show as QR` reuses `ux-12` §6.6 cinematic morph.
- **Audit log is not in MVP.** The page has room to grow into 02's table; v2 adds the `[ Audit log ]` row at the bottom with the SignedEventLog spec.
- **Per-relay management** (02's RTT table) is also v2 — the v1 line above is a single sentence.

### 4.9 Advanced → Start a new account (alert)

04's exact copy and frame. Trigger: tap on the destructive-red row (no chevron — the row is the action).

> **Start a new account?**
>
> This will replace your current account on this device. Anything you've already posted (notes, memories, feedback, clips) stays online but you won't be able to edit it from here anymore.
>
> If you have your key saved elsewhere, you can sign back in later under Advanced.
>
> [ Cancel ] [ Start new ]

**`.alert`** not `.confirmationDialog` (existing `UserIdentityView.swift:236-237` rationale — popover-elision under iOS 26). When the user has imported an nsec or connected a bunker, the body adapts (the trailing parenthetical removes "later under Advanced" if they're on a bunker — disconnecting doesn't lose the key).

Mechanism: `clearIdentity()` → `start()` (silently regenerates). Pop to Identity root. `Haptics.medium`. No celebration.

### 4.10 First-launch beat (delta from UX-10 §S3)

UX-10 §S3 stands. **Two refinements** (01's calls):

1. The constellation resolves into the 96pt avatar; **the slug name appears** beneath the display name (3-line stagger 80ms each per ux-15 §7.3): `display.large` name, `caption` mono slug, `subhead` *"Welcome. This is you."*
2. **Remove** the *"Reveal key (advanced)"* hint from S3. It's a power-user nudge at the worst moment — when the user has just been told this is them. Power users find Advanced from Settings.

`reduceMotion`: skip the constellation, instant avatar + 3 lines, single `Haptics.light`.

---

## 5. Wiring contract — what signs, what doesn't

> **The boundary rule** (01's editorial test): *if the artifact is something a user would put their name on in a magazine masthead, sign with the user identity. If it's a thing the agent did, sign with the agent identity.*

### 5.1 Wires today (no change)

| Surface | Call site | Kind | Signs? |
|---|---|---|---|
| Auto-publish generated profile | `UserIdentityStore.swift:266` (`publishGeneratedProfileIfNeeded`) | 0 | **user** (forced `LocalKeySigner`) |
| Feedback compose | `FeedbackStore.swift:38` → `publishFeedbackNote` | 1 | **user** |
| Feedback reply | `FeedbackStore.swift:58` → `publishFeedbackNote` | 1 | **user** |
| Shake-to-feedback | `ShakeDetector` → `FeedbackComposeView` → `publishFeedbackNote` | 1 | **user** (inherits) |
| Relay AUTH (NIP-42) | `FeedbackStore.swift:188` | 22242 | **user** (`authSigner`) |

### 5.2 Will sign — **headline finding: requires schema change before wiring**

**`Note` has no author discriminator, but `addNote` is called from three sites — two user-initiated, one agent-initiated.** Without a discriminator, we cannot honour the rule "user-facing notes sign, agent-authored notes don't."

| Site | File:Line | Initiator |
|---|---|---|
| `AgentNotesView.swift:84` | `store.addNote(text:, kind: .free)` | **user** (manual New Note sheet) |
| `FriendDetailView.swift:96` | `store.addNote(text:, kind: .free, target: .friend(...))` | **user** (note about a friend) |
| `AgentTools+NotesMemory.swift:30` | `store.addNote(text:, kind:)` | **agent** (LLM tool call) |

**Required precondition for Slice B**: add `Note.author: NoteAuthor { case user, agent }`, `decodeIfPresent` default `.user`. **Pick the overload approach**: keep `addNote(...)` signature, add `addNote(..., author: .agent)` overload that the agent tool calls. User sites don't change. (Option B — splitting into `addUserNote`/`addAgentNote` — works but has wider blast radius.)

`AgentMemory` is clean — already exclusively agent-authored. **Memories never sign with the user identity.** A future "user highlight" lives in a *new* model.

`Clip` is clean — `Clip.Source` already discriminates: `.touch / .auto / .headphone / .carplay / .watch / .siri / .agent`. **Sign all sources except `.agent`.** Callsites verified: `ClipComposerSheet:204, :211` (touch), `AutoSnipController:134` (pass-through). All on signing paths.

### 5.3 Wiring table — engineer's contract

| Surface | Sign? | Kind | Tags | Site to wire | Status |
|---|---|---|---|---|---|
| Profile (kind:0) | **user** | 0 | — | `EditProfileView.save` | new in this brief |
| Feedback compose | user | 1 | `a`, `t`, `-` | wired | no change |
| Feedback reply | user | 1 | `e`, `p`, `a`, `-` | wired | no change |
| Notes (user) | **user** | 1 | `["a", episodeCoord]`, `["t", "note"]` | new `publishUserNote` on `UserIdentityStore` | **needs `Note.author`** |
| Notes (agent tool) | **don't** | n/a | local-only | `AgentTools+NotesMemory.createNote` | **needs `Note.author=.agent`** |
| Memories | **don't** | n/a | local-only | `AppStateStore+Memories` | agent state |
| Clips, source `.touch / .auto / .headphone / .carplay / .watch / .siri` | **user** | 9802 (NIP-84) | `["a", episodeCoord]`, `["context", transcript]`, `["alt", caption]` | new `publishUserClip` on `UserIdentityStore` | future |
| Clips, source `.agent` | **don't** | n/a | local-only | same | future |
| Highlights (future) | user | 9802 | as above | TBD | future |
| Comments on episodes (future) | user | 1111 (NIP-22) | `["A", showCoord]`, `["E", parentID?]` | TBD | future |
| Friend DMs (future) | user | 14/1059 (NIP-17) | per ux-12 | TBD | future |
| Agent runs / tool calls | **don't** | — | (agent identity) | `AgentIdentityStore` | separate |
| Agent chat replies | **don't** | — | (agent identity / in-process) | — | separate |
| AI-generated wikis | **don't** | — | (agent identity if published) | — | separate |
| Cached transcripts / RSS / OPML / playback positions | **don't** | — | local-only | — | local |

### 5.4 New methods on `UserIdentityStore`

Slice B adds (mirroring the existing `publishFeedbackNote` shape):

```swift
func publishProfile(name: String, displayName: String, about: String, picture: String) async throws -> SignedNostrEvent
func publishUserNote(_ note: Note, episodeCoord: String?) async throws -> SignedNostrEvent
func publishUserClip(_ clip: Clip) async throws -> SignedNostrEvent
```

All three reuse the existing `signer` and `FeedbackRelayClient.publish` machinery. None are required to ship before §4.1–§4.9 surfaces ship — Slice A reads only `publicKeyHex`, `npub`, `npubShort`, `mode`, kind-0 profile fields.

---

## 6. Implementation slices — A and B run in parallel

The merge surface between the two slices is `UserIdentityStore.swift` only. **Slice A** reads existing fields; **Slice B** adds new methods. They cannot conflict at the file level if Slice B appends and Slice A doesn't touch `UserIdentityStore.swift` at all.

### 6.1 Slice A — Surfaces (Frontend Engineer)

Owns every view file. Touches `SettingsView.swift` (insert new row) and `FeedbackView.swift` (redirect identity icon to push `IdentityRootView`). **Does not touch** `UserIdentityStore`, `Note`, `AppStateStore+Notes`, or `AgentTools+NotesMemory`.

**Created** under `App/Sources/Features/Identity/`: `IdentityRootView`, `EditProfileView`, `AvatarStylePickerView`, `AdvancedView`, `UseMyOwnKeyView`, `RemoteSignerView` (hosts the promoted `Nip46ConnectCard`), `AccountDetailsView`, `IdentitySettingsRow`, `ModeBadge`.

**Modified** (additive): `Features/Settings/SettingsView.swift` — insert `IdentitySettingsRow` as first section above Library. `Features/Feedback/FeedbackView.swift` — replace `.sheet { UserIdentityView() }` with deep-link to `IdentityRootView`. `Features/Feedback/Nip46ConnectCard.swift` — add `presentation: .card | .primary` parameter; primary mode drops outer chrome and header glyph.

**Deleted**: `Features/Feedback/UserIdentityView.swift`.

Slice A is shippable on its own — wiring is unchanged from today (Feedback signs; Notes/Clips stay local until Slice B lands).

### 6.2 Slice B — Wiring (Domain Engineer)

Owns the domain models and publish layer. **Does not touch** any view file.

**Modified**: `Domain/Note.swift` — add `author: NoteAuthor`, `decodeIfPresent` default `.user`. `State/AppStateStore+Notes.swift` — add `addNote(..., author:)` overload; call new `publishUserNote` from the user-author path. `State/AppStateStore+Clips.swift` — call `publishUserClip` for any source ≠ `.agent`. `Agent/AgentTools+NotesMemory.swift:30` — pass `author: .agent`. `Services/UserIdentityStore.swift` — append `publishProfile`, `publishUserNote`, `publishUserClip` (all `MainActor`, mirroring `publishFeedbackNote`).

**Created**: `Domain/NoteAuthor.swift`; `AppTests/Sources/UserIdentityWiringTests.swift` — table-driven tests covering every row of §5.3.

Slice B is shippable on its own through the existing `UserIdentityView`.

### 6.3 Sequencing

Both slices land to `main` independently. The integration moment is `EditProfileView.save` calling `identity.publishProfile` — Slice A may stub with the existing `publishGeneratedProfileIfNeeded` machinery until Slice B merges. **Recommend Slice A first** (visible delivery), Slice B second (architectural cleanup) — but the order is commutative.

---

## 7. Glass tier discipline (consolidated)

Per ux-15. Every surface in this brief tier-classified:

| Surface | Tier | Tint | Notes |
|---|---|---|---|
| Settings → Identity row | T1 | clear | One quiet lift; avatar hairline ring carries state |
| IdentityRootView body | T0 | paper | Reading surface — glass would distract |
| IdentityRootView mode badge | T2 | clear (Local) / `glass.agent` (Bunker) | The defended `glass.agent` exception (02's call) |
| Edit profile button | T3 | onyx | `.glassProminent` — the only T3 fill on the root |
| Avatar (root, edit, picker) | T0 | hairline ring | No breathing, no rotation — calm |
| EditProfileView | T0 + T1 toolbar | paper / clear | Form fields underline-only |
| Style picker tiles | T1 | clear | Selected ring: 2pt `accent.agent` (defended) |
| Advanced page | T0 + T1 toolbar | paper | List body |
| Use my own key | T0 + T1 toolbar | paper | Confirm checkbox is shape-distinguished |
| Remote signer | T0 + T1 toolbar | paper; `glass.agent` on auth-challenge inline row | Promoted from `Nip46ConnectCard` |
| Account details | T0 + T1 toolbar | paper | Each row mono value + copy chip |
| QR sheet | per `ux-12` §6.6 | — | Single source of truth — do not redesign |
| Sign-out alert | system `.alert` | — | Per existing `UserIdentityView.swift:236-237` |

**Forbidden everywhere on Identity surfaces**: `accent.player` (copper — reserved for now-playing per ux-15 §9.2), `accent.friend` (amber — reserved for friend provenance per ux-15 §9.3), agent gradient as a *fill* (only as the 2pt selected-style ring), T4 cinematic (this is not a hero surface; the QR sheet is the only morph and it lives in `ux-12`).

---

## 8. Accessibility (delta from siblings — they're consistent)

- **Mode badge** is shape + label, not color: `Local Key` is plain text, `Bunker via X` adds a leading `link.icloud` glyph. Color is additive.
- **Style picker selection** is ring **and** label state ("(current)"), not ring alone.
- **Account ID copy success** uses `UIAccessibility.post(notification: .announcement, argument: "Copied")` (03's pattern) plus glyph + label change (01's pattern).
- **Settings row VoiceOver**: combines as one element — *"Identity. Bright Signal. Local Key. n-pub-1-p-l-7-…-q-9-k-4. Button."*
- **Reduce Motion**: avatar swap is instant; `motion.considered` cross-fades become opacity transitions; first-launch constellation skips per UX-10 §S3 Reduce Motion path.
- **Dynamic Type**: every typography token from ux-15 §3 ramp; 96pt avatar fixed (it's a portrait, not text); display name wraps to two lines at AX5 with the Edit button anchored to *bottom-of-name + 32pt*, not a fixed offset.

---

## 9. Open questions surfaced by synthesis

1. **Username uniqueness** (03's Q1). MVP accepts collisions silently; slug suffix differentiates. Defer relay-side check.
2. **Profile sync receipt** (03's Q2). MVP: no "Saving…/Synced." Failure inline; success silent.
3. **Photo upload host** (03 footer, 04 Q3). MVP ships **without** PhotosPicker. Curated styles + paste URL only. Blossom is its own brief.
4. **Audit log** (02 §6). v2. §4.8 is sized to grow into it.
5. **Multi-account** (02 §4.6). v2. Keychain layout migration: `user-private-key-hex.{fingerprint}` + `active-fingerprint` pointer. Engineer brief follow-up.
6. **NIP-65 outbox / kind:10002** (02 Q4). v2. MVP uses hard-coded `FeedbackRelayClient.profileRelayURLs`.
7. **Lost-key recovery** (03 Q4). "Start a new account" alert mentions it once. No dedicated backup surface in MVP.

---

## 10. Commits and rejections

**Commits to:** Identity at the top of Settings, named "Identity"; the npub block named "Account ID" with 04's plain-English explainer; **the 02 mode badge as a permanent fixture wherever the npub appears**; 01's editorial pull-quote for `about`; 04's six curated dicebear styles + 01's paste-URL as the avatar paths in MVP; 03's push-not-sheet primary surfaces for nsec and remote signer; 03's promotion of `Nip46ConnectCard` to a primary-page treatment; 04's "Start a new account" framing buried in Advanced (not "Sign Out" at the root); UX-10 §S3 first-launch with 01's slug surfacing and the `Reveal key (advanced)` hint removed; the wiring contract built on top of a new `Note.author` discriminator (the only schema change).

**Rejects:** PhotosPicker without a media host (03 itself disabled it; 04 hand-waved); audit log + multi-account in MVP (02's strongest ideas, deferred to v2 with their infra); silent first-launch (03/04's underweighting of the earned beat); "Profile" as the surface name (04's call — the keypair reality matters too much); copper / amber / fill-tier agent gradient on the Identity surface (each reserved per ux-15).

If a Nostr-naive user opens Settings, taps Identity, edits their About line, picks a style, and never returns — the app has done its work. If a power user lands on the same row, taps through to Advanced → Account details, sees mode + npub + hex + fingerprint + relay status, and copies the bunker URI — the same surface served them. **One Identity. One contract. Two slices that ship in parallel.**
