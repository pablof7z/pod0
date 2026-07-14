# Identity-04 — The Newcomer's Profile

> **Angle**: The user has never heard of Nostr. They just want a podcast app. Identity is invisible plumbing until they choose to look at it. When they do — usually to change their photo — the surface should feel like the Profile screen of any well-made app, not a cryptography tutorial.
>
> **Voice**: *Most users never need to look here. Make the moment they do feel respectful, not technical.*

---

## 1. First Launch — invisible by design

There is **no identity moment**. No "Hi, you're Bright Signal!" reveal. No constellation animation. No keys to back up. The word "Nostr" never appears.

What happens: at first launch, `UserIdentityStore.start()` silently generates a key, derives a stable display name and dicebear avatar, and writes them to the keychain. The kind-0 profile publishes in the background. The user sees the onboarding flow described in `ux-10-onboarding.md` — they pick podcasts, they hear a briefing — and identity is never mentioned.

The closest thing to a "reveal" is implicit: the agent addresses the user by their generated name in its first spoken line.

> *"Hi Bright Signal — I read everything from this week. Want the four-minute version?"*

That's the only place a newcomer hears their name on day one. They might smile. They might wonder where it came from. Most won't notice. **All three responses are correct.**

The user can rename themselves later in Settings → Profile, at which point the agent will use the new name. Until then, the generated name *is* their name, and the app treats it that way.

> Onboarding owns the "Identity created on device" beat (`ux-10` Step 3). This brief owns everything *after* onboarding — the Settings surface the user finds when curiosity, vanity, or a desire for a real photo brings them looking.

---

## 2. Settings → Profile — the entry point

At the top of Settings sits a single row labeled **"Profile"**. Not "Identity." Not "Account." Not "Nostr." Profile.

```
SETTINGS                                                     ⓧ
─────────────────────────────────────────────────────────────
                                                              
  ╭──────────────────────────────────────────────────────╮
  │  ◉  Bright Signal                                  › │   ← Profile row
  │     bright-signal-7a3f                                │
  ╰──────────────────────────────────────────────────────╯
                                                              
  PLAYBACK                                                   
  ◌  Voice & briefings                                    › 
  ◌  Downloads                                            › 
  ◌  Playback speed                                       › 
                                                              
  LIBRARY                                                    
  ◌  Subscriptions                                        › 
  ◌  Import & export                                      › 
                                                              
  ABOUT                                                      
  ◌  Help                                                 › 
  ◌  Send feedback                                        › 
                                                              
─────────────────────────────────────────────────────────────
```

**Surface specs**
- Container: standard Settings list. Not glass.
- Profile row: T1 clear glass card (the *one* visual lift in Settings — your face deserves it). Corner `lg` (16). Content: 36pt avatar, display name in `headline`, generated handle in `caption` `text.tertiary`.
- The handle (`bright-signal-7a3f`) is included to differentiate users with collisions. It's not labeled "username" because it isn't one — it's a stable label the system shows beneath your name.
- Tap target: full row. Chevron right.

**Why a card and not a row?** The card establishes "this is yours" without ceremony. A flat list row would imply equivalence with "Downloads" — which would be wrong. Profile is *who*; everything else is *what*.

---

## 3. Profile root — your face, your name, a copy button

Tapping Profile opens a screen that looks like the Profile screen of any well-built consumer app. No identity language. No keys.

```
‹ Settings           PROFILE                                  
─────────────────────────────────────────────────────────────
                                                              
                       ╭──────────╮                          
                       │          │                          
                       │   ◉◉◉    │   ← 88pt avatar           
                       │          │                          
                       ╰──────────╯                          
                                                              
                     Bright Signal                            
                     bright-signal-7a3f                       
                                                              
                                                              
            ╭───────────────────────────────╮                 
            │       Edit profile            │                 
            ╰───────────────────────────────╯                 
                                                              
                                                              
  ABOUT ME                                                    
  ╭──────────────────────────────────────────────────────╮  
  │ Listening from Madrid. Mostly tech podcasts and       │  
  │ history. I leave notes for myself.                    │  
  ╰──────────────────────────────────────────────────────╯  
                                                              
  ACCOUNT ID                                                  
  ╭──────────────────────────────────────────────────────╮  
  │ npub1q9x…f7zk                                  Copy  │  
  ╰──────────────────────────────────────────────────────╯  
  Used to sync your account across apps. You can ignore     
  this unless you know you need it.                         
                                                              
                                                              
  ◌  Advanced                                            ›  
                                                              
─────────────────────────────────────────────────────────────
```

**Surface specs**
- Reading surface — **T0 paper** (`bg.canvas`), no tinted glass. Editorial restraint.
- Hero avatar: 88pt circle, soft shadow `y:2 r:8 a:0.08`. Centered.
- Display name: `display.large` (New York, 28pt). Handle below in `caption` `text.tertiary`.
- "Edit profile" CTA: `.glassProminent` style (onyx fill on T3 clear), full width minus 32pt padding, centered. **Not** the agent gradient. Not copper. Profile is identity; identity is neutral.
- About me: T0 reading card, `body` text, `Corner.md`. Empty state shows a single line of placeholder *"Tap Edit profile to write a short bio."* in `text.tertiary`.
- Account ID: T0 inset well (`bg.sunken`), monospaced bech32 prefix+ellipsis+suffix, with a **Copy** button on the right. Caption beneath in `caption` `text.secondary`.
- Advanced: standard list row at the bottom, separated by 32pt of breathing room from the Account ID block. Visually de-prioritized — it's a doorway, not a destination.

**The Account ID line is the most important piece of plain-English copy in the brief.**

> *"Used to sync your account across apps. You can ignore this unless you know you need it."*

That sentence does three things: (a) tells a curious user what the long string is *for* without saying "public key," (b) explicitly grants permission to ignore it, (c) signals to a power user that "across apps" is the operative concept — they'll know what to do with it.

We never say `npub`. We never say "public key." We never say "Nostr." If a power user is hunting for those terms, they're already in Advanced.

---

## 4. Edit Profile — feels like every other app

```
‹ Profile           EDIT PROFILE              Save           
─────────────────────────────────────────────────────────────
                                                              
                       ╭──────────╮                          
                       │   ◉◉◉    │                          
                       ╰──────────╯                          
                       Change photo                          
                                                              
  NAME                                                        
  ╭──────────────────────────────────────────────────────╮  
  │ Bright Signal                                         │  
  ╰──────────────────────────────────────────────────────╯  
                                                              
  ABOUT                                                       
  ╭──────────────────────────────────────────────────────╮  
  │ Listening from Madrid. Mostly tech podcasts and       │  
  │ history. I leave notes for myself.                    │  
  │                                                       │  
  ╰──────────────────────────────────────────────────────╯  
  140 characters left                                         
                                                              
─────────────────────────────────────────────────────────────
```

**Behavior**
- "Change photo" opens an action sheet: **Choose a style** (the curated dicebear sheet — see §8), **Choose from library** (PhotosPicker), **Take photo** (camera), **Remove photo**.
- Save button is `text.primary` foreground, no background, top-right. Becomes `accentAgent` indigo only after a real change is made; disabled until then.
- Tapping Save: optimistic update — the new name appears everywhere instantly (Settings row, agent dialogue, future notes/memories). Background task signs and publishes a kind-0. **The user never sees "publishing to relays" or "syncing."** If publish fails, the local state still reflects the edit and a background retry happens on next foreground.
- Validation: name must be 1–48 chars, no leading/trailing whitespace. Failure shows a quiet inline caption beneath the field, never an alert.
- About: 0–280 chars. Counter only shows below 50 remaining.
- Pressing back with unsaved changes: confirmation alert (not dialog — same iOS 26 popover-elision rationale used in `UserIdentityView`'s sign-out): *"Discard changes?" / "Discard" / "Keep editing"*.

**No mention** of broadcasting, relays, signing, events, kinds. Saving feels instant because — to the user — it is.

---

## 5. Advanced — buried one level deep

The Advanced row in §3 leads here. This is where Nostr-aware language is allowed for the first time, and even here it's gentle.

```
‹ Profile           ADVANCED                                  
─────────────────────────────────────────────────────────────
                                                              
  Most people will never need anything on this page.          
  It's here for people coming from other apps that use         
  the same kind of account.                                    
                                                              
                                                              
  ◌  Use my own key                                       › 
     Already have an account from another app?                
                                                              
  ◌  Sign in with a remote signer                         › 
     If you keep your key in a separate signing app.          
                                                              
                                                              
  ─────────────────────────────────────────────────────       
                                                              
  ◌  Account details                                      › 
     Full account ID, copy options                            
                                                              
  ◌  Start a new account                                  › 
     Replaces the account on this device                       
                                                              
─────────────────────────────────────────────────────────────
```

**Surface specs**
- T1 clear toolbar; T0 list body.
- Lead paragraph in `body` `text.secondary`, 16pt top padding from nav bar. This paragraph is the difference between feeling respected and feeling lectured at.
- Each row: `headline` title + `caption` subhead, both in plain English. No `nsec`, no `NIP-46`, no `bunker` on the row labels — those terms appear inside the destination pages.
- Hairline divider between the two sign-in options (top group) and the account-management options (bottom group).

**Order matters.** "Use my own key" is the most common reason a power user lands here. "Start a new account" — destructive — sits at the bottom, separated by a hairline.

---

## 5a. Advanced → Use my own key

Dedicated page. The first place the term "Nostr" *can* appear, but only inside the explainer. The header doesn't use it.

```
‹ Advanced          USE MY OWN KEY                            
─────────────────────────────────────────────────────────────
                                                              
  If you already use an app like Damus, Amethyst, or          
  Primal, you have a private key — it usually starts          
  with `nsec1`. Paste it here and Podcastr will use the       
  same account, so your profile, follows, and notes show      
  up the same everywhere.                                      
                                                              
  Your key is stored only in this device's keychain.          
  We never see it. We never send it anywhere.                  
                                                              
                                                              
  YOUR KEY                                                    
  ╭──────────────────────────────────────────────────────╮  
  │ nsec1…                                          Paste │  
  ╰──────────────────────────────────────────────────────╯  
                                                              
                                                              
            ╭───────────────────────────────╮                 
            │      Use this key             │                 
            ╰───────────────────────────────╯                 
                                                              
                                                              
  Don't have one? You don't need one — your existing          
  account works fine. This is just for people coming from     
  other apps.                                                  
                                                              
─────────────────────────────────────────────────────────────
```

**Surface specs**
- T0 reading surface. Body copy in `body`, 24pt margins.
- Field: T1 clear glass, monospaced, secure entry, with an inline **Paste** button that auto-fills if the clipboard contains a string starting with `nsec1`. (Mirrors the existing `Nip46ConnectCard.autoPasteBunkerIfPresent()` pattern — the user copied it from somewhere; assume good intent.)
- Validation: bech32 decode on tap of "Use this key." On success, `UserIdentityStore.importNsec(_:)` runs, the page pops back to Profile, and a quiet confirmation toast: *"Your account is now signed in."* No fireworks.
- On failure: inline red caption beneath the field — *"That key doesn't look right. Check the start (it should begin with `nsec1`) and try again."*

**Tone notes**
- "Private key" appears once, plainly, with `nsec1` in code voice as a recognizable shape — not as a definition.
- The footer paragraph is the doctor's-note line: it explicitly tells anyone confused that they don't need to be here. The newcomer should read this, exhale, back out.

---

## 5b. Advanced → Sign in with a remote signer

```
‹ Advanced     SIGN IN WITH A REMOTE SIGNER                   
─────────────────────────────────────────────────────────────
                                                              
  Some people prefer to keep their key in a separate          
  signing app — like Amber or nsec.app — and let other        
  apps ask permission whenever they need to post              
  something. Podcastr supports this.                           
                                                              
  Open your signer app, find the option to connect a new      
  app (it might say "bunker" or "remote sign"), and paste     
  the connection link here.                                    
                                                              
                                                              
  CONNECTION LINK                                             
  ╭──────────────────────────────────────────────────────╮  
  │ bunker://…                                     Paste │  
  ╰──────────────────────────────────────────────────────╯  
                                                              
            ╭───────────────────────────────╮                 
            │           Connect             │                 
            ╰───────────────────────────────╯                 
                                                              
─────────────────────────────────────────────────────────────
```

Mechanically this is the existing `Nip46ConnectCard` flow, restyled as a full-page reading surface instead of a card-in-a-stack. State machine unchanged — `connecting` / `awaitingAuthorization(url)` / `connected` / `failed` all surface the same in-content statuses, but in the gentler tone of this page (e.g., "Waiting for you to approve in your signer app…" instead of "Waiting for approval…").

The technical terms `NIP-46` and `bunker URI` do not appear as headers. They appear once each in the body, set lowercase and in passing.

---

## 6. Re-signing in — frictionless because we expect it

The newcomer-pure path is *never* re-sign-in. But power users who already have an nsec from another app *will* land in Podcastr expecting to use it. We want their first impression to be a 10-second flow, not a hunt.

**Three discoverability surfaces:**

1. **Onboarding's "I know what I'm doing" link** (per `ux-10`, S1) deep-links to *Use my own key* directly. They paste, they're in, they continue onboarding with their real identity from the start.
2. **Settings → Profile → Advanced → Use my own key** — the deliberate route described above.
3. **A welcome message in Send Feedback's first thread**, only visible if the user later realizes they want their real identity attached: *"Already have an account on Nostr? You can use it here — Settings → Profile → Advanced."* This is the only place outside onboarding and Advanced where we hint at the existence of an external-account world. (Optional; ship with §1 + §2 first, add the feedback hint based on usage.)

**Friction budget for the import flow**: from "Settings tab" to "key imported" should take ≤ 4 taps and ≤ 8 seconds. Currently: Settings → Profile → Advanced → Use my own key → Paste (auto-fills) → Use this key. **Five taps**. We trim by making "Advanced" a deep-link destination from the onboarding power-user link (skip the Settings hop on first run).

If the user is already signed in (generated key) and imports an nsec, the generated key is replaced. The newly-imported account becomes the active one. Notes and memories created under the old generated key remain on the network forever (we can't unpublish), but they no longer appear under "your" identity in the app — and we say so honestly in the sign-out warning (§7).

---

## 7. Sign-out — plainly told

Located at Advanced → **Start a new account** (we never use the phrase "sign out" — the newcomer didn't sign in, so signing out is conceptually wrong). The destructive action of removing the current key and generating a new one is honest about its consequences.

Triggered with `.alert` (not `.confirmationDialog`) — same iOS 26 popover-elision risk that the existing `UserIdentityView` solved with the same primitive. Reuse the rationale comment.

```
        ┌─────────────────────────────────────┐
        │                                     │
        │       Start a new account?          │
        │                                     │
        │  This will replace your current     │
        │  account on this device. Anything   │
        │  you've already posted (notes,      │
        │  memories, feedback, clips) stays   │
        │  online but you won't be able to    │
        │  edit it from here anymore.         │
        │                                     │
        │  If you have your key saved          │
        │  elsewhere, you can sign back in    │
        │  later under Advanced.              │
        │                                     │
        │  ┌─────────────┐  ┌──────────────┐  │
        │  │   Cancel    │  │  Start new → │  │
        │  └─────────────┘  └──────────────┘  │
        │                                     │
        └─────────────────────────────────────┘
```

**Copy decisions**
- "Start a new account" — frames the action as a *beginning*, not an *ending*. Less scary, more accurate (a new key *is* generated).
- The list of what stays online (notes, memories, feedback, clips) is concrete. Vague language ("your data") would make this feel sketchy.
- The mention of "your key saved elsewhere" is the only place outside Advanced sub-pages where we acknowledge the existence of external key custody. It's a parenthesis for the people who need it; a newcomer reads past it.
- "Start new →" — destructive role on the right (iOS convention), but not red. The action isn't dangerous; it's just consequential. Save red for actually destructive things (Delete account, eventually).

**Mechanism**: `clearIdentity()` followed by `start()` (which generates a fresh key per current behavior). The user lands back on Profile with a new auto-generated name and avatar. No celebration. Quiet.

---

## 8. Avatars — six curated styles, plus your own photo

**The call**: ship a hybrid. Default to a curated set of 6 dicebear styles, plus PhotosPicker. Not "always system picker" (forces a decision the newcomer doesn't want at zero state). Not "always random dicebear" (the trained eye reads it as generic, lazy — see character voice).

**The 6 styles** — chosen for distinctness in 200ms peripheral vision and absence of cultural baggage:

| # | Dicebear style | Visual character           | Notes                                           |
|---|----------------|---------------------------|-------------------------------------------------|
| 1 | `personas`     | Soft geometric humans      | Current default. Friendly, neutral.             |
| 2 | `notionists`   | Hand-drawn line illos     | Editorial register, matches our type voice.    |
| 3 | `lorelei`      | Painterly portraits       | Warm, expressive — for users who want a "face"  |
| 4 | `shapes`       | Abstract geometric         | For users who don't want a face at all          |
| 5 | `glass`        | Translucent gradient orb  | Echoes our material language; ambient feel      |
| 6 | `identicon`    | Algorithmic mark          | The "minimal" choice — geek-friendly without jargon |

**Picker UI** (inside the "Change photo" sheet → "Choose a style"):

```
‹ Edit profile      CHOOSE A STYLE                            
─────────────────────────────────────────────────────────────
                                                              
  Each style is built from your account, so the result is    
  always yours.                                               
                                                              
  ╭──────╮ ╭──────╮ ╭──────╮ ╭──────╮ ╭──────╮ ╭──────╮      
  │ ◉◉◉ │ │ ✏✏✏ │ │ 🖼🖼🖼 │ │ ▲▼◆ │ │ ◌◌◌ │ │ ░▒░ │      
  │ Per- │ │ Note │ │ Lorel│ │Shape │ │Glass │ │Ident │      
  │sonas │ │ist   │ │ ei   │ │ s    │ │      │ │icon  │      
  ╰──────╯ ╰──────╯ ╰──────╯ ╰──────╯ ╰──────╯ ╰──────╯      
   ●                                                         
  (current)                                                   
                                                              
                                                              
            ╭───────────────────────────────╮                 
            │           Use this style      │                 
            ╰───────────────────────────────╯                 
                                                              
─────────────────────────────────────────────────────────────
```

**Specs**
- 6 tiles in a horizontal scrollable rail (3 visible on iPhone Mini, 4 on standard widths). Each tile 92pt avatar + 18pt label.
- Tiles are T1 clear glass, `Corner.lg`, with a 2pt `accentAgent` ring on the currently-selected tile (only place agent-indigo touches the profile screen — semantically OK because picking a style is a creative act, and the agent is our "creative" identity).
- Each preview is generated with the user's own pubkey-derived seed, so they see *their* version of each style before committing — no surprise on save.
- Tap a tile = preview only (no change yet). "Use this style" commits and pops back. This separation prevents the destructive "I tapped it just to see" mistake.
- The lead sentence — *"Each style is built from your account, so the result is always yours."* — quietly conveys deterministic-from-pubkey without saying "pubkey."

**The PhotosPicker path** (separate action sheet entry: *Choose from library*) replaces the dicebear avatar with a user-uploaded photo, scaled and cropped to a 512×512 square, stored locally and referenced in the kind-0 `picture` field via a Blossom upload (separate brief). A photo overrides the style choice. *Remove photo* returns to whichever style the user last picked, defaulting to `personas`.

**Why six and not eight or four**: six fills a single horizontal rail comfortably on every supported screen size with one half-clipped peek tile suggesting "scroll for more." Four feels stingy. Eight feels like a dropdown.

---

## 9. Wiring contract — what gets signed by this identity

In plain English, every place in Podcastr where the user creates content that gets cryptographically signed by the identity described in this brief:

| Surface                       | Plain-English description                                                       | Today / Future |
|-------------------------------|---------------------------------------------------------------------------------|----------------|
| **Notes**                     | The notes you write on episodes (`AppStateStore+Notes.swift`)                  | Today          |
| **Memories**                  | The bits you save for later (`AppStateStore+Memories.swift`)                   | Today          |
| **Feedback you send**         | Compose + thread replies in Send Feedback (`FeedbackStore`, `FeedbackComposeView`) | Today          |
| **Shake-to-feedback**         | The thing where you shake the phone to report something (`ShakeDetector`)      | Today          |
| **Highlights**                | Passages you mark in transcripts                                                | Future         |
| **Clips**                     | Audio segments you share                                                        | Future         |
| **Comments**                  | Replies on a podcast or episode                                                 | Future         |
| **Friend messages**           | DMs to your friends and their agents (per `ux-12-nostr-communication.md`)      | Future         |

**The user surface for this contract** lives nowhere in the primary app. It belongs in **Advanced → Account details**, under a small expandable section labeled *"What's signed by this account"* — bullets in plain English using the table's right column. Most newcomers never open it. The presence of the list, calmly written, is what matters.

**Copy for that section** (verbatim, when implemented):

> *"Anything you create here is automatically attached to your account, so you can take it with you to other apps that use the same kind of account. This means: notes you write, memories you save, the feedback you send, and (later) highlights, clips, and comments. The app does this for you in the background — you don't need to do anything."*

---

## 10. Liquid Glass tier discipline (per `ux-15`)

Profile is a **reading surface**, not an alive surface. Tier choices are deliberately quiet.

| Surface                          | Tier | Tint / corner                     | Rationale                                       |
|----------------------------------|------|-----------------------------------|-------------------------------------------------|
| Settings nav chrome              | T1   | clear / `lg`                      | Standard system register                        |
| Settings → Profile entry card    | T1   | clear / `lg`                      | The one lift in Settings — your face            |
| Profile root (body)              | T0   | paper                             | Reading content                                 |
| About me card                    | T0   | `bg.elevated` / `md`              | Reading content                                 |
| Account ID well                  | T0   | `bg.sunken` / `md`                | Inset, code-like — sunken signals "data, not action" |
| "Edit profile" CTA               | `.glassProminent` (T3 onyx) | `pill` / 14r        | Primary action, neutral identity                |
| Edit Profile fields              | T0   | `bg.sunken` / `md`                | Form inputs as wells                            |
| Change photo / style picker tiles | T1  | clear / `lg`                      | Selectable, light glass                         |
| Selected style ring              | 2pt `accentAgent` indigo | —                       | Only place agent gradient appears on Profile    |
| Advanced page body               | T0   | paper                             | Reading content                                 |
| Use my own key / Remote signer pages | T0 + T1 toolbar | —                       | Reading + form                                   |
| Sign-out alert                   | system `.alert` | —                                | Per existing `UserIdentityView` precedent       |

**Forbidden on these surfaces**: T2 player tint (copper is reserved), T2 friend tint (amber is reserved), T2 agent tint as a fill (only as the 2pt selected-style ring). T4 cinematic — never; this is not a hero surface.

The Profile experience is intentionally **the calmest screen in the app**. Quiet is the message.

---

## 11. Open questions / risks

- **Generated handle collisions.** With ~36 adjective×noun combinations and a 4-char pubkey suffix, collisions in display name (without suffix) will happen frequently. Decision: always show the suffix in the handle line, never in the display name. Two "Bright Signal" users coexist; the suffix differentiates.
- **Renaming ergonomics.** When a user renames themselves, the agent's first dialogue on the next launch should naturally pick up the new name. Verify the agent's prompt template reads from `display_name` at invocation time, not at session start.
- **Photo upload host.** PhotosPicker → kind-0 `picture` requires a host. Blossom is the natural choice (per Nostr conventions) but adds infrastructure. Interim option: store photo locally, use the `picture` field only when the user has imported their own nsec (signaling they care about cross-app portability). Newcomers default to dicebear. Decide before shipping §4.
- **Power-user discoverability.** The "I know what I'm doing" link in onboarding is critical. If it's not present, power users will rage-uninstall before finding the Advanced menu. Verify with the Onboarding brief owner.
- **Account ID copy affordance.** Single tap on the row could also copy (instead of requiring a tap on the explicit Copy button). Ship with explicit Copy button — fewer accidental clipboard writes — but instrument and revisit.
- **The word "account" overloaded.** "Your Podcastr account" vs. "your Apple ID" vs. "your OpenRouter account" — three different things. We'll call the OpenRouter one "your AI key" everywhere it appears in Settings, to keep "account" reserved for the identity described here.

---

**File**: `/Users/pablofernandez/Work/podcast-player/docs/spec/briefs/identity-04-newcomer.md`
