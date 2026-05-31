# Android Feature Parity - Status Matrix

**Goal:** bring the Android Compose shell to feature parity with the iOS app,
built on the same NMP/Rust kernel. Business logic stays in Rust; Android is a
thin rendering + capability shell (mirror of `App/Sources/` on iOS).

**Reference:** `App/Sources/` is the parity specification; the Rust kernel
(`apps/nmp-app-podcast/`) is the shared source of truth. The Android UI lives
in `android/Podcast/app/src/main/java/io/f7z/podcast/`.

## Status Labels

| Label | Meaning |
|---|---|
| Shipped | User-visible behavior works through the NMP stack on Android. |
| This PR | Landed by the "snapshot completeness + subscribe/search/episode detail" PR. |
| Scaffold | Types / UI / action shells exist, but real behavior is absent. |
| Not started | No Android implementation yet. |

## Tier 1 - Core Usability

| Feature | Status |
|---|---|
| Subscribe via RSS | This PR |
| Search (iTunes/RSS directory) | This PR |
| Library / show grid | Shipped |
| Show detail + episode list | This PR (detail + tappable episode list) |
| Episode detail view | This PR |
| Feed refresh (pull-to-refresh) | This PR |
| Audio playback | Shipped |
| Variable speed | Shipped |
| Sleep timer | Scaffold (action wired; no UI control yet) |
| Episode download UI | Not started (action wired; no UI surface) |
| Playback settings | Not started (snapshot field decoded; no settings UI) |
| Playback queue | Not started (snapshot field decoded; no queue UI) |
| Lock-screen / media controls | Not started |

## Tier 2 - AI

| Feature | Status |
|---|---|
| Inbox triage | Not started (snapshot field decoded; no inbox UI) |
| Agent chat | Scaffold |
| Transcripts | Not started |
| AI chapters | Not started (chapters render in episode detail; no synthesis trigger) |
| Auto ad-skip | Not started |
| RAG / wiki | Not started |
| AI briefings | Scaffold |
| Voice mode | Scaffold |
| AI picks / categories | Not started (categories render in episode detail; no picks rail) |

## Tier 3 - Nostr

| Feature | Status |
|---|---|
| Keypair generation | Not started |
| BYOK nsec | Scaffold / stub |
| NIP-46 bunker | Not started |
| Profile editing | Not started |
| Relay list | Not started |
| NIP-F4 discovery + publish | Not started |
| Episode comments | Not started |
| Social graph | Not started |

## Tier 4 - Platform

| Feature | Status |
|---|---|
| Android Auto | Not started |
| Home-screen widget | Scaffold |
| App Actions | Not started |
| Local notifications | Not started |

## Notes on this PR

This PR moves Android from a read-only tech demo to a player a user can
actually drive: find a podcast, subscribe, browse episodes, open an episode,
and play it.

- **Snapshot completeness.** `PodcastSnapshot.kt` now mirrors the verified Rust
  projections (`ffi/projections/{library,settings,inbox}.rs`): `search_results`,
  `settings`, `queue`, `inbox` on the top-level snapshot; the full
  `EpisodeSummary` field set (`enclosure_url`, `description`, `played`,
  `starred`, `download_path`, `playback_position_secs`, `chapters`,
  `ai_categories`, `triage_decision`); `PodcastSummary.{feed_url, author,
  description}`; and new `ChapterSummary` / `SettingsSnapshot` / `InboxItem`
  data classes using the real wire field names.

- **Action contract corrected.** The kernel dispatch model is
  `(namespace, op-tagged body)` — namespace `"podcast"` or `"podcast.player"`,
  body `{"op":"<variant>", …}` — not flat dotted action ids. `subscribe`,
  `unsubscribe`, `refresh_all`, `search_itunes`, `download`, `delete_download`
  ride `"podcast"`; `play`, `pause`, `seek`, `set_speed`, `set_sleep_timer`
  ride `"podcast.player"`. The prior demo passed the dotted op path as the
  *namespace* (unregistered), so its player dispatches never reached the
  kernel; this PR fixes Player + Home to the correct contract.

- **Known gaps.** Sign-in remains stubbed; downloads have no UI surface (the
  action is wired but no button/affordance exists); playback settings, queue,
  and inbox are decoded from the snapshot but have no dedicated screens yet.
