# Identity-01 — Minimal / Editorial

> One of five parallel proposals for the User Identity surface. This one is built around the design philosophy *"calm by default, alive on demand"* — typography-first, generous whitespace, Liquid Glass in restraint. **No Nostr jargon up front.** A Nostr-naive user should be able to live in this surface for a year and never need to learn the word.
>
> Authority: visual rules in [docs/spec/briefs/ux-15-liquid-glass-system.md](ux-15-liquid-glass-system.md). Onboarding adjacency in [docs/spec/briefs/ux-10-onboarding.md](ux-10-onboarding.md) §S3. The existing `UserIdentityView` (currently scoped to Feedback) is **replaced** by the surface specified here; Feedback deep-links into it.

---

## 1. Stance

The user already has an identity — we made one for them at first launch. They didn't ask for it, but they have one, and it's real: a real keypair, a real kind-0 profile published to relays, a real avatar, a real name. This brief's whole job is to **make that fact feel ordinary**. Not magical. Not "secured." Not "wallet-grade." Ordinary, like the name on a library card.

Three operating rules:

1. **Editorial, not chrome.** The Identity root reads like the front matter of a periodical — a portrait, a name, a single line of metadata, abundant air. Glass is structural and quiet (T0 paper for body, T1 for chrome only). Exactly **one** T3 surface earns its weight: the Edit affordance.
2. **Power paths are footer-quiet.** "Sign in with an existing key" is a `caption` link at the bottom of the page, not a button. Bunker connect is one tap deeper. Power users find them; everyone else never sees them.
3. **No jargon above the fold.** "Identity," "Edit profile," "Avatar," "About." The words *Nostr*, *npub*, *nsec*, *NIP-46* appear only in advanced sub-sheets, and even there they're contextualized.

If a choice doesn't defend "calm by default, alive on demand," it isn't here.

---

## 2. Settings → Identity entry

The Identity row sits at the **top** of `SettingsView` — above Library — because the masthead precedes the table of contents. It answers *who am I in this app?* before any other setting matters.

```
┌──────────────────────────────────────────────────────────┐
│  Settings                                                │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │  ╭────╮   Bright Signal                       ›   │  │
│  │  │ ◐◑ │   bright-signal-a3f2                       │  │
│  │  ╰────╯                                             │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  Library                                                 │
│  ◍  Subscriptions                            47    ›    │
│  ⊞  Categories                               12    ›    │
│   …                                                      │
└──────────────────────────────────────────────────────────┘
```

**Spec:**

- **Tier**: T0 paper (a `Section` row inside the standard `List`). No tinted glass — this is part of the chrome of Settings, not a hero.
- **Layout**: 44pt avatar + 12pt gutter + two-line text stack + chevron. Total row height **64pt** (16pt taller than a standard `SettingsRow` to give the avatar breathing room — every other row stays 44pt).
- **Avatar**: the user's current `picture` field, rendered at 44pt with a 1pt `hairline` ring (light) / `rgba(255,255,255,.12)` (dark). Falls back to dicebear seed deterministic from pubkey if `picture` fails to load. Never a placeholder silhouette.
- **Top line**: `display_name` in `headline` (SF Pro Rounded Semibold 17/22) — the warm register, per `ux-15` §3, because this is *the user as voice*. Falls back to `name` if `display_name` is empty.
- **Second line**: `name` (the slug) in `caption` SF Mono Medium, `text.tertiary`. The slug is *deliberately visible here*, not the npub. The npub belongs to the detail view.
- **No status badge**, no "Signed in" chip. Of course they're signed in — they're using the app.
- **Tap**: pushes the Identity root view (`motion.standard`).

This row is the *only* place in the app where the user's display name and avatar appear together as chrome. (Notes, memories, and feedback show this same pair as the byline of authored content — see §8.)

---

## 3. Identity root

The destination behind that row. **A page, not a card stack.** The current `UserIdentityView` builds three concentric grey cards on a grey background; this view does the opposite — content on paper, one quiet seam.

```
┌──────────────────────────────────────────────────────────┐
│   ‹ Settings              Identity                       │
│                                                          │
│                                                          │
│                       ╭───────────╮                      │
│                       │   ◐◑◐◑    │                      │
│                       │   ◑◐◑◐    │   ← 96pt avatar      │
│                       │   ◐◑◐◑    │     T0, 1pt hairline │
│                       ╰───────────╯                      │
│                                                          │
│                                                          │
│                    Bright Signal                         │
│                  bright-signal-a3f2                      │
│                                                          │
│                                                          │
│              ╭──────────────────────╮                    │
│              │       Edit           │   ← T3, only       │
│              ╰──────────────────────╯     glass on page  │
│                                                          │
│                                                          │
│   ─────────────────────────────────────────────────      │
│                                                          │
│   ABOUT                                                  │
│                                                          │
│      "                                                   │
│      A new account, freshly minted.                      │
│      Tell people who you are.                            │
│                                                          │
│                                                          │
│   ─────────────────────────────────────────────────      │
│                                                          │
│   PUBLIC ADDRESS                                         │
│                                                          │
│   npub1xq…7q9                              [copy]        │
│                                                          │
│   This is how others recognize you across                │
│   apps that use Nostr. It's safe to share.               │
│                                                          │
│                                                          │
│                                                          │
│                                                          │
│                                                          │
│   Already have a key? Sign in with an existing one.      │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

**Spec:**

- **Background**: `bg.canvas` (warm-paper / near-black ink). **No card.** The whole page is one paper plane. Per `ux-15` §5: "Pure reading surfaces — glass would distract."
- **Avatar**: 96pt circle, 1pt `hairline` ring, T0. **No breathing animation, no glass tint.** It is a portrait, not a status light. The first launch is the only moment it animates (§7).
- **Display name**: `display.large` (New York Medium 28/32, tracking -0.3). Centered. This is the only New York treatment on the page besides the about pull-quote — and that's the point.
- **Name slug**: `caption` SF Mono Medium 13/17 in `text.tertiary`. Centered, 4pt below the display name. The slug is the user's first encounter with their identifier; treating it as small caps mono lets it feel *typeset*, not technical.
- **Edit button**: T3 interactive glass, `.glass` style (clear tint, not agent gradient — agent gradient is reserved per `ux-15` §6.1), `headline` label, 14h / 22v / 14r. **Width 220pt, centered.** This is the only T3 element on the page; everything else is paper. The contrast — paper plane with one glass affordance floating in the middle — is the whole composition.
- **Section dividers**: `divider.bold` hairlines with section-cap labels in `caption.small` SF Pro Medium +0.2 tracking, uppercase, `text.tertiary`. *Exactly* the editorial register the brand uses for "ABOUT" and "PUBLIC ADDRESS." Two sections — no more.
- **About**: see §3.1.
- **Public address**: `mono.timestamp` SF Mono Medium 13/17 truncated to `npub1xq…7q9`. A `[copy]` glyph button trailing (24pt tap target padded to 44pt). Below: *one explanatory sentence*, `subhead` (15/20), `text.secondary`. **The word "Nostr" appears here for the first time** — and exactly once.
- **Footer link**: `caption` (13/17) in `text.tertiary`, centered, 32pt above the safe-area bottom. "Already have a key? **Sign in with an existing one.**" The bolded clause is the link. No icon. No box. This is the doorway to the power-user paths (§5, §6) and it is *visually quieter than a tab bar label*. Power users find it because they're looking; everyone else's eye glides past.

**What is deliberately absent:**

- No "Sign out" button. (Sign-out lives inside the Edit sheet, three taps away — see §4.5.)
- No relay status, no connection chips, no QR code, no kind-0 publish indicator.
- No `accentPlayer` copper anywhere. Copper is reserved (`ux-15` §9.2). Identity is not the player.
- No agent gradient. Identity is *the user*, not the agent.
- No badge, no avatar-corner camera glyph. The Edit affordance is the editor.

### 3.1 The about pull-quote

This is the load-bearing typographic moment of the page.

```
   ABOUT
   ───────────────────────────────────────────────

      "
      A new account, freshly minted.
      Tell people who you are.
```

**Spec:**

- Typography: **New York Medium 19/26**, tracking 0. (Per `ux-15` §3, New York requires ≥19pt.) Light italic when the user has not yet edited (placeholder state).
- A **floating oversized opening quote** glyph: New York Regular **40pt**, color `text.tertiary`, baseline-shifted up 8pt and indented -4pt outside the text block's leading edge. This is the editorial signature.
- Hanging indent: body text begins **24pt** in from the page's content rule. No closing quote. (Closing quotes feel possessive; we want the line to keep breathing.)
- **No glass behind the quote.** T0 paper. Glass on a reading surface is a §5 sin in `ux-15`.
- Empty-state copy is one second-person line — *"Write a line for people who find your posts."* When the user has written an `about`, that prose replaces the placeholder — same typography, same opening quote, italic dropped.
- Maximum measure: **62 characters** on phone (matches the wiki body in `ux-15` §5.4 — same reading register). Long abouts wrap; we never crop.

This is the only element on the page that can grow with the user. Everything else is fixed.

---

## 4. Profile editor

Reached via the single Edit button. **A medium-detent sheet**, not a full-screen modal — staying out of full-screen reinforces that this is a small, contained edit, not a configuration ritual.

```
┌──────────────────────────────────────────────────────────┐
│         ─── (drag handle, 36×4)                          │
│                                                          │
│   Cancel                Edit                       Save  │
│                                                          │
│                                                          │
│                       ╭───────────╮                      │
│                       │   ◐◑◐◑    │                      │
│                       │   ◑◐◑◐    │   ← 96pt avatar      │
│                       │   ◐◑◐◑    │     live preview     │
│                       ╰───────────╯                      │
│                                                          │
│              [ Shuffle ]    [ Paste URL ]                │
│                                                          │
│                                                          │
│   ─────────────────────────────────────────────────      │
│                                                          │
│   DISPLAY NAME                                           │
│   Bright Signal                                          │
│                                                          │
│   USERNAME                                               │
│   bright-signal-a3f2                                     │
│                                                          │
│   ─────────────────────────────────────────────────      │
│                                                          │
│   ABOUT                                                  │
│      "                                                   │
│      ▌                                                   │
│      A note for people who find your posts.              │
│                                                          │
│   ─────────────────────────────────────────────────      │
│                                                          │
│   Sign out of this account                               │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

**Spec:**

- **Sheet**: T1 clear glass, top corners `Corner.xl` (24pt), detents `[.medium, .large]`, drag handle 36×4 per `ux-15` §6.3.
- **Toolbar**: standard SwiftUI `.principal` title `headline`, leading `Cancel`, trailing `Save`. **Save is `.glassProminent`** when changes exist; ghosts to a plain `.glass` button when the form matches the persisted state. (Save spinning state replaces the label with `Saving…` and a 14pt circular progress; sheet does not auto-dismiss until the kind-0 publish broadcast returns success from at least one relay — at which point haptic `success` fires and the sheet dismisses on `motion.considered`.)

### 4.1 Avatar editor — two affordances, no upload

The minimal-editorial answer to "how do you change your picture" is: **you don't *take* one — you choose one or you paste one.**

- **Avatar preview**: 96pt circle, T0, **identical** to the Identity root. Live-bound to the staged `picture` value. When the user hits Shuffle, the avatar morphs (`motion.snappy` cross-fade 220ms) to the new image.
- **Shuffle**: `.glass` style chip, `caption` SF Pro Medium, 12h / 8v / pill. Reseeds the dicebear URL with a new random seed (4-char hex appended to pubkey prefix, like the existing generator). On tap: `Haptics.light`, fade-swap.
- **Paste URL**: `.glass` style chip, identical proportions to Shuffle. On tap: opens an inline `TextField` *below* the avatar preview, pre-filled from clipboard if it contains an `https://` URL. Submitting the field validates the URL (HEAD request, 4s timeout) and either accepts (avatar morphs to the new image) or shows a single-line error in `error` color: *"That URL didn't work — try another."* No Cancel needed; pasting is non-destructive until Save.

**No camera. No photo picker. No upload pipeline.** Three reasons:

1. Hosting is out of scope for the minimal proposal. The other four proposals can argue for blob upload; this one says *the avatar is content the user references, not content the user uploads*.
2. Dicebear's procedural avatars feel personal *enough* — they shuffle to feel chosen, not assigned.
3. Power users with their own image hosting paste a URL. Everyone else lives on Shuffle. Both paths are first-class.

### 4.2 Display name + username

Two stacked `TextField` rows, each labelled in `caption.small` SF Pro Medium uppercase `text.tertiary` ("DISPLAY NAME", "USERNAME"). Field text is `body` (17/24) SF Pro. No background fill — the field rests directly on paper, with a 0.5pt `hairline` rule beneath that thickens to `text.primary` on focus (`motion.snappy`). Per `ux-15` — paper above, paper below, the underline does the work.

Validation:

- **Display name**: 0–48 characters. Empty is allowed; the Identity root falls back to `name`. Trimmed on save.
- **Username**: 1–32 characters, unicode allowed (we never enforce ASCII slugs — that's chauvinism). Empty disallowed; if cleared, restore the previous value on blur with a `Haptics.light`.

These fields publish `display_name` and `name` to the kind-0 event respectively. No mention of "kind-0" anywhere in the UI.

### 4.3 About — the editor

The exact same typography as §3.1 — New York Medium 19/26, oversized opening quote, hanging indent. This is a `TextEditor`, not a `TextField`. **Multi-line is the point.**

- Placeholder when empty: in `text.tertiary`, italic — *"A note for people who find your posts."* Disappears on focus.
- Maximum: 280 characters. No counter visible until the user crosses 240 — at 240 a `caption` counter appears in `text.tertiary` (`240/280`), at 270 it shifts to `warning`, at 280 it freezes input and shows `error`.
- The opening-quote glyph is *part of the editor's chrome*, not part of the editable text. It's positioned the same way as in the read view; the user types *into* the quote.

This is the only field on the page that can become distinctive. Most users will leave the display name as-is and never touch the avatar. The about field is where the editorial register pays off — they're writing copy for themselves.

### 4.4 Save semantics

Save is *one* button publishing *one* kind-0 event. There's no separate "Update profile" / "Update avatar" / "Update bio." The mental model is: I edited my page; I'm done.

On save:

1. Compose new kind-0 JSON (`name`, `display_name`, `about`, `picture`).
2. Sign with current signer (local key or remote signer — invisible to the user).
3. Broadcast to `FeedbackRelayClient.profileRelayURLs`.
4. On first relay ack: `Haptics.success`, sheet dismisses (`motion.considered`), Identity root cross-fades to new values (`motion.standard`, 350ms).
5. On total failure (no relay accepted within 8s): inline banner T1 above the toolbar — `warning` tint — *"Saved on this device. We'll try again when you're online."* Sheet stays open until the user hits Cancel; the local edit persists either way.
6. **Signer-stall path (NIP-46 only):** if the remote signer takes longer than 8s to return the signature, the Save state shows `subhead` *"Waiting for your signer…"* with a Cancel affordance that aborts the publish and leaves the form dirty. Same editorial register as the relay-failure banner; never a spinner alone.

### 4.5 Sign out — buried, deliberate

A single `.pressable` text row at the bottom of the sheet content (above the safe-area), `body` SF Pro Regular, color `error`. **No icon, no box, no "destructive button" red bar.**

```
   Sign out of this account
```

Tap → confirmation alert (not action sheet, per the existing comment in `UserIdentityView.swift`):

> **Sign out?**
>
> If you don't have your key saved somewhere, this account is gone forever. We can't recover it.
>
> [ Cancel ] [ Sign out ]

Sign out is destructive (existing `clearIdentity()` deletes the keychain slot). The copy makes the irreversibility plain *without* using the word "private key" — that's a §5 word, not a §3 word. Power users who connected via NIP-46 see the same row but with the alert copy *"Disconnect your remote signer? Your account stays where it is — this just unlinks it from this device."* Same row, different consequence, copy adapts to mode.

---

## 5. nsec import — the existing-key path

Reached only from the Identity root footer link "Already have a key? **Sign in with an existing one.**" Presented as a **medium-detent sheet** over the Identity root.

```
┌──────────────────────────────────────────────────────────┐
│         ─── (drag handle)                                │
│                                                          │
│   Cancel        Use an existing key                      │
│                                                          │
│                                                          │
│   Paste your private key.                                │
│                                                          │
│   ┌──────────────────────────────────────────────────┐   │
│   │  nsec1•••••••••••••••••••••••••••••••••••       │   │
│   └──────────────────────────────────────────────────┘   │
│                                                          │
│                                                          │
│   ☐  I have this key saved somewhere safe.               │
│                                                          │
│                                                          │
│   ─────────────────────────────────────────────────      │
│                                                          │
│   Your private key never leaves this device.             │
│   It's stored in the iOS Keychain — the same place       │
│   that holds Wi-Fi passwords and Apple Pay cards.        │
│                                                          │
│   Switching keys replaces this account.                  │
│   Your current account (bright-signal-a3f2) won't be     │
│   posted from this device anymore.                       │
│                                                          │
│                                                          │
│              ╭──────────────────────╮                    │
│              │     Use this key     │   ← .glassProminent│
│              ╰──────────────────────╯     disabled until │
│                                            both filled   │
└──────────────────────────────────────────────────────────┘
```

**Spec:**

- **Sheet tier**: T1 clear glass, top corners `Corner.xl`. Body content is paper (T0).
- **Field**: `SecureField` with `mono.timestamp` typography (the dots are the same width — pleasing). Paper background, single 0.5pt `hairline` rule. Auto-paste from clipboard *only if* clipboard string starts with `nsec1` and field is empty (mirrors the existing bunker auto-paste pattern). Auto-paste fires `Haptics.light` and shows a 1.4s caption beneath: *"Pasted from clipboard."*
- **Confirm checkbox**: an iOS native `Toggle` styled as a checkbox (leading 22pt SF Symbol `square` / `checkmark.square.fill`). Required before the action button enables. The label uses `subhead` register, not legalese. *"I have this key saved somewhere safe."* — affirmative, not warning. (Trains the user to back keys up.)
- **Use this key button**: `.glassProminent`, 220pt centered, 32pt below the body. **Disabled (alpha 0.4)** until field is non-empty *and* checkbox is checked. The double gate is intentional friction.
- **Body copy**: `body` (17/24), SF Pro, `text.secondary`. Two paragraphs. **Words used:** "private key" (yes — the user typed it), "iOS Keychain" (concrete, comparable), "Wi-Fi passwords" (Apple's own analogue). **Words avoided:** "nsec" (the field hint and `nsec1•••` already telegraph the format), "Nostr" (already established in §3), "delegate," "ephemeral," "compromise."
- **The acknowledgment paragraph** ("Switching keys replaces this account…") names the user's current slug — *"bright-signal-a3f2"* — explicitly, in mono. Naming what they're losing makes the trade real. Per `ux-15` §3 mono register, this is editorial in the same way newsroom corrections name the article.

**Errors:**

- Invalid nsec → inline `caption` `error`-color line below the field: *"That doesn't look like a private key. It should start with `nsec1`."* Replaces the existing "Invalid nsec — check the key and try again" copy with something a Nostr-naive user can act on.
- Network failure during the post-import kind-0 fetch → silent fallback to default profile (handled by existing machinery). No surfaced error.

**Success:**

- `Haptics.success`, sheet dismisses, Identity root reloads with the imported account's profile (or, if no kind-0 has been published yet, with the npub-shorthand pattern as display name and a placeholder dicebear seeded from the new pubkey). The morph from previous-identity to new-identity uses a cross-fade `motion.considered` — never a sliding transition. **The portrait does not animate as if it's a new person; it just becomes a new person.** This is the right register for an authentication change.

---

## 6. NIP-46 — connect to a remote signer

**Reuse the existing `Nip46ConnectCard`**, presented as a **second medium-detent sheet** reachable from inside the nsec import sheet.

The reasoning: NIP-46 is a power-user-of-power-users feature. The user who pastes an nsec is already advanced; the user who pastes a `bunker://` URI has chosen security over convenience. Giving NIP-46 its own footer entry on the Identity root would dilute the "calm by default" stance — adding an *enterprise auth* entry to a portrait page is exactly what a periodical wouldn't do.

### 6.1 Entry point

At the bottom of the nsec import sheet body, above the action button:

```
   ─────────────────────────────────────────────────

   Or: connect to a remote signer instead.
```

`caption` SF Pro Regular, `text.tertiary`, the second sentence is the link. Tap → second sheet pushes (sheet-on-sheet, both medium detent). The phrase "remote signer" is the only place this term appears outside of the connect card itself.

### 6.2 The card itself

The existing `Nip46ConnectCard` keeps its shape — same input row, same auth-challenge handling, same connected/failed/reconnecting states. **Minor visual reductions** for editorial fit:

- Drop the "NIP-46 Remote Signer" header label. Replace with `display.large` New York title *"Remote signer"* — single line, no glyph.
- Drop the `link.icloud.fill` glyph from the header.
- The input field gets the same hairline-only treatment as nsec (no background fill).
- The "Connecting to bunker…" rows reuse `subhead` typography in `text.secondary` — strip the `link` SF Symbol from "Connect bunker"; the button label alone is sufficient.
- Error states: same as today, but the body copy gets the same translation pass — *"Your remote signer didn't accept the connection."* not *"failed: timeout"*.

The connect card retains all power-user concrete details: the `bunker://…?relay=…&secret=…` placeholder stays (a power user reading this needs to see the format), the auth-challenge "Approve in browser" button stays, the connected-state npub display stays.

### 6.3 Connected state lift

When connected, the second sheet's body collapses to:

```
   Connected to your remote signer.

   Signing as
   npub1abcd…7q9                              [copy]

   This device doesn't have your private key.
   Every signature is approved by your signer.

              ╭──────────────────────╮
              │      Disconnect      │
              ╰──────────────────────╯
```

`body` typography, paper. The Disconnect button is `.glass` style, **not** destructive-red. (The existing card uses red.) Disconnect from a remote signer isn't destructive — the keys live elsewhere. Reserve red for the actual destructive moment (sign-out / clear local key).

---

## 7. First launch — what does the user see?

### 7.1 Position relative to UX-10

`ux-10-onboarding.md` §S3 specifies a dedicated Identity step: a constellation animation with the line *"Identity created."* and a faint *"Reveal key (advanced)"* hint. That step is **kept**, with one minimal-editorial refinement: the slug-name appears.

The constellation settles. The dots resolve into a 96pt avatar — the same dicebear that will show forever. Below the portrait, three lines fade in over 600ms, staggered 80ms each (per `ux-15` §7.3 stagger rule):

```
                     ╭───────────╮
                     │   ◐◑◐◑    │
                     │   ◑◐◑◐    │
                     ╰───────────╯


                  Bright Signal
                bright-signal-a3f2

              Welcome. This is you.
```

- Line 1: `display.large` New York Medium 28/32. The display name.
- Line 2: `caption` SF Mono Medium. The slug.
- Line 3: `subhead` SF Pro Regular 15/20, `text.secondary`. *"Welcome. This is you."* — a single editorial line. Not a tutorial. Not "you can change this in Settings." The user is being introduced to a person, and that person is them.

The "Reveal key (advanced)" link from UX-10 is **removed** in this proposal. It's a power-user hint at the worst possible moment: when the user has just been told this is them. If they want to reveal it, the Identity row is a settings tap away. The S3 step is a portrait, not a key reveal.

### 7.2 Reduce Motion

When `accessibilityShouldDifferentiateWithoutColor` is irrelevant here (no color-only signals); when `reduceMotion` is on: the constellation skips. The avatar appears instantly with the three lines (no stagger, no fade), `Haptics.light` once. The whole step is 1.4s instead of 3.2s, which is the right answer for users who turn motion off — they don't want the cinematic.

### 7.3 What is *not* shown at first launch

- No "we generated a Nostr key for you" toast.
- No prompt to back up the key.
- No relay status, no "publishing your profile…" indicator (the publish runs invisibly via `publishGeneratedProfileIfNeeded`; failures are silent and retried at the next launch).
- No prompt to choose a name. Choosing your name is an *opt-in* moment, reachable from Settings → Identity → Edit. Nostr-naive users who never visit Settings will live as Bright Signal forever, and that is **fine**.

The whole point of the minimal stance: the user is given an identity the way they're given a default ringtone. They can change it. Most won't. Both are okay.

---

## 8. Wiring contract — what signs with this identity, and what doesn't

This is the contract the engineer will implement against. **The Settings → Identity surface specified above replaces `App/Sources/Features/Feedback/UserIdentityView.swift`**; the Feedback flow links into Settings → Identity rather than presenting its own sheet.

| Surface                                  | Signs with user identity? | Event kind / coordinate                    | Status today               | Notes                                                                                                       |
|------------------------------------------|---------------------------|--------------------------------------------|----------------------------|-------------------------------------------------------------------------------------------------------------|
| **Profile metadata** (this brief)        | Yes                       | kind 0                                     | Auto-publish exists        | Edit sheet republishes on every Save. Body = `{name, display_name, about, picture}`.                        |
| **Feedback threads** (FeedbackComposeView) | Yes                     | kind 1, `a` tag = project coordinate, `t` tag = category, `-` tag (relay-private) | **Wired** (see `FeedbackStore.publishThread`) | No change. Already routes through `UserIdentityStore.publishFeedbackNote`.                                  |
| **Feedback replies** (FeedbackThreadDetailView) | Yes                | kind 1, `e`/`p` reply tags + project coord | **Wired**                  | No change.                                                                                                  |
| **Notes** (per-episode)                  | Yes — to be wired         | kind 1 with episode-coordinate `a` tag     | **Local-only today**       | This brief doesn't design the note format; it just *names that they should sign*. Engineer brief follow-up. |
| **Memories** (highlights from agent-mediated moments) | Yes — to be wired | kind 1 (or NIP-yet-to-decide) with episode + timestamp tags | **Local-only today** | Same as notes — sign once persistence migrates from local to relay-published.                              |
| **Highlights / clips** (future)          | Yes                       | NIP-84 highlight (kind 9802) with episode + transcript-range tags | **Not yet built**          | Specified in `ux-12-nostr-communication.md` §3 share-clip flow. Will sign through this identity.           |
| **Comments on episodes/clips** (future)  | Yes                       | kind 1 reply chain or NIP-22 generic comment | **Not yet built**          | Same identity as feedback — single signer for everything user-authored.                                     |
| **Friend DMs / friend-agent commands**   | Yes                       | NIP-17 (kind 14 / 1059)                    | Friends spec in `ux-12`    | The friend-agent path in `ux-12` §6 explicitly uses *this* identity for self-DMs and outgoing.              |
| **Agent chat messages** (user → agent)   | **No** (in-process IPC)   | n/a                                        | n/a                        | Local turn-of-conversation, not a signed event. If/when agent moves to Nostr DM transport, see friend DMs. |
| **Agent replies / tool calls / briefings** | **No** — agent identity   | (separate keypair)                         | Agent identity is separate | Per `UserIdentityStore` doc comment. Agent-authored content is never user-signed and never carries the user's avatar. |
| **AI-generated wikis**                   | **No** — agent identity   | (separate keypair)                         | n/a                        | Wiki citations show the agent gradient, not the user's portrait.                                            |
| **Playback positions / library state**   | **No**                    | n/a                                        | n/a                        | Local. (iCloud-synced via `iCloudSettingsSync`. Never published.)                                           |

**The boundary rule** the engineer should enforce in code review: *if the artifact is something a user would put their name on in a magazine masthead, it signs with the user identity. If it's a thing the agent did, it signs with the agent's identity.* Notes and memories pass that test even though they're not yet wired; agent replies fail it even though the user invoked them.

**Feedback flow consequence:** with auto-generation, the "no identity" branch in `FeedbackComposeView` never triggers — every user always has a signer at compose time. Any "view your identity" affordance in the Feedback flow (e.g. the byline avatar in `FeedbackThreadDetailView`) deep-links via `NavigationLink` to Settings → Identity rather than sheeting a duplicate view. The existing in-Feedback `UserIdentityView` is removed.

---

## 9. Accessibility specifics

Honoring the constraints from the task and `ux-15` §6.6:

### 9.1 Differentiate without color

- **Copy state on the npub copy button**: success state is *not* `success` green alone. The button transitions from `[copy]` glyph + label to a **`checkmark` glyph + the word "Copied"** in the same color, with a brief 1.4s persistence. Per `ux-15` §6.1 the existing `UserIdentityView.swift:119` already pairs glyph+label; this brief preserves that pairing.
- **Save button enabled vs disabled**: enabled = `.glassProminent` with full opacity *and* the label "Save"; disabled = same style at 0.4 opacity *and* label is unchanged but accessibility trait `.notEnabled` is set, so VoiceOver reads "Save, dimmed."
- **nsec checkbox**: the toggle is shape-distinguished (empty `square` SF Symbol vs filled `checkmark.square.fill`). No color-only state.
- **Field focus**: the underline thickening from 0.5pt → 1pt is *thickness*, not just color shift. (The color also darkens but the thickness carries the signal.)

### 9.2 Reduce motion

- **First-launch S3 constellation**: skipped per §7.2.
- **Avatar shuffle morph**: cross-fade replaced with instant swap.
- **Save success transition**: the `motion.considered` cross-fade from previous to new profile values becomes an instant swap, with `Haptics.success` carrying the feedback that the visual transition would have.
- **Edit sheet present/dismiss**: the system-default sheet motion already respects `reduceMotion`; no overrides.
- **The Identity root has no ambient motion** to begin with. The avatar does not breathe, the page does not parallax. Per `ux-15` §9 the breathing treatment is reserved for the agent orb. This page is calm by default — `reduceMotion` finds nothing to reduce, which is the right answer for a user-portrait page.

### 9.3 Dynamic Type

- Every typography token is from the `ux-15` §3 ramp; all scale.
- The 96pt avatar is a fixed dimension (it's a portrait, not text), but the `display.large` name beneath grows. At AX5, the name can wrap to two lines — the layout absorbs this; the Edit button's vertical position is anchored to *the bottom of the name stack + 32pt*, not to a fixed offset.
- The about pull-quote's 24pt hanging indent is a fixed visual offset, not a ratio of text size. At AX5 it remains 24pt — the indent serves typesetting, not text-relative spacing.

### 9.4 VoiceOver

- The Settings → Identity row reads as one element: *"Identity. Bright Signal. bright-signal-a3f2. Button."* (Two lines collapse to one announcement.)
- The Identity root portrait + name + slug stack reads as one heading: *"Bright Signal. bright-signal-a3f2. Heading."* — `accessibilityElement(children: .combine)` with `.heading` trait on the parent.
- The npub line reads as: *"Public address. n-pub-1-x-q-…-7-q-9. Double tap to copy."* — the dots are spoken as "dot dot dot," **not** "ellipsis."
- The footer link reads as a link: *"Already have a key? Sign in with an existing one. Link."*

---

## 10. Commits and rejections

**Commits to:** a single paper-dominant page with one T3 affordance and one editorial pull-quote; the slug name as a typeset feature; two avatar paths (Shuffle, Paste URL) and no upload pipeline; footer-quiet power paths (nsec is a link, NIP-46 is a link inside the import sheet); reuse of `Nip46ConnectCard` with typography reductions; first-launch handled by UX-10 §S3, refined to surface the slug and remove the key-reveal hint; a wiring contract that distinguishes user-authored from agent-authored by editorial test.

**Rejects:** three-card layouts (the current `UserIdentityView` is the wrong register); status badges, "Connected" chips, kind-0 publish progress, relay counts; camera / photo picker for the avatar; surfacing "Nostr" / "npub" / "nsec" as primary chrome (each appears once, in its right place); copper or agent-gradient anywhere on this surface (both reserved per `ux-15` §9); any motion the page doesn't earn — the portrait is calm, power paths are quiet, the agent is alive somewhere else.

If the user asks "where's my account?", it's one tap from Settings, one page deep, one button to edit. If they don't ask, the app has done its work without them noticing — which is the entire point.
