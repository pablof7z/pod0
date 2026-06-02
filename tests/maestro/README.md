# Maestro UI Test Suite (P0) — Podcastr iOS

End-to-end UI flows for the highest-priority (P0) user journeys of the
Podcastr iOS app (`io.f7z.podcast`), written for
[Maestro](https://maestro.mobile.dev).

## Status: forward-looking scaffolding

These flows target `accessibilityIdentifier` values that a **parallel agent is
adding** to the iOS shell. As of writing, those identifiers are **not yet on
`origin/main`** (a `grep` for `accessibilityIdentifier` under `App/Sources/`
returns nothing). Until the id-tagging work lands and a build carrying those
ids is installed, the flows are **not runnable end-to-end** — the first
`assertVisible: { id: "tab-home" }` will fail. Each flow falls back to stable
visible text where the convention defines no id, and many `id:` taps are marked
`optional: true` so a missing id degrades to the text path rather than hard
-failing the whole suite.

## Install Maestro

```sh
curl -fsSL "https://get.maestro.mobile.dev" | bash
# then add to PATH per the installer output, and verify:
maestro --version
```

A booted iOS Simulator (or a connected device) with the app installed is
required. Build + install the app first (see repo `AGENTS.md` / Xcode), then:

```sh
# Run the full P0 suite
maestro test tests/maestro/config.yaml

# Run a single flow
maestro test tests/maestro/flows/p0/02-play-episode-basic.yaml
```

## Layout

```
tests/maestro/
  config.yaml                     # suite config; lists the 8 P0 flows
  README.md                       # this file
  flows/
    shared/
      launch.yaml                 # cold-launch + wait for Home
      subscribe-darknet.yaml      # setup: subscribe to "Darknet Diaries"
    p0/
      01-subscribe-via-search.yaml
      02-play-episode-basic.yaml
      03-pause-resume.yaml
      04-skip-forward-back.yaml
      05-offline-library-access.yaml
      06-queue-add-multiple.yaml
      07-background-playback.yaml
      08-reactive-state.yaml
```

Shared helpers are imported with `runFlow:` and are excluded from suite-level
discovery so they never run standalone.

## Flow summary

| Flow | Scenario | Network | Manual verification |
|---|---|---|---|
| 01-subscribe-via-search | Search + subscribe to "Darknet Diaries", confirm it lands in Library reactively | Yes | — |
| 02-play-episode-basic | Tap an episode, confirm mini-player appears + button flips to Pause | Yes | "elapsed advances within 1s" budget |
| 03-pause-resume | Pause then resume; control toggles Play↔Pause | Yes (via 02) | clock freeze while paused; resume from ~T not 0:00 |
| 04-skip-forward-back | Skip ±interval in the full player; playback stays playing | Yes (via 02) | ±15s numeric deltas; round-trip ≈ original |
| 05-offline-library-access | Cold-launch offline; cached Library + episodes render | No (offline) | **toggle network off** (no portable Maestro command); render ≤2s, graceful offline |
| 06-queue-add-multiple | Long-press 3 episodes → Add to Queue; open Up Next | Yes | exact A/B/C order; count badge = 3 |
| 07-background-playback | Background ~5s, foreground; UI stays consistent | Yes (via 02) | **audio keeps playing** + elapsed matches on return |
| 08-reactive-state | Download from detail, see live row update; play-state consistent cross-screen | Yes | live advancing download % on the list row |

## Setup helpers

- **`shared/launch.yaml`** — cold-launches with `stopApp: true` and waits for
  the Home tab.
- **`shared/subscribe-darknet.yaml`** — subscribes to "Darknet Diaries" (a
  stable public feed) so flows needing "a subscribed podcast with episodes" can
  `runFlow` it. Network-dependent.

## Reality vs. the task's stated convention (IMPORTANT)

The task brief described a tab set of *Home, Library, Downloads, Briefings,
Social, Inbox, Agent, Identity*. The **actual** app
(`App/Sources/App/RootView.swift`) differs:

- Real tabs: **Home, Library, Bookmarks, Clippings, Wiki**. There is **no**
  Downloads / Briefings / Social / Inbox / Agent / Identity tab (Briefings was
  removed from iOS).
- **Search** is a top-right toolbar magnifying-glass button (sheet), and that
  sheet searches the **local library + transcripts**, not a remote catalog.
- **Subscribe-by-keyword (discovery)** is the sidebar → **Add Show** → Discover
  (Apple iTunes) path (`Features/Library/{AddShowSheet,DiscoverSearchForm}`),
  where tapping a result row subscribes. Flow 01 exercises that real path.
- **Downloads** is a screen under **Settings** (sidebar → gear), not a tab.
- **Queue / Up Next** is the player's `PlayerQueueSheet`; "Add to Queue" is an
  episode-row **context-menu** (long-press) action.
- The **mini-player** is an iOS-26 `tabViewBottomAccessory`; tapping it opens
  the full `PlayerView` sheet (skip-forward/back live there).

None of the 8 P0 flows require the phantom tabs, so they map cleanly onto the
real navigation. Where the convention id maps to a real surface, the flows use
it; otherwise they use stable text. Update the id taps once the parallel agent
assigns ids to the sidebar / Add Show / queue surfaces.

## ID-placement notes for the parallel a11y-id agent

Two convention ids are reused on surfaces the convention did not explicitly
name. For these flows to bind once ids land, please tag these surfaces (or tell
us a distinct id to use):

- **`search-field` / `search-result-row`** read most naturally as the top-right
  **Search sheet** (`PodcastSearchView`'s `.searchable`). But flows 01 +
  `subscribe-darknet` use them on the **Discover / Add Show** field
  (`DiscoverSearchForm` → `DiscoverSearchTextField`, placeholder
  *"Search Apple Podcasts"*) and its iTunes result rows — a different view.
  Either add `discover-search-field` / `discover-result-row` ids there, or
  apply `search-field` / `search-result-row` to the Discover surface too. The
  flows tap these ids `optional: true` and fall back to placeholder/title text,
  so they degrade rather than hard-fail until this is resolved.
- **`home-episode-row`** is a Home-screen id, but flows 02 / 06 / 08 tap episode
  rows inside **`ShowDetailView`** (the per-podcast episode list reached by
  opening a show). Those detail rows need an episode-row id too — reuse
  `home-episode-row` on `ShowDetailView`'s list, or define
  `show-detail-episode-row` and we will switch the taps.

## Conventions used (per the task)

Tabs: `tab-home`, `tab-library` (others in the convention have no real tab).
Mini player: `mini-player-bar`, `mini-player-play-pause`, `mini-player-title`.
Full player: `player-play-pause`, `player-skip-forward`, `player-skip-backward`,
`player-speed-button`. Library: `library-podcast-list`, `library-podcast-row`.
Home: `home-episode-row`, `home-search-button`. Search: `search-field`,
`search-result-row`. Settings: `settings-root-list`. Downloads: `downloads-list`.

## Limitations

- **Network toggling (airplane mode):** Maestro has no portable command to
  toggle iOS Simulator/device connectivity. Flow 05 leaves the network toggle
  as a manual / CI-harness step (disable host networking or device Airplane
  Mode before the offline cold launch).
- **Audio + numeric clock assertions:** Maestro asserts on the view hierarchy,
  not audio output or numeric clock deltas. "Audio keeps playing", "elapsed
  advances within budget", and "skip ±15s" are flagged
  `# MANUAL VERIFICATION REQUIRED` inside the flows; the automatable proxies
  (mini-player present, Play↔Pause toggle) are asserted instead.

## Validate on first real run

A few command/selector choices can only be confirmed against a live build with
the ids installed. On the first end-to-end run, re-verify:

- **iOS back navigation** — `back` is Android-only; flow 08 uses a left-edge
  swipe (interactive pop) + a "Back" text fallback. Confirm the swipe pops the
  detail view on iOS.
- **`pressKey: Enter`** — submits the search field (was `Return`; Maestro's key
  enum uses `Enter`). Confirm the Discover/Search submit fires.
- **`pressKey: Home`** (flow 07) — confirm it backgrounds the app on the target
  Simulator/device.
- **Optional-tap chains** — in flows where the episode-open / play taps are
  `optional: true`, confirm they actually fire once ids land; otherwise a later
  required `assertVisible` (e.g. `mini-player-bar`) fails with a misleading
  error rather than at the real cause (a missing episode-row id).
