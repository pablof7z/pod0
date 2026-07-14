# Castro — competitive analysis

## What people love

- **The Inbox/Queue two-stack** is Castro's signature and widely called "game-changing." New episodes from subscriptions land in the Inbox — a neutral triage station — rather than auto-downloading into a flat queue. Users consciously decide what enters their listening list. ([9to5Mac](https://9to5mac.com/2020/07/19/castro-inbox/), [MacStories](https://www.macstories.net/reviews/castro-3-review-the-castro-youve-always-wanted/))
- **Per-show rules** let power users configure each podcast independently: auto-queue next/last, auto-archive, episode limits, per-show speed, Trim Silence on/off, Enhance Voices on/off. High-priority shows skip triage entirely.
- **Chapter support** is universally praised — chapter list accessible by tapping the episode title, chapter artwork, forward/back buttons between chapters, chapter pre-skip. Called "probably the best I have ever used."
- **Enhance Voices** (dynamic compressor targeting speech frequencies) and **Trim Silence** (smart silence removal, comparable to Overcast's Smart Speed) add 10–20% listening-time savings without perceptible quality loss.
- **Visual identity** — polished dark mode with colored episode art backdrops, clean typography, and a native iOS aesthetic that users consistently prefer over Overcast's utilitarian look.
- **Horizontal swipe in Now Playing** toggles between artwork and controls (sleep timer, speed, feature toggles), plus hidden double-tap and swipe-hold gestures — discoverability issue but depth that power users love.
- **Drag-and-drop queue reordering** for fast triage session management; inbox can be bulk-cleared.

## What people hate

- **Stability regressions post-acquisition** (2024): playback failures, CarPlay crashes, lost position/speed. The new team acknowledges them openly and is fixing, but trust was damaged. (Castro.fm blog, TechCrunch)
- **Castro Plus price increase** — new ownership raised prices >60% for some long-term subscribers. Significant backlash from the loyal base.
- **Feature discoverability is poor** — chapters require tapping the episode title, a non-obvious target; power features are buried. Users regularly discover features by accident or by Googling.
- **Device sync was missing for years**, only arriving in 2025 via iCloud Keychain (Castro Plus only). Competitors had it much earlier.
- **Near-death scare eroded trust**: Tiny let Castro go dark in late 2023, website down, engineers gone, before Bluck Apps rescued it in January 2024. Users who switched away have not all returned.

## Notable shipped features

- **Inbox triage + Queue** (core UX paradigm)
- **Per-show rules**: destination (Inbox / Queue Next / Queue Last / Archive), episode limits, per-show playback speed, per-show audio effects
- **Enhanced Audio**: Enhance Voices (dynamic speech compressor), Trim Silence (smart silence removal), Mix to Mono
- **Full chapter support**: chapter list, chapter artwork, chapter skip-ahead, CarPlay chapters (shipping with iOS 18 update)
- **AirPlay 2** and **Apple Watch** remote playback control
- **CarPlay** support (rewritten 2024)
- **Sleep timer**
- **Sharing**: share episode link or timestamp via system share sheet
- **Castro Plus**: $24.99/year — unlocks audio effects, chapters, per-show rules, dark mode, device sync (iCloud), priority support

## UX patterns worth noting

**The two-stack model (Inbox + Queue):**
The key insight is that a subscription list and a listening queue are different things. Castro treats subscriptions as signal sources and the Queue as an intentional playlist. New episodes flow into the Inbox — a date-ordered triage surface — where the user makes exactly one decision per episode: Queue Next, Queue Last, or Archive. Episodes auto-downloaded to Queue only when you say so (or via per-show Auto-Queue rules). This means the Queue reflects the user's explicit intent, not algorithmic defaults, and remains short enough to feel manageable. Contrast with Overcast/Pocket Casts where subscriptions and the queue blur together: every episode is either in a flat list or auto-queued, creating cognitive overload at scale.

**Swipe gestures in triage:** In the Inbox, a swipe-left or swipe-right gesture maps to "queue" vs "archive," making rapid triage possible without lifting a thumb. Long-press reveals extra options.

**Onboarding to the triage flow:** New users are taught the two-stack model immediately. Per-show rules let power users automate the triage for shows they always or never want in the queue, so the Inbox only surfaces genuine decisions.

**Now Playing card:** Full-screen card with episode artwork dominating the background (colored backdrop derived from artwork). Horizontal swipe left from artwork surface to reach controls. Swipe-hold from bottom for show notes. Double-tap artwork to star an episode.

**Visual identity:** Dark-forward, art-driven. Colored backdrops from episode artwork give each episode a distinct visual presence. Editorial typography is minimal but clean.

## What Podcastr should steal (3–7 ideas)

- **Feature**: AI-powered Inbox triage
  - **Why it fits Podcastr**: Castro's Inbox is a manual decision layer. Podcastr's AI agent has perfect knowledge of every unlistened episode and the user's listening history — it can do the triage autonomously, surfacing only episodes worth the user's attention and pre-queuing the rest. "AI triages your inbox while you sleep" is a direct, legible upgrade of Castro's core value prop.
  - **Effort**: M (UI is reuse of Castro pattern; AI ranking/scoring is the work)
  - **Risk**: Users who want control may feel agency is removed — offer a review mode alongside auto-mode.

- **Feature**: Per-show AI profiles (rules + context)
  - **Why it fits Podcastr**: Per-show rules in Castro are purely mechanical (speed, silence). Per-show AI profiles in Podcastr can include topics to highlight, guests to flag, skip intros/ads intelligently, and feed the RAG index per podcast — a qualitative upgrade.
  - **Effort**: M
  - **Risk**: Complexity; surface carefully in settings, not onboarding.

- **Feature**: Swipe-to-triage gesture system
  - **Why it fits Podcastr**: The swipe-left/swipe-right triage pattern is fast and addictive. Adapt it: swipe to queue, swipe to TLDR, swipe to archive. Directly maps to Podcastr's three consumption modes (listen, brief, skip).
  - **Effort**: S
  - **Risk**: None — gesture is well-established iOS pattern.

- **Feature**: Now Playing card with editorial art backdrop
  - **Why it fits Podcastr**: Castro's colored art backdrop gives each episode a cinematic presence. With Podcastr's "cinematic motion" pillar and Liquid Glass aesthetic, this pattern elevates into a full editorial moment — artwork, motion, and AI context card in one surface.
  - **Effort**: S (visual pattern) to M (Liquid Glass material + motion)
  - **Risk**: None.

- **Feature**: Episode limits / auto-archive for low-priority shows
  - **Why it fits Podcastr**: Users subscribed to 100+ shows (Castro's stated target user) need automatic pruning. AI can determine "you haven't listened to this show in 3 months, auto-archiving new episodes" without the user configuring it.
  - **Effort**: S
  - **Risk**: Over-automation; needs clear user visibility into what was archived.

## Anti-patterns to avoid

- **Burying features behind non-obvious gestures** — Castro's chapter access (tap the episode title) and starred-episode double-tap are power-user secrets that hurt discoverability. Podcastr should surface AI features prominently, not hide them in gesture layers.
- **Subscription price shocks** — Castro lost goodwill by raising prices >60% post-acquisition. Podcastr should communicate pricing changes early and grandfather legacy users.
- **Letting infrastructure rot** — Castro's near-death came from Tiny neglecting backend updates for years. Podcast app infrastructure (feed parsing, CDN, sync) must be treated as product, not ops.
- **Conflating queue and subscription** — the non-Castro apps (Overcast, Apple Podcasts) blur the two. Never let "subscribed to" imply "will play next." Podcastr's Queue is the user's explicit intent layer.

## One-line pitch

Castro proved that the podcast problem is a *triage* problem, not a playback problem — Podcastr's job is to make the AI agent so good at triage that the user never has to open an Inbox at all.
