# Identity-03 — iOS-Native / Apple Settings

> **Voice**: Familiar, predictable, no surprises. The user already knows how to use this surface — they have used it on their iPhone since iOS 11. We do not reinvent. We adopt.

> **One of five parallel proposals** for the User Identity surface. This proposal answers the question *"what would Apple ship?"* by mirroring Settings → Apple Account / iCloud to the letter. Sibling proposals may take editorial, ambient, narrative, or branded angles; this one is the boring-on-purpose baseline.

---

## 1. Position

Identity is the **first row** of the Settings root, above `Library`. This is non-negotiable: in Apple's own Settings, the Apple Account row sits above all other groups inside its own ungrouped first section. We match exactly.

```
┌───────────────────────────────────────────────────────┐
│  Settings                                             │
├───────────────────────────────────────────────────────┤
│  ┌────┐                                              ›│
│  │ ◐  │   Bright Signal                              │
│  └────┘   Podcastr Identity, Notes, Memories         │
├───────────────────────────────────────────────────────┤
│  LIBRARY                                              │
│  📡  Subscriptions                              42  ›│
│  ▦   Categories                                  6  ›│
├───────────────────────────────────────────────────────┤
│  PLAYBACK                                             │
│  ▶︎   Player                  1× · 30s · 30s         ›│
├───────────────────────────────────────────────────────┤
│  KNOWLEDGE                                            │
│  ✦   AI                       claude-opus-4         ›│
│  ▤   Wiki                                            ›│
│  …                                                    │
└───────────────────────────────────────────────────────┘
```

**Row anatomy** (matches the Apple Account row pixel-for-pixel):

- **Avatar**: 60pt circle, leading. Image from `picture` field of the kind:0 profile event; falls back to the dicebear SVG already generated in `UserIdentityStore.generatedProfile`. Circular mask, 0.5pt hairline ring at `separator` colour.
- **Title**: `display_name` from kind:0. Falls back to the `Bright Signal`-style auto-name. SF Pro Text body, weight `.semibold`, `text.primary`.
- **Subtitle**: `Podcastr Identity, Notes, Memories`. SF Pro Text footnote, `text.secondary`. This is the *Apple* idiom: "what is this account signed into" — we copy it.
- **Chevron**: standard `chevron.forward` glyph, `text.tertiary`, trailing.
- **Row height**: 76pt (matches the iOS Settings Apple Account row).
- **Inset**: standard `.insetGrouped` 16pt horizontal, free-standing first section (no header).

The row is a **`NavigationLink`** that pushes `IdentityDetailView`. No sheet. No popover. No glass. No motion beyond the system push.

> Liquid Glass appears in this proposal **only** on the navigation bar and any sticky `safeAreaInset` toolbars — i.e., the same places Apple's own Settings adopts material in iOS 26. The list itself is `Color(.systemGroupedBackground)` with `Color(.secondarySystemGroupedBackground)` cells. No tinted glass on rows.

---

## 2. Identity Detail View

Pushed from the Settings root row. Title: **"Identity"**. Large title, inline on scroll. Standard back chevron labelled `Settings`.

```
┌───────────────────────────────────────────────────────┐
│  ‹ Settings                Identity                   │
├───────────────────────────────────────────────────────┤
│                                                       │
│                       ╭──────╮                        │
│                       │  ◐   │   100pt                │
│                       ╰──────╯                        │
│                                                       │
│                    Bright Signal                      │
│                npub1xy7…h2qe  ⎘                       │
│                                                       │
├───────────────────────────────────────────────────────┤
│  PROFILE                                              │
│  ⓘ   Edit Profile                                   ›│
├───────────────────────────────────────────────────────┤
│  SIGN IN                                              │
│  🔑  Sign In with Different Identity               ›│
├───────────────────────────────────────────────────────┤
│  ⏻   Sign Out                                         │
└───────────────────────────────────────────────────────┘
       Your private key is stored on this device
       in the iOS Keychain. It never leaves.
```

### Sections, top to bottom

**Header card (free-standing, no Section header)**
- Centered avatar, 100pt, circular, 0.5pt hairline.
- `display_name` directly below — `title2`, `.semibold`, `text.primary`.
- `npub1xy7…h2qe` — middle-truncated bech32, `footnote` mono, `text.secondary`. Tap copies to clipboard with a `UINotificationFeedbackGenerator.success` haptic and a 1.4s "Copied" inline toast (system style — `.tint(.green)` checkmark replaces the trailing `doc.on.doc` glyph, matching the Apple Account screen's "copy email" affordance).
- No buttons. No CTA. No QR code in this proposal — this is the predictable surface.

**`PROFILE` section (one row)**
- `Edit Profile` → pushes `EditProfileView` (§3). Icon `person.text.rectangle`, tinted `Color.accentColor`. No subtitle.

**`SIGN IN` section (one row)**
- `Sign In with Different Identity` → pushes `SignInRouterView` (§4 + §5). Icon `key`, tinted `.gray`. No subtitle. The phrasing matches Apple's *"Sign In with a different Apple Account"*.

**Destructive footer (free-standing, no Section header)**
- `Sign Out` — full-width row, `text` only (no badge icon), `.foregroundStyle(.red)`. Body weight regular, centered. No chevron. Tapping fires the alert in §6.
- Below it, a footer-style `Text` block — `caption`, `text.tertiary`, centered, two lines max:
  > *Your private key is stored on this device in the iOS Keychain. It never leaves.*

**No** "remove key from this device" alternate copy. **No** badge counts. **No** glass cards. **No** liquid morphing avatar. This is a system surface; the user is not surprised by what they see.

---

## 3. Edit Profile

Pushed from `Edit Profile`. Title: **"Edit Profile"**, inline. `Form` with `.insetGrouped`. Toolbar: leading `Cancel`, trailing `Done` (disabled until any field changes; matches Apple Account → Name, Phone Numbers, Email).

```
┌───────────────────────────────────────────────────────┐
│  ‹ Identity              Edit Profile          Done   │
├───────────────────────────────────────────────────────┤
│  PHOTO                                                │
│                       ╭──────╮                        │
│                       │  ◐   │   80pt                 │
│                       ╰──────╯                        │
│                  Choose Photo                         │
│                  Take Photo                           │
│                  Reset to Default                     │
├───────────────────────────────────────────────────────┤
│  DISPLAY NAME                                         │
│  Bright Signal                                        │
├───────────────────────────────────────────────────────┤
│  USERNAME                                             │
│  bright-signal-2c4f                                  │
├───────────────────────────────────────────────────────┤
│  ABOUT                                                │
│  Feedback identity generated by Podcastr.             │
│                                                       │
│                                                       │
│                                              0 / 280  │
└───────────────────────────────────────────────────────┘
```

### Field rows

**`PHOTO` section** — hosts a centered 80pt avatar followed by three native rows:
- `Choose Photo` — `PhotosPicker(selection: $item, matching: .images, photoLibrary: .shared())`. The picker is the Apple system picker, full-screen, no custom chrome.
- `Take Photo` — pushes a `UIImagePickerController` wrapped via `UIViewControllerRepresentable` with `.camera` source. Permission prompt is the standard `NSCameraUsageDescription` flow.
- `Reset to Default` — destructive (`.foregroundStyle(.red)`). Sets `picture` back to the dicebear seed-derived URL.

Selected images need a hosted URL — the `picture` field of kind:0 is a URL string, and inlining a base64 `data:` URI is unsafe (a 1024² JPEG at q=0.85 is ~150–300KB; base64 inflates +33%; many relays cap event size at 16–256KB and will drop the kind:0 silently). Until a media-host integration lands, the `Choose Photo` and `Take Photo` rows are present but **disabled with a footer**:
> *"Custom photos arrive with a future update. For now, your photo is generated from your key."*
The `Reset to Default` row is hidden in this same window. Sibling proposal `identity-04-blossom` is the surface that wires up Blossom upload and re-enables these rows; this proposal does not pretend to solve hosting it cannot solve.

**`DISPLAY NAME` section** — single `TextField` row, body, `text.primary`. Header `DISPLAY NAME` (uppercased, system caption2 SmallCaps style — Apple's standard form header). Footer not shown unless validation fails. Max 64 chars. `.textInputAutocapitalization(.words)`.

**`USERNAME` section** — single `TextField`, mono, `text.secondary`. `.textInputAutocapitalization(.never)`, `.autocorrectionDisabled()`. Validates against `^[a-z0-9_-]{3,32}$`. Footer (always visible):
> *Used in @mentions. Lowercase letters, numbers, hyphens, and underscores.*

If invalid, footer turns `red` and reads the specific validation reason ("Must be at least 3 characters", etc.). `Done` button stays disabled while invalid.

**`ABOUT` section** — `TextField(..., axis: .vertical).lineLimit(3...8)`, body. Footer right-aligned character counter `0 / 280`. Counter turns `red` past 280; `Done` disables.

### Save semantics

- `Done` writes to a draft in-memory profile, signs a kind:0 event with the current `signer`, publishes to `FeedbackRelayClient.profileRelayURLs` (same path as `publishGeneratedProfileIfNeeded`), and pops back.
- A `ProgressView()` replaces `Done` for ≤2s; on failure, a native `.alert` ("Couldn't save profile — Try Again / Cancel") appears, error text taken verbatim from the relay client.
- On success, `Haptics.success()` and a system "Saved" toast — but **no celebratory motion**. This is Settings.

### Cancel semantics

If any field is dirty, `Cancel` raises a native `.confirmationDialog`:
> *Discard Changes?* — `Discard Changes` (destructive) / `Keep Editing`.

If clean, `Cancel` pops immediately.

---

## 4. Sign In with Different Identity (router)

`Sign In with Different Identity` does not present a sheet or a menu. It pushes a **router view**, `SignInRouterView`, which is itself a list of two rows. This is the same pattern Apple uses when "Sign In with a different Apple Account" splits into "Use existing" / "Create new".

```
┌───────────────────────────────────────────────────────┐
│  ‹ Identity      Sign In                              │
├───────────────────────────────────────────────────────┤
│  🔑  Sign In with nsec                              ›│
│  🔗  Sign In with NIP-46 Bunker                     ›│
├───────────────────────────────────────────────────────┤
│         Signing in replaces the identity              │
│         currently on this device. Future              │
│         activity will be signed with the              │
│         new key.                                      │
└───────────────────────────────────────────────────────┘
```

Footer text (caption, `text.tertiary`, centered, three lines) sets expectations exactly once. After this point neither push view repeats the warning — the user has read it.

### 4.1 Sign In with nsec

Push view, **not a sheet**. Title: `Sign In with nsec`, inline. Toolbar: trailing `Sign In` (disabled until `nsecInput.isEmpty == false && validNsecPrefix(nsecInput)`).

```
┌───────────────────────────────────────────────────────┐
│  ‹ Sign In        Sign In with nsec        Sign In   │
├───────────────────────────────────────────────────────┤
│                                                       │
│  PRIVATE KEY                                          │
│  ┌─────────────────────────────────────────────┐     │
│  │ nsec1•••••••••••••••••••••••••••••     ⎘  │     │
│  └─────────────────────────────────────────────┘     │
│                                                       │
│  Your private key is the password for your            │
│  Nostr identity. It is stored on this device          │
│  in the iOS Keychain and never leaves.                │
├───────────────────────────────────────────────────────┤
│  ▶︎  Show Key                                         │
│  ⎘  Paste from Clipboard                              │
└───────────────────────────────────────────────────────┘
```

**Field**: `SecureField("nsec1…", text: $nsecInput)`. Mono, `text.primary`. `.textInputAutocapitalization(.never)`, `.autocorrectionDisabled()`. The trailing `doc.on.clipboard` glyph inside the field is a tap target — no separate "Paste" button beside the field; we keep it inside, like the iOS Wi-Fi password field does.

**Show Key row** — toggles `SecureField` ↔ `TextField` for the same binding. Single row, full-width, default tint. Glyph swaps `eye` ↔ `eye.slash`. Apple-pattern.

**Paste from Clipboard row** — present **only** when `UIPasteboard.general.hasStrings && pasteboard string starts with "nsec1"`. Otherwise hidden. (We do not call `UIPasteboard.string` directly — that triggers the privacy banner. We probe `hasStrings` and let `UIPasteboard.detectPatterns` confirm `URL`-ish content. The actual read happens on tap.)

**Footer** (caption, `text.tertiary`):
> *Your private key is the password for your Nostr identity. It is stored on this device in the iOS Keychain and never leaves.*

**Validation** — on tap `Sign In`:
- Calls `identity.importNsec(_:)` (existing). On `loginError`, the error is surfaced as a footer below the field, `red`, no alert.
- On success: `Haptics.success()`, pop back **two** views (past the router) to the Identity Detail. The new avatar/name appear; if the kind:0 has not yet been fetched from relays, the avatar shows a 28pt SF Symbol `person.crop.circle.fill` placeholder and the title shows the `npubShort`. A `ProgressView().controlSize(.small)` sits next to the title until the profile resolves or a 4s timeout — then the auto-name is shown.

**Error states surfaced inline (never alerts)**:
- `Invalid nsec — check the key and try again.` (existing copy)
- `Network unavailable — your key is saved, profile will sync later.`

---

## 5. Sign In with NIP-46 Bunker

Push view. Title: `Sign In with Bunker`, inline. Embeds the existing `Nip46ConnectCard` **promoted** to a primary surface — i.e., its outer card chrome (the `Color(.secondarySystemBackground)` rounded rect) is dropped and its contents flow into a `Form` with the same section structure.

```
┌───────────────────────────────────────────────────────┐
│  ‹ Sign In       Sign In with Bunker                  │
├───────────────────────────────────────────────────────┤
│  BUNKER URI                                           │
│  ┌─────────────────────────────────────────────┐     │
│  │ bunker://abc…?relay=wss://…&secret=…   ⎘   │     │
│  └─────────────────────────────────────────────┘     │
│                                                       │
│  Paste a bunker URI from Amber, nsec.app,             │
│  or nsecBunker.                                       │
├───────────────────────────────────────────────────────┤
│  🔗  Connect                                          │
├───────────────────────────────────────────────────────┤
│         Your private key never touches this           │
│         device — every signature happens              │
│         inside the bunker.                            │
└───────────────────────────────────────────────────────┘
```

### State surfacing — inline, no sheets

The five `RemoteSignerState` cases each have a single, predictable representation:

| State | Surface |
|---|---|
| `.idle` | Form above. `Connect` row enabled when input non-empty. |
| `.connecting` | `Connect` row replaced by a row containing `ProgressView().controlSize(.small)` + caption *"Connecting to bunker…"*. URI field becomes read-only. |
| `.reconnecting` | Same as connecting; caption reads *"Reconnecting…"*. Shown only on launch when resuming a saved session — there is no input field above it (replaced by a cell showing the truncated bunker pubkey). |
| `.awaitingAuthorization(url)` | Inline section appears above `Connect`: header `APPROVE IN BROWSER`, single row `Approve in Browser` with glyph `safari`, blue tint, opens `url`. Below: `Cancel Connection` row, `red`. |
| `.connected(pubkey)` | Form is replaced by a "currently signed in" view: avatar + display_name + truncated npub + `Disconnect Bunker` row at the bottom (`red`). Identical structure to the Identity Detail header, scoped here. |
| `.failed(message)` | The form is unchanged; below `Connect` row, a footer in `red` shows the truncated message verbatim. Tapping `Connect` again retries. |

**Auto-paste**: matches existing `autoPasteBunkerIfPresent` — if `UIPasteboard.general.string` begins with `bunker://`, prefill on appear. No banner, no toast, no glass — Apple-style invisible convenience.

**No QR scanner in this proposal.** A camera icon would invite custom chrome we are not allowed to add. Sibling proposal (`identity-02-cinematic`) owns QR scanning.

---

## 6. Sign Out

`Sign Out` row at the bottom of Identity Detail (§2). Tap fires a native **`.alert`**, *not* `.confirmationDialog`. Per the existing comment in `UserIdentityView.swift`:

> "iOS 26's popover-promotion can elide the Cancel button on dialogs anchored to a tappable element […] Particularly important here: deleting the private key is irreversible if the user doesn't have their nsec backed up elsewhere."

```
        ╭──────────────────────────────────────╮
        │                                      │
        │           Sign Out?                  │
        │                                      │
        │   This will replace your identity.   │
        │   Future activity will be signed     │
        │   with a new key.                    │
        │                                      │
        ├──────────────────────────────────────┤
        │           Cancel                     │
        ├──────────────────────────────────────┤
        │       Sign Out (destructive red)     │
        ╰──────────────────────────────────────╯
```

**Title**: `Sign Out?`
**Message**: *"This will replace your identity. Future activity will be signed with a new key. Continue?"*
**Buttons** (in order, top to bottom — system order):
1. `Cancel` — `.cancel` role.
2. `Sign Out` — `.destructive` role. Calls `identity.clearIdentity()`. Then `start()` runs the silent generated-key path (next launch effectively, but we trigger it inline so the Identity Detail re-renders with the new auto-name). `Haptics.medium()` fires.

If the user has imported an `nsec` (i.e. `isGeneratedLocalKey == false`), the message reads:
> *"This will remove your imported key from this device. Make sure you have your nsec backed up — it cannot be recovered from Podcastr."*

If a NIP-46 bunker is connected:
> *"This will disconnect your bunker. Future activity will be signed with a new key."*

The button label remains `Sign Out` in all three cases. The Apple convention is one verb per destructive action, varied message text. No "Remove" / "Disconnect" / "Sign Out" trichotomy — that requires the user to recognise their own state, which we should not demand.

---

## 7. First-Launch Behaviour — Silent

The auto-generated identity is created in `UserIdentityStore.start()` if no key exists. **There is no UI for this.** No splash, no welcome card, no "your identity is ready" toast. The user opens the app, lands on Library, and the identity exists in Settings — discoverable when (and only when) they go looking.

This is the Apple Account model exactly: when a user signs into a brand-new iPhone, an Apple ID is *implied* by the setup flow but never *announced*. We do the same.

**Concrete consequence for the codebase:**

- `RootView` does not branch on `identity.hasIdentity`. The Settings row above-the-fold simply renders whatever the store currently holds.
- The first time the user posts a Note, Memory, or Feedback message, the auto-generated kind:0 has already been published (existing `publishGeneratedProfileIfNeeded` runs in `start()`), so the friend they're messaging or the relay they're posting to sees a fully-formed profile. No "anonymous" state ever appears on the wire.
- If publishing the kind:0 fails on first launch (no network), the next foreground enter retries. Still silent.

**What we do not do**:
- No "Welcome — your identity is *Bright Signal*" sheet.
- No Settings badge dot inviting the user to "complete your profile".
- No tooltip pointing at the avatar row.
- No avatar customisation prompt.

The user discovers their identity the same way they discover their Apple Account: by tapping Settings.

---

## 8. Wiring Contract — Where This Identity Signs

Apple Settings → Apple Account lists the services the account is signed into ("iCloud", "Media & Purchases", "Find My", …). We mirror this. The Identity Detail subtitle on the Settings root row reads:

> **Podcastr Identity, Notes, Memories**

…where the trailing list is the live, comma-joined set of *user-content* surfaces this key signs. The full list, as of this brief:

| Surface | Kind | Tag scheme | Site in code | Subtitle term |
|---|---|---|---|---|
| Feedback notes | `1` | `["a", projectCoordinate]`, `["t", category]`, `["-"]` | `UserIdentityStore.publishFeedbackNote` | "Feedback" |
| Notes (per-episode) | `1` | `["a", episodeCoordinate]`, `["t", "note"]` | `AppStateStore+Notes` (planned signing site) | "Notes" |
| Memories | `30078` (parameterised replaceable) or `1` per spec — TBD | `["d", memoryID]`, `["t", "memory"]` | `AppStateStore+Memories` (planned) | "Memories" |
| Highlights / Clips (future) | `9802` (NIP-84 highlight) | `["a", episodeCoordinate]`, `["context", transcriptSlice]` | TBD | "Highlights" |
| Comments on shows / episodes (future) | `1111` (NIP-22 comment) | `["A", showCoordinate]`, `["E", parentCommentID?]` | TBD | "Comments" |
| Shake-to-feedback | same as Feedback | inherits | `ShakeDetector` → `FeedbackComposeView` → `publishFeedbackNote` | (covered by Feedback) |
| Profile metadata | `0` | none | `publishGeneratedProfileIfNeeded`, `EditProfileView.save` | (implicit) |

**Subtitle composition rule**: the subtitle on the Settings root row is the comma-joined sequence `["Podcastr Identity"] + presentSurfaces`, in declaration order — yielding e.g. `Podcastr Identity, Notes, Memories`. The `Podcastr Identity` lead is fixed (it is the *what is this row* anchor, mirroring Apple's `Apple Account, iCloud, Media & Purchases` lead). Inactive surfaces (zero notes, zero memories) are still listed — Apple's Settings does not hide "Find My" just because nothing is being tracked. Predictability over information density.

### Surfaces this identity does **not** sign

- Agent runs (`AgentRun*`) — signed by the *agent* identity, separate `KeyPair` in `AgentIdentityStore`. Agent identity has its own Settings root row under `Agent` group.
- Friend-system DMs — signed by the agent identity (per existing template).
- Subscription / OPML state — local only, not on the wire.
- Playback position / history — local only.

The two identities are visually distinguished in Settings:

```
┌───────────────────────────────────────────────────────┐
│  Settings                                             │
├───────────────────────────────────────────────────────┤
│  ┌────┐                                              ›│
│  │ ◐  │   Bright Signal                              │   ← USER (this brief)
│  └────┘   Podcastr Identity, Notes, Memories         │
├───────────────────────────────────────────────────────┤
│  …library, playback, knowledge…                      │
├───────────────────────────────────────────────────────┤
│  AGENT                                                │
│  🧠  Agent                                          ›│   ← AGENT (existing)
└───────────────────────────────────────────────────────┘
```

The user row is at the very top because *the user is the principal*. The agent is a service the user owns, so it sits inside `AGENT` like `iCloud` sits inside Apple's settings.

---

## 9. What This Proposal Refuses

For clarity to reviewers comparing the five proposals, the explicit non-goals of this one:

- **No Liquid Glass on identity surfaces.** Glass appears only on the system-provided navigation bar and any `.toolbar(...)` chrome — i.e., wherever Apple's own Settings adopts material. Cards, rows, and buttons are matte system fills.
- **No custom avatar treatment.** No animated rings, no breathing scale, no rotating gradient. Circle. Hairline. Done.
- **No celebratory motion** on sign-in success — `Haptics.success()` and a system toast, that is the entire ceremony.
- **No QR onboarding**, no deep-link handling beyond the existing `bunker://` paste detection.
- **No identity switcher / multi-account.** Apple does not let you have two Apple Accounts active simultaneously. We do not let you have two Nostr keys.
- **No agent-mediated copy** — the Identity Detail never references the agent ("Your agent uses this key to…"). The agent has its own surface.
- **No editorial copy.** No "Welcome to your Podcastr identity" warmth. The strings are descriptive and short.
- **No empty state.** The auto-generated identity guarantees the surface always has content.

---

## 10. Accessibility & Localisation Notes

- All rows are `Button` or `NavigationLink` — VoiceOver hits the system-provided "Button" / "Link" trait automatically. No custom `accessibilityLabel` overrides needed except on the avatar (`accessibilityLabel("Profile photo of \(displayName)")`).
- The avatar in the Settings root row has `accessibilityHint("Tap to manage your identity")`.
- The npub copy affordance: `accessibilityLabel("Public key")`, `accessibilityValue(npubShort)`, `accessibilityHint("Double-tap to copy")`. The transient "Copied" state announces via `UIAccessibility.post(notification: .announcement, argument: "Copied")`.
- Dynamic Type: every row uses `AppTheme.Typography.body` / `.headline` — already scaling. Avatar size is fixed (60pt root, 100pt detail header) per Apple's Settings convention; the surrounding labels reflow.
- All strings live in `Localizable.strings` keys prefixed `identity.*` — no inline copy.
- RTL: `chevron.forward` flips automatically; `truncatedMiddle` on npub still reads correctly.
- Reduce Motion: no custom motion in this proposal, so nothing to gate.

---

## 11. Apple-Settings Pattern Audit

A line-by-line check that this proposal matches Settings → Apple Account on iOS 18 / iOS 26:

| Apple Settings element | Our analogue | Match? |
|---|---|---|
| Account row at top of Settings, above first group | Identity row at top, above Library | ✓ |
| Avatar circle, leading | Same | ✓ |
| Display name, semibold body | Same | ✓ |
| Subtitle listing services | Subtitle listing surfaces | ✓ |
| Detail view: hero card with avatar + name + AppleID | Hero card with avatar + name + npub | ✓ |
| Sections grouped by intent (Personal Info / Sign-In & Security / iCloud / …) | Profile / Sign In / Sign Out | ✓ (compressed; we have less surface area) |
| `Edit` button trailing on hero card | We push `Edit Profile` from a row instead — Apple uses both patterns; the row pattern is more current (iOS 17+) | ✓ |
| Sign Out as final destructive row | Same | ✓ |
| Native `.alert` confirmation on Sign Out | Same | ✓ |
| No bottom sheet for Sign-In flows | We use push views, not sheets | ✓ |
| `PhotosPicker` for avatar | Same | ✓ |
| `SecureField` with show/hide toggle for credentials | Same for nsec | ✓ |
| Inline error footer, never alert, on credential validation | Same | ✓ |
| Footer caption explaining where credentials are stored | Same | ✓ |

---

## 12. Open Questions for Synthesis

These are explicit hooks for the synthesis pass that picks across all five identity proposals:

1. **Username uniqueness**: Apple's Apple ID is globally unique by virtue of email/phone. Nostr usernames are not. Do we (a) accept collisions silently, (b) check against a relay-side directory on save, or (c) defer the field to a future spec? This proposal currently assumes (a).
2. **Profile sync**: kind:0 events are eventually consistent across relays. Apple Account changes feel instantaneous. Do we surface "Saving…" → "Synced" mini-status on the Identity Detail header? This proposal says **no** (silent, like Apple). Synthesis may override.
3. **Multi-device**: nothing in this proposal handles "I signed in with the same nsec on two phones, now I have two profiles editing the same kind:0". The last-write-wins semantics of Nostr are inherited; we do not surface conflict UI.
4. **Lost-key recovery**: Apple has Account Recovery. We have… `nsec backups`. The Sign Out alert mentions backup once; nothing else does. Synthesis may want a dedicated "Back Up Your Key" surface — out of scope here.

---

**File path**: `/Users/pablofernandez/Work/podcast-player/docs/spec/briefs/identity-03-ios-native.md`
