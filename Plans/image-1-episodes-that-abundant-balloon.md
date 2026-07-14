# Separate Podcast identity from Podcast subscription

## Context

User-reported bug: episodes the agent adds via `play_external_episode` show no artwork because the resolver chain `episode.imageURL ?? subscription?.imageURL` finds nothing — the agent has to remember to pass `image_url` for the artwork to live anywhere.

But the user's directive goes deeper than the bug. The current data model conflates two concepts inside `PodcastSubscription`:

1. **Podcast identity & metadata** — `feedURL`, `title`, `author`, `imageURL`, `description`, `language`, `categories`. Facts about a show.
2. **User subscription state** — `subscribedAt`, `autoDownload`, `notificationsEnabled`, `defaultPlaybackRate`, `lastRefreshedAt`, `etag`, `lastModified`. Facts about the user's relationship with the show.

`Episode.subscriptionID` points at this combined record, so the only way the system can "know about a podcast" is to have a `PodcastSubscription` row. That's why `play_external_episode` falls back to a sentinel `Episode.externalSubscriptionID` (no metadata at all) and why `AgentGeneratedPodcastService` has to mint a hacky `isAgentGenerated: true` pseudo-subscription. Both are workarounds for the missing concept of "a podcast we know about but the user hasn't followed."

**Library means episodes, not podcasts.** Episodes enter the library through three mechanisms — automatic feed polling (because the user follows), manual single-episode add, or agent-added single episodes — and all three deserve the same first-class treatment with proper artwork and metadata.

**Note on the screenshot.** The episode row in the user's screenshot ("Agent Generated / The Self-Control Files…") is technically from the TTS-generated path (its show name `"Agent Generated"` comes from the synthetic subscription's `title`, not from `play_external_episode` which discards `podcastTitle`). The user's directive — "exclusively from `play_external_episode`" — names the worse case but the underlying design rot is shared. The architectural fix below repairs both paths.

## Target model

```
Podcast                            (NEW — pure metadata)
  id: UUID
  feedURL: URL?                    (nil for synthetic kinds e.g. Agent-Generated)
  title, author, description
  imageURL: URL?
  language, categories
  kind: .rss | .synthetic          (replaces isAgentGenerated)
  discoveredAt: Date

PodcastSubscription                (slimmed — user's follow state)
  podcastID: UUID                  (FK -> Podcast.id)
  subscribedAt: Date
  autoDownload: AutoDownloadPolicy
  notificationsEnabled: Bool
  defaultPlaybackRate: Double?
  lastRefreshedAt, etag, lastModified   (HTTP polling cache; only meaningful when subscribed)

Episode
  podcastID: UUID                  (FK -> Podcast.id; renamed from subscriptionID)
  ... (rest unchanged)
```

Three rules that fall out of the split:
- **Knowing about a podcast ≠ subscribing to it.** Adding a `Podcast` row never adds a `PodcastSubscription` row.
- **Every episode has a real `podcastID`.** No more sentinel.
- **The "subscriptions" UI surface = `PodcastSubscription` rows joined with `Podcast`.** "Library" = `Episode` rows (joined with `Podcast` for display).

## Plan

### 1. New domain types

- **`App/Sources/Podcast/Podcast.swift`** — NEW. Struct with the fields above. `kind` enum (`.rss`, `.synthetic`).
- **`App/Sources/Podcast/PodcastSubscription.swift`** — slim down to the user-pref + HTTP-cache fields. `id` becomes `podcastID`. Remove `feedURL`, `title`, `author`, `imageURL`, `description`, `language`, `categories`, `isAgentGenerated` (those move to `Podcast`).

### 2. Episode field rename

- **`App/Sources/Podcast/Episode.swift`** — rename `subscriptionID` → `podcastID`. Implement a custom `init(from decoder:)` that decodes `podcastID` if present, falling back to legacy key `subscriptionID`. Custom `encode(to:)` writes the new `podcastID` key only. This is a deliberate one-way migration — once an install upgrades, the JSON/SQLite payload uses the new key forever (no downgrade path). Remove `Episode.externalSubscriptionID` sentinel constant.

### 3. AppState

- **`App/Sources/Domain/AppState.swift`** — add `var podcasts: [Podcast] = []`. Keep `subscriptions` field name but its element type is the slimmed `PodcastSubscription`. Add forward-compat decode: if a persisted file has the OLD shape (`subscriptions: [LegacyPodcastSubscription]`), call a one-shot migrator (next section) before populating `podcasts` + `subscriptions`.

### 4. Persistence migration (one-way, lossless)

- **`App/Sources/State/Persistence.swift`** — on load, if the decoded `AppState` carries a `legacy_subscriptions` shape (presence of `feedURL` / `title` keys in elements), split each legacy element into a `Podcast` (keeping the legacy UUID as `Podcast.id`) + a `PodcastSubscription(podcastID: sameUUID, ...)`. Because `Episode.subscriptionID` (now `podcastID`) values match those UUIDs, episode foreign keys keep working. Migration runs once; subsequent loads see the new shape.
- **Unknown-podcast sentinel.** Introduce `Podcast.unknownID: UUID` as a stable constant (analogous to the dropped `Episode.externalSubscriptionID`). On migration, episodes that previously pointed at the dropped sentinel are repointed to this `Podcast(kind: .synthetic, title: "Unknown", imageURL: nil)` row. The same row backs new external plays where the agent didn't supply a `feed_url` (see §7). Stable UUID keeps migration idempotent across re-runs.
- **`App/Sources/State/EpisodeSQLiteStore.swift`** — **leave the SQLite column name `subscription_id` unchanged**; only the Swift model field renames. The column is internal to the SQLite store and never user-visible; renaming it adds migration risk for zero behavior win. Document in a comment that the column semantically holds a `podcastID`.

### 5. Store APIs (`AppStateStore+*`)

- **`App/Sources/State/AppStateStore+Podcasts.swift`** — split into two files (each under the 300-line soft limit):
  - `AppStateStore+Podcasts.swift` — `podcast(id:)`, `podcast(feedURL:)`, `upsertPodcast(_:)`, `allPodcasts`.
  - `AppStateStore+Subscriptions.swift` — `subscription(podcastID:)`, `addSubscription(podcastID:)`, `removeSubscription(podcastID:)`, `subscribedPodcasts` (join helper), `setNotificationsEnabled`, `setAutoDownload`, etc.
- Existing callers of `store.subscription(id:)` are renamed/redirected. Compatibility shim `store.subscription(id:)` returns the join of `Podcast` + `PodcastSubscription?` if you want a single sweep; otherwise grep-and-update.
- **`App/Sources/State/AppStateStore+Episodes.swift`** — delete `upsertExternalEpisode`. Replace with `upsertEpisode(podcastID:audioURL:title:guid:duration:imageURL:)` which has no notion of "external."

### 6. Feed subscribe machinery split

`subscribe_podcast` currently does "resolve feed → create PodcastSubscription with metadata." Split this:

- **New helper**: `ensurePodcast(feedURL:) async throws -> Podcast` — does the HTTP fetch + parse to populate `Podcast` metadata. Idempotent: returns existing `Podcast` if `feedURL` already known. **Preserve the existing case-insensitive `feedURL.absoluteString` match** that `AppStateStore+Podcasts.swift:36` uses today, so we don't double-create podcasts that the old code would have merged.
- `subscribe_podcast` becomes: `ensurePodcast(feedURL:)` → add `PodcastSubscription` row → pull current episodes.

### 7. Agent tools

- **`App/Sources/Agent/AgentToolSchema+Podcast.swift`** — `play_external_episode` schema:
  - **Required**: `audio_url`, `title`.
  - **Optional**: `feed_url`, `duration_seconds`, `timestamp`.
  - **Drop**: `image_url`, `podcast_title`. The system resolves both from the `Podcast` record fetched via `feed_url` when supplied.
  - Tool description updated to: "When you know the show's RSS feed_url (e.g. from `search_podcast_directory`), pass it — the player will then show the show's real artwork. If you only have a raw audio URL (user-pasted link, Nostr-shared URL), omit feed_url; the episode plays under an Unknown-podcast record."
- **`App/Sources/Agent/AgentTools+PodcastExternal.swift`** — `playExternalEpisodeTool`:
  - If `feed_url` supplied → `ensurePodcast(feedURL:)` → upsert Episode under that podcast.
  - Else → upsert Episode under `Podcast.unknownID`.
  - Either path starts playback.
- **`App/Sources/Agent/LivePodcastAgentToolDeps.swift`** — `playExternalEpisode` adapter signature drops `imageURL`, `podcastTitle`; gains optional `feedURL`.
- **`App/Sources/Agent/AgentPrompt.swift`** — line 38 prompt example updated: `play_external_episode(audio_url, title, feed_url?)`, with a note that `feed_url` should come from a `search_podcast_directory` hit when available.

### 8. TTS-generated path

- **`App/Sources/Agent/AgentGeneratedPodcastService.swift`** — create an "Agent Generated" `Podcast` record (kind: `.synthetic`, `feedURL: nil`, bundled artwork or solid-color placeholder). **No automatic subscription.** The user can opt to follow it via a normal subscribe flow if/when we expose it; for now `Agent Generated` episodes simply appear in the library like any other.
- **`App/Sources/Agent/AgentTTSComposer.swift`** — set `imageURL` on the published episode from the first snippet chapter as a nice-to-have (covers shows-it-stitched-from artwork on episodes that mix sources); falls back to `Podcast.imageURL` of the synthetic podcast.

### 9. UI surfaces

Every callsite using `subscription?.imageURL`, `subscription?.title`, or `store.subscription(id:)` needs to become a `Podcast` lookup. The pattern is mechanical: where today's code reads "the subscription that owns this episode," tomorrow's code reads "the podcast that owns this episode." Files identified in the survey (representative, not exhaustive):

- Home: `HomeAgentPickCard.swift`, `HomeContinueListeningSection.swift`, `HomeResumeCard.swift`, `HomeSubscriptionRow.swift`, `HomeSubscriptionListSection.swift`, `HomeFeaturedSection.swift`.
- Library: `EpisodeRow.swift`, `LibraryGridCell.swift`, `ShowDetailHeader.swift`, `ShowDetailEpisodeList.swift`.
- Episode/Player: `EpisodeDetailView.swift`, `EpisodeDetailHeroView.swift`, `PlayerView.swift`, `MiniPlayerView.swift`, `PlayerClipSourceChip.swift`, `EpisodeRowContextMenu.swift`.
- Agent surfaces: `AgentChatEmptyViews.swift`, `LivePodcastInventoryAdapter.swift`, `LivePodcastRAGAdapter.swift`, `AgentTTSComposer.swift:230-235` (the snippet-chapter artwork resolver — same `ep.imageURL ?? store.subscription(...)?.imageURL` shape; becomes `... ?? store.podcast(id: ep.podcastID)?.imageURL`).
- Settings/Downloads/Categories: `DownloadsManagerView.swift`, `CategoriesListView.swift`, `CategoryDetailView.swift`, `SettingsView.swift`, `DataStorageSettingsView.swift`, `StorageSettingsView.swift`, `CategoriesRecomputeSheet.swift`.

The "subscription list" sections on Home (`HomeSubscriptionListSection` / `HomeSubscriptionRow`) iterate over `PodcastSubscription` rows joined with their `Podcast` — only podcasts the user follows show up there. The new model removes the `isAgentGenerated` exclusion filter on the subscription list (`AppStateStore+Sorting.swift:22`) because Agent-Generated is now a `Podcast` with no subscription row, not a special-cased subscription.

`EpisodeDetailView.swift:118` (`isExternal = episode.subscriptionID == Episode.externalSubscriptionID`) becomes `isFollowed = store.subscription(podcastID: episode.podcastID) != nil` — the "external" notion goes away.

### 10. OPML import/export, feed refresh, notifications

- **`App/Sources/Services/DataExport.swift`** — OPML export iterates `subscribedPodcasts` (the join), not all podcasts. Behavior unchanged for users.
- Feed refresh scheduler reads `state.subscriptions` (rows), joins to `state.podcasts` for the feed URL + etag/lastModified. Unfollowed podcasts never get polled.
- Notifications: same. Only subscribed.

### 11. Changelog

- **`App/Resources/whats-new.json`**: `"Agent-added episodes now carry full podcast artwork and metadata — no more blank tiles."`

## Critical files

| File | Change |
|------|--------|
| `App/Sources/Podcast/Podcast.swift` | **NEW** — metadata-only podcast |
| `App/Sources/Podcast/PodcastSubscription.swift` | Slim to user-prefs + HTTP cache, `podcastID` FK |
| `App/Sources/Podcast/Episode.swift` | `subscriptionID` → `podcastID`; drop sentinel |
| `App/Sources/Domain/AppState.swift` | Add `podcasts: [Podcast]`; legacy decode shim |
| `App/Sources/State/Persistence.swift` | One-shot migration: split legacy subscriptions |
| `App/Sources/State/EpisodeSQLiteStore.swift` | Column rename `subscription_id` → `podcast_id` |
| `App/Sources/State/AppStateStore+Podcasts.swift` | Split into Podcasts + Subscriptions |
| `App/Sources/State/AppStateStore+Episodes.swift` | Replace `upsertExternalEpisode`; no sentinel |
| `App/Sources/Agent/AgentToolSchema+Podcast.swift` | Drop `image_url`/`podcast_title`; add `feed_url` |
| `App/Sources/Agent/AgentTools+PodcastExternal.swift` | Route through `ensurePodcast(feedURL:)` |
| `App/Sources/Agent/LivePodcastAgentToolDeps.swift` | Adapter signature change |
| `App/Sources/Agent/AgentPrompt.swift` | Prompt example update |
| `App/Sources/Agent/AgentGeneratedPodcastService.swift` | Create `Podcast` (kind: `.synthetic`); no auto-subscription |
| `App/Sources/Agent/AgentTTSComposer.swift` | Pass first-chapter art as episode `imageURL` |
| All UI surfaces listed in §9 | Mechanical rename `subscription?.xxx` → `podcast?.xxx` |
| `App/Resources/whats-new.json` | Changelog line |

## Out of scope (explicit follow-ups)

- **Library surface for unfollowed podcasts.** This plan ensures the data lives in `Podcast`, but doesn't add a new UI for "podcasts I have episodes from but don't follow." If the user wants a "Recently played shows" section that lists podcasts even without a subscription, that's a separate UX pass.
- **Show-detail "Follow" button.** With the split, `ShowDetailHeader` could naturally gain a follow/unfollow toggle for podcasts that don't have a subscription yet. Worth a follow-up but not in this PR.
- **Cleanup of orphan podcasts.** When the user clears a single agent-added episode, the parent `Podcast` row may be left dangling if it has no subscription and no other episodes. A periodic GC pass (or on-delete cascade) can prune; not urgent.

## Verification

1. **Migration test** — load an existing AppState JSON (pre-split shape) and assert it round-trips into `(podcasts, subscriptions)` with episode FKs intact. Add a fixture-backed test next to `WhatsNewServiceTests` style.
2. **Build** — `mcp__xcode__build_run_sim`.
3. **Existing flows unchanged** — verify subscribed shows (Joe Rogan, Huberman, etc.) still render artwork and titles everywhere on Home/Library/Player.
4. **External episode flow (with feed_url)** — agent calls `play_external_episode(audio_url, title, feed_url)` for a show the user does NOT follow:
   - Episode plays.
   - Episode appears in Continue Listening with the source podcast's real artwork and title — no waveform.
   - Source podcast does NOT appear in the user's subscription list on Home.
   - Featured agent pick rail renders the episode with proper artwork.
5. **External episode flow (no feed_url)** — agent calls `play_external_episode(audio_url, title)` with no feed (e.g., a user-pasted link):
   - Episode plays.
   - Episode appears in Continue Listening parented to the `Unknown` podcast with a generic placeholder tile and "Unknown" as the show name. No crash, no orphan.
6. **Promote-to-follow** — after the external play, agent calls `subscribe_podcast(feed_url)` for the same feed. The existing `Podcast` row is reused (no duplicate), a `PodcastSubscription` row is added, and the show now appears in the subscription list. Existing episode keeps its `podcastID`.
7. **TTS path** — agent generates a stitched episode; it appears in the library with proper synthetic-podcast artwork.
8. **Tests** — run `mcp__xcode__test_sim`; expect updates needed for any test referencing `PodcastSubscription(feedURL:title:imageURL:...)` (they'll need to switch to creating `Podcast` + `PodcastSubscription` pairs). Repair as part of the change.

## Scope note

This is a deliberately larger change than the surface bug because the user explicitly called for re-architecture. The mechanical UI rename in §9 is the bulk of the line count but is genuinely mechanical. The conceptual core is §1–§4 (model split + migration); everything else flows from there.

There is a natural seam at the boundary of §1–§7 (model + migration + agent tools) vs. §9 (mechanical UI rename). If staging is preferred over a single PR, the first half can land behind a temporary `store.subscription(id:)` shim that returns the joined `Podcast + PodcastSubscription?` view so existing UI code compiles unchanged; the UI rename then ships as a follow-up with no behavior change. Default plan is to do it in one pass since the user asked for the rearchitecture explicitly.
