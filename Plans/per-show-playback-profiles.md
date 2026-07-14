# Per-Show Playback Profiles + LLM-Detected Intro/Outro Skip — Implementation Plan

## Executive summary

- **Profile is trimmed to two fields:** `speed: Double?` and `autoPlayNext: Bool?`. `voiceBoost` is dropped entirely; intro/outro are no longer profile fields. The orphan `defaultPlaybackRate` migration (→ `profile.speed`) still stands.
- **Intro/outro reframed as per-episode LLM-detected markers**, mirroring the existing ad-segment pipeline. `AIChapterCompiler` already takes one transcript pass and emits chapters + summaries + ads in a single LLM call; we extend its prompt schema to also emit `introEnd` and `outroStart` scalars. Stored on `Episode`, not on the profile.
- **Auto-skip wiring mirrors `PlaybackState+AdSkip.swift` verbatim:** a new `PlaybackState+IntroOutroSkip.swift` plugs into the same 1-second `tickPersistence` loop. Intro skip = `engine.seek(to: introEnd)`. Outro skip = fire `onEpisodeFinished` (respects `autoMarkPlayedOnFinish`, `autoPlayNext`, and `endOfEpisode` sleep timer mode for free).
- **Per-show angle for intro/outro lands in commit 5**, when the profile expands to add `autoSkipIntro: Bool?` and `autoSkipOutro: Bool?`. Commit 4's editor ships with two rows (speed, autoPlayNext); commit 5 adds the two skip toggles when the detection fields exist to bind to. Flagged in §8 for confirmation.
- **One existing bug surfaced:** `PlaybackState.adSegments` is refreshed only in `setEpisode`, so detection that finishes after episode load doesn't reach the auto-skip loop in the current session. Intro/outro would inherit it; recommend a `RootView` `.onChange` bridge that pushes both `adSegments` and the new intro/outro fields into `PlaybackState` when the live episode model changes.

---

## 1. File-level inventory

### Files to edit

| Path | Change shape |
|---|---|
| `App/Sources/Podcast/PodcastSubscription.swift` | Add `playbackProfile: ShowPlaybackProfile?`. `CodingKeys` + `init(from:)` + `encode(to:)`. Migrate legacy `defaultPlaybackRate` → `ShowPlaybackProfile(speed: ...)` in `init(from:)`. Keep `defaultPlaybackRate` decodable for one release; stop writing it. ~25 lines. |
| `App/Sources/Podcast/Episode.swift` | Add `introEnd: TimeInterval?` and `outroStart: TimeInterval?` fields. Add to `CodingKeys` + `init(from:)` with `decodeIfPresent`. Also add to `init(...)`. ~15 lines. (No `introOutroDetected` flag — old episodes with cached `adSegments` are intentionally skipped per design decision; both-nil is the correct read for "not detected, won't auto-skip" regardless of cause.) |
| `App/Sources/Features/Player/PlaybackState.swift` | (a) Add `currentSubscription: PodcastSubscription?` and `resolveSubscription: (Episode) -> PodcastSubscription?` injected closure. (b) Change `applyPreferences(from settings:)` → `applyPreferences(from settings: Settings, subscription: PodcastSubscription?)` — read `profile.speed` (fresh-load only) and add `autoPlayNextOverride: Bool?`. (c) Add `introEnd: TimeInterval?`, `outroStart: TimeInterval?`, `autoSkipIntroEnabled: Bool`, `autoSkipOutroEnabled: Bool` fields, refreshed in `setEpisode` and via the new `RootView` `.onChange` bridge. (d) Add `skippedIntroForCurrentEpisode: Bool` (session-scoped, cleared on episode change like `skippedAdSegmentIDs`). (e) Call new `applyAutoSkipIntroOutroIfNeeded(at:)` from `tickPersistence` next to `applyAutoSkipAdsIfNeeded`. ~40 lines. Stays under 500-line cap (currently ~452). |
| `App/Sources/Services/AIChapterCompiler.swift` | (a) Extend both system prompts (`systemPromptFull`, `systemPromptEnrichOnly`) to include `intro_end` and `outro_start` keys in the JSON schema, with detection rules. (b) Extend `parseFull` / `parseEnrichOnly` payload structs and return types to surface `(introEnd, outroStart)`. (c) Persist via a new store call `setEpisodeIntroOutroMarkers(...)`. (d) **Idempotence gate is unchanged** — `episode.adSegments == nil` still gates the call. Episodes already compiled stay as-is (no backfill); only newly-compiled episodes get intro/outro markers. ~40 lines. |
| `App/Sources/State/AppStateStore+Episodes.swift` | In the RSS merge path (line 96 area), preserve `introEnd` and `outroStart` the same way `adSegments` is preserved. ~3 lines. |
| `App/Sources/App/RootView.swift` | (a) Wire `playbackState.resolveSubscription` in `.onAppear`. (b) Replace `applyPreferences(from: store.state.settings)` calls with the new two-arg form. (c) Add `.onChange(of: store.subscription(id: state.episode?.subscriptionID)?.playbackProfile)` so a mid-playback profile edit reapplies. (d) Add `.onChange(of: store.episode(id: playbackState.episode?.id))` to bridge fresh `adSegments` / `introEnd` / `outroStart` from the live store into `PlaybackState` when AIChapterCompiler finishes after `setEpisode` (also fixes the existing ad-bridge bug — see §8 #1). (e) Update `onEpisodeFinished` autoplay gate to read `playbackState.autoPlayNextOverride ?? store.state.settings.autoPlayNext`. ~25 lines. |
| `App/Sources/Features/Player/PlayerSpeedSheet.swift` | Add "Save as default for *Show Name*" button at the bottom. Calls a new `store.setSubscriptionPlaybackProfileSpeed(id, speed:)`. Hidden when no subscription context. ~30 lines. |
| `App/Sources/State/AppStateStore+Podcasts.swift` | Add `setSubscriptionPlaybackProfile(_ id: UUID, profile: ShowPlaybackProfile?)`, `setSubscriptionPlaybackProfileSpeed(_ id: UUID, speed: Double?)`, plus convenience setters for `autoPlayNext`, `autoSkipIntro`, `autoSkipOutro`. Mirror `setSubscriptionAutoDownload`. ~30 lines. |
| `App/Sources/Features/Library/ShowDetailView.swift` | In `ShowDetailSettingsSheet`, add "Playback" section with `NavigationLink` to `ShowPlaybackProfileEditor`. Summary line: e.g. "1.5× · Auto-play on" when set, "Defaults" otherwise. ~15 lines. |

### Files to create

| Path | Purpose |
|---|---|
| `App/Sources/Podcast/ShowPlaybackProfile.swift` | The trimmed `ShowPlaybackProfile` struct: just `speed` and `autoPlayNext` in commit 1; expands in commit 5 with `autoSkipIntro` and `autoSkipOutro`. ~40 lines initially, ~60 after commit 5. |
| `App/Sources/Features/Library/ShowPlaybackProfileEditor.swift` | Editor screen. Commit 4: speed picker + autoPlayNext toggle + Reset. Commit 5: adds two skip toggles. ~120 lines total after commit 5. |
| `App/Sources/State/AppStateStore+IntroOutroMarkers.swift` | New persistence shim for `setEpisodeIntroOutroMarkers(id:introEnd:outroStart:)`. Mirrors `AppStateStore+AdSegments.swift` exactly. ~30 lines. |
| `App/Sources/Features/Player/PlaybackState+IntroOutroSkip.swift` | The skip extension. Mirrors `PlaybackState+AdSkip.swift`. Two functions: one that handles intro (seek), one that handles outro (fire finished). Both called from `tickPersistence`. ~40 lines. |
| `AppTests/Sources/ShowPlaybackProfileTests.swift` | Codable migration + resolver tests (~80 lines). |
| `AppTests/Sources/IntroOutroDetectionTests.swift` | Prompt-parse + auto-skip-loop tests (~100 lines). |

### Files NOT touched (intentional)

- `App/Sources/Audio/AudioEngine.swift` — no per-show skip-interval override anymore (button intervals stay global), so engine wiring is unchanged.
- `App/Sources/Audio/NowPlayingCenter.swift` — same.
- `App/Sources/Services/iCloudSettingsSync.swift` — `PodcastSubscription` isn't iCloud-synced; out of scope.
- `App/Sources/Features/Settings/PlaybackSettingsView.swift` — no global "auto-skip intros" toggle; per-show only by design.

---

## 2. Model placement

### 2a. `ShowPlaybackProfile` — nested on `PodcastSubscription`

Decision unchanged from the previous draft. `defaultPlaybackRate: Double?` is the precedent; persistence path is shared; cleanup-on-unsubscribe is free.

**Shape (commit 1):**

```swift
struct ShowPlaybackProfile: Codable, Hashable, Sendable {
    var speed: Double?           // nil → fall back to Settings.defaultPlaybackRate
    var autoPlayNext: Bool?      // nil → fall back to Settings.autoPlayNext

    var isEmpty: Bool { speed == nil && autoPlayNext == nil }
}
```

**Shape after commit 5 (when intro/outro detection ships):**

```swift
struct ShowPlaybackProfile: Codable, Hashable, Sendable {
    var speed: Double?
    var autoPlayNext: Bool?
    var autoSkipIntro: Bool?     // nil → off
    var autoSkipOutro: Bool?     // nil → off

    var isEmpty: Bool {
        speed == nil && autoPlayNext == nil
            && autoSkipIntro == nil && autoSkipOutro == nil
    }
}
```

The growth across commits is forward-compat — additive optional fields decode silently with `decodeIfPresent`. Commit 4 ships the editor with two rows; commit 5 adds two more rows in the same screen.

### 2b. Intro/outro markers — scalars on `Episode`, NOT `AdKind` extensions

**Why not extend `AdKind` with `.intro` / `.outro`:**

- **Outro auto-skip is "end the episode," not "seek past a span."** Pocket Casts treats it that way; it composes correctly with `autoMarkPlayedOnFinish`, `autoPlayNext`, and the `endOfEpisode` sleep-timer mode already plumbed through `onEpisodeFinished`. Reusing `AdSegment` would force the skip code into `engine.seek(to: end)` which is wrong shape for the outro case.
- **Semantics:** ads = commercial content (host-read sponsor copy); intro/outro = show structure (cold open, theme music, sign-off). Mixing them in `adSegments` makes existing readers wrong: `Settings.autoSkipAds` would silently auto-skip intros for users who never opted in; `PlayerPrerollSkipButton.activePrerollSegment` could surface "Skip 30s ad" over a podcast intro; `Chapter.overlapsAd` would mark chapters that overlap the intro as ad-overlapping (amber stripe).
- **The user's spec said "ask the LLM to flag those times like we do with ads"** — that's about the *detection pipeline* (same LLM, same prompt, same JSON response), not the *storage schema*. We honor the pipeline reuse, not the schema reuse.

**Shape on `Episode`:**

```swift
var introEnd: TimeInterval?      // seconds; nil = no intro detected, don't auto-skip
var outroStart: TimeInterval?    // seconds; nil = no outro detected, don't auto-skip
```

No `introOutroDetected` flag — by design decision, old episodes with cached `adSegments` are not backfilled. Both-nil is the correct read for "don't auto-skip" regardless of whether detection ran and found nothing or never ran at all.

---

## 3. Persistence path

### Profile

Additive `decodeIfPresent` on `playbackProfile`; migrate orphan `defaultPlaybackRate` in `PodcastSubscription.init(from:)` when `playbackProfile == nil && defaultPlaybackRate != nil`. No persistence version bump.

### Episode intro/outro markers

Same additive pattern. New keys on `CodingKeys`:

```swift
case introEnd, outroStart
```

Both decode via `decodeIfPresent` defaulting to `nil`. Episodes pre-dating this feature decode silently as nil and stay that way — they will NOT be re-evaluated by `AIChapterCompiler` because the gate (`episode.adSegments == nil`) is unchanged.

**RSS merge preservation** (`AppStateStore+Episodes.swift` line 96): the existing read-modify-write loop already preserves `adSegments`, `playbackPosition`, etc. when a feed refresh brings an updated episode. Extend the same preservation to `introEnd` and `outroStart` — RSS feeds never carry these.

---

## 4. PlaybackState integration

### Profile wiring (commit 2)

1. **`resolveSubscription` closure** on `PlaybackState`, default no-op; wired by `RootView.onAppear` to `store.subscription(id: episode.subscriptionID)`.
2. **`currentSubscription`** cached in `setEpisode` after the resolve.
3. **`applyPreferences(from settings:, subscription:)`** — sets `engine.setRate(profile.speed ?? settings.defaultPlaybackRate)` only when `engine.episode == nil`. Sets `autoPlayNextOverride = profile.autoPlayNext`. Skip intervals untouched (button durations stay global).
4. **`onEpisodeFinished`** in `RootView` reads `playbackState.autoPlayNextOverride ?? store.state.settings.autoPlayNext`.

### Intro/outro skip wiring (commit 5)

New extension `PlaybackState+IntroOutroSkip.swift`:

```swift
extension PlaybackState {
    func applyAutoSkipIntroOutroIfNeeded(at time: TimeInterval) {
        if autoSkipIntroEnabled,
           let introEnd, time < introEnd, !skippedIntroForCurrentEpisode {
            skippedIntroForCurrentEpisode = true
            engine.seek(to: introEnd)
            return
        }
        if autoSkipOutroEnabled,
           let outroStart, time >= outroStart,
           let id = episode?.id,
           !engine.didReachNaturalEnd {
            // Fire end-of-episode early. tickPersistence's natural-end
            // branch handles the rest (autoMarkPlayedOnFinish,
            // autoPlayNext, sleep-timer end-of-episode mode).
            engine.seek(to: duration) // make natural-end true
        }
    }
}
```

- **Intro:** identical pattern to `applyAutoSkipAdsIfNeeded` — one-shot per session via `skippedIntroForCurrentEpisode`, so a user scrubbing back into the intro isn't yanked forward again. Reset alongside `skippedAdSegmentIDs` in `setEpisode`.
- **Outro:** seek to `duration` rather than calling `onEpisodeFinished` directly. This routes through `engine.didReachNaturalEnd` → `tickPersistence`'s existing end-detection branch (line 443), which respects `autoMarkPlayedOnFinish` and the `endOfEpisode` sleep-timer gating that `RootView.onEpisodeFinished` already handles. Single code path; no duplicated end-of-episode logic.

Called from `tickPersistence` right next to `applyAutoSkipAdsIfNeeded(at: time)`.

### Bridging fresh detection results into `PlaybackState` (commit 5, fixes pre-existing bug)

`PlaybackState.adSegments` (line 159) is currently set only in `setEpisode` (line 248). `AIChapterCompiler.compileIfNeeded` runs in `PlayerView.task(id:)` *after* setEpisode and writes back to the store — but `PlaybackState`'s local cache is never refreshed. So today, a fresh ad-skip detected mid-playback doesn't take effect until the next episode load. Intro/outro inherits this if we mirror verbatim.

Fix in `RootView`:

```swift
.onChange(of: store.episode(id: playbackState.episode?.id ?? UUID())) { _, fresh in
    guard let fresh else { return }
    playbackState.adSegments = fresh.adSegments ?? []
    playbackState.introEnd = fresh.introEnd
    playbackState.outroStart = fresh.outroStart
}
```

Cheap reactive bridge. Surface in §8 risks because the ad bug is preexisting — fix may belong in its own micro-commit before the intro/outro work, depending on review preference.

---

## 5. AIChapterCompiler extension (commit 5)

**Prompt schema extension (both `systemPromptFull` and `systemPromptEnrichOnly`):**

```json
{
  "chapters": [...],
  "ads": [...],
  "intro_end": <seconds or null>,
  "outro_start": <seconds or null>
}
```

**Rule text appended to both prompts:**

```
Intro/outro rules:
  - "intro_end" is the second the show's intro (theme music, cold open, "Welcome to…") ends and the main content begins. Null when there is no distinct intro.
  - "outro_start" is the second the outro (sign-off, credits, "next time on…") begins. Null when there is no distinct outro.
  - Do NOT use these for ads — ads go in the "ads" array.
  - "intro_end" should generally be under 120 seconds; "outro_start" should generally be in the last 5% of the episode.
  - Both null is the right answer for episodes with no clear structural framing.
```

**Parse path:**

Extend `Payload` decoder in `parseFull` / `parseEnrichOnly`:

```swift
let intro_end: Double?
let outro_start: Double?
```

Clamp to `[0, duration]`. Reject `intro_end >= duration / 2` (defensive — LLMs occasionally hallucinate a "whole episode is intro" answer). Reject `outro_start <= duration / 2` similarly. Pass results to `store.setEpisodeIntroOutroMarkers(...)`.

**Idempotence gate is unchanged.** The existing `guard episode.adSegments == nil` gate stays. Old episodes that already have `adSegments` populated will NOT re-run — they stay bare of intro/outro markers (`introEnd == nil`, `outroStart == nil`), which the auto-skip code reads correctly as "don't skip." Only newly-compiled episodes pick up the new fields. Decision logged in §8 #2.

**`setEpisodeIntroOutroMarkers`** in the new `AppStateStore+IntroOutroMarkers.swift`:

```swift
@MainActor
func setEpisodeIntroOutroMarkers(
    _ id: UUID,
    introEnd: TimeInterval?,
    outroStart: TimeInterval?
) {
    guard let idx = state.episodes.firstIndex(where: { $0.id == id }) else { return }
    var episodes = state.episodes
    episodes[idx].introEnd = introEnd
    episodes[idx].outroStart = outroStart
    state.episodes = episodes
}
```

---

## 6. Remote command integration

Nothing to do. Per-show skip-button intervals were dropped, so `MPRemoteCommandCenter.preferredIntervals` plumbing stays as-is. Intro/outro is silent auto-skip — no lock-screen affordance needed in v1. A future "Skip intro" chip near the scrubber (mirroring `PlayerPrerollSkipButton`) is optional follow-up; not in this batch.

---

## 7. Test strategy

### `AppTests/Sources/ShowPlaybackProfileTests.swift` (commit 1–2)

1. **`testLegacyDefaultPlaybackRateMigratesToProfile`** — decode JSON with `defaultPlaybackRate: 1.5`, assert `subscription.playbackProfile?.speed == 1.5`.
2. **`testProfileCodableRoundtrip`** — encode/decode a full profile.
3. **`testApplyPreferencesPrefersProfileSpeedOnFreshLoad`** — `PlaybackState` with empty engine, settings `defaultPlaybackRate=1.0`, profile `speed=1.5`. `applyPreferences(from:subscription:)` → `engine.rate == 1.5`.
4. **`testApplyPreferencesFallsBackToSettingsWhenProfileNil`** — subscription with `playbackProfile == nil`. Engine rate = settings default.
5. **`testAutoPlayNextOverrideReadable`** — profile with `autoPlayNext = false`, assert `playbackState.autoPlayNextOverride == false`.

### `AppTests/Sources/IntroOutroDetectionTests.swift` (commit 5)

1. **`testParseFullExtractsIntroOutroMarkers`** — feed `AIChapterCompiler.parseFull` a JSON blob with `intro_end: 45`, `outro_start: 3540`, assert the result.
2. **`testParseFullClampsOutOfRange`** — `intro_end: 9999` on a 1800s episode clamps to 1800 or nils out per the half-episode defensive rule.
3. **`testApplyAutoSkipIntroOutroIfNeededSkipsIntro`** — set `introEnd = 60`, `autoSkipIntroEnabled = true`, call with `time = 5`, assert `engine.seek(to: 60)` was invoked and `skippedIntroForCurrentEpisode == true`.
4. **`testApplyAutoSkipIntroOutroIfNeededOneShotPerSession`** — same setup, second call with `time = 30` should not seek again.
5. **`testOutroSkipFiresEndOfEpisode`** — set `outroStart = 1700` on a 1800s episode, `autoSkipOutroEnabled = true`, call with `time = 1700`, assert `engine.seek(to: ~1800)` was invoked (which triggers the existing natural-end branch on the next tick).
6. **`testIntroOutroNotSkippedWhenProfileToggleOff`** — `autoSkipIntroEnabled = false`, assert no-op.

---

## 7b. Order of operations (each commit independently compiles + ships value)

### Commit 1 — Trimmed model + migration

- Add `ShowPlaybackProfile.swift` with `speed` + `autoPlayNext` only.
- Add `playbackProfile` field on `PodcastSubscription` with Codable migration of orphan `defaultPlaybackRate`.
- Add `ShowPlaybackProfileTests` (Codable + migration cases).
- No callers yet — field is dormant.
- **Whats-new:** skip (internal-only).

### Commit 2 — Resolver + `applyPreferences` wiring + `RootView` autoplay gate

- Add `resolveSubscription` closure and `currentSubscription` to `PlaybackState`.
- Change `applyPreferences` to two-arg form. Add `autoPlayNextOverride`.
- Update `RootView.onAppear` (resolver + applyPreferences calls) and `onEpisodeFinished` (autoplay gate reads override first).
- Add the `applyPreferences` integration tests.
- **Behavior change:** any user with a legacy `defaultPlaybackRate` set now actually sees it apply on episode load (today: dead data).
- **Whats-new:** "Per-show playback speed (if previously set) now applies when you start an episode."

### Commit 3 — Store setters + `PlayerSpeedSheet` "Save as default"

- `AppStateStore+Podcasts` setters: `setSubscriptionPlaybackProfile`, `setSubscriptionPlaybackProfileSpeed`, `setSubscriptionPlaybackProfileAutoPlayNext`.
- Bottom-anchored button in `PlayerSpeedSheet`, visible only when subscription resolves; haptic + toast on save.
- **Whats-new:** "Save a default playback speed for each show right from the speed sheet."

### Commit 4 — `ShowDetailSettingsSheet` row + `ShowPlaybackProfileEditor`

- New "Playback" section in `ShowDetailSettingsSheet` with a `NavigationLink`.
- Editor: speed picker, `autoPlayNext` toggle, "Reset to defaults" button.
- ~3 rows, ~120 lines. Stays well under any size cap.
- **Whats-new:** "Long-press a show → 'Settings for this show' → 'Playback' to set per-show speed and auto-play."

### Commit 5 — Intro/outro detection sub-feature

This is the biggest commit and may split if review prefers. Order within:

- **5a (data model):** Add `introEnd` and `outroStart` to `Episode` + Codable migration. Preserve in RSS merge. Add `AppStateStore+IntroOutroMarkers.swift`.
- **5b (detection):** Extend `AIChapterCompiler` prompts + parsers + idempotence gate. Persist via the new setter.
- **5c (playback wiring):** Add intro/outro fields and toggles to `PlaybackState`. Add `PlaybackState+IntroOutroSkip.swift`. Call from `tickPersistence`. Reset `skippedIntroForCurrentEpisode` in `setEpisode`.
- **5d (live-data bridge):** Add the `RootView` `.onChange(of: store.episode(id:))` bridge that pushes fresh `adSegments` + intro/outro markers into `PlaybackState` (this also fixes the preexisting ad-skip-after-detect bug — call out in PR description).
- **5e (per-show UI):** Extend `ShowPlaybackProfile` with `autoSkipIntro: Bool?` and `autoSkipOutro: Bool?`. Add the two toggles to `ShowPlaybackProfileEditor`. Add `setSubscriptionPlaybackProfileAutoSkipIntro/Outro` store setters. Wire `autoSkipIntroEnabled` / `autoSkipOutroEnabled` flags in `PlaybackState.applyPreferences` from the profile.
- **Whats-new:** "Auto-skip intros and outros: in each show's playback settings, toggle on and the player will jump past detected intros and end episodes early when the outro begins."

5a–5b together produce data but no behavior. 5c–5d wire playback. 5e adds the per-show toggles. Each sub-commit compiles. Reviewer may prefer them merged; suggested split is for diff readability.

---

## 8. Risks and open questions

1. **Preexisting bug: `PlaybackState.adSegments` stale-after-detect** — **DECIDED: bundle fix into commit 5d.** `AIChapterCompiler.compileIfNeeded` runs from `PlayerView.task(id:)` after `setEpisode` and writes to the store, but `PlaybackState`'s local cache is set only inside `setEpisode` (line 248). So ads detected mid-session never reach the auto-skip loop until the next episode load. Intro/outro inherits verbatim. The `RootView` `.onChange(of: store.episode(id: state.episode?.id))` bridge in commit 5d fixes both. Single change, single commit. Call out in PR description so reviewers see the incidental fix.

2. **Idempotence: old episodes are intentionally skipped — DECIDED.** Episodes that already have `adSegments` populated will NOT be re-run through the LLM to backfill intro/outro. They stay with `introEnd == nil` and `outroStart == nil`, which auto-skip reads correctly as "don't skip." Only newly-compiled episodes pick up the new markers. Saves LLM spend; accepted trade-off is that subscribers' back-catalog has no intro/outro auto-skip until those episodes get re-published or manually re-compiled.

3. **Profile field growth across commits.** Commit 1 ships the profile at 2 fields. Commit 5 grows it to 4 fields. The Codable migration is forward-compat by construction (additive optionals, `decodeIfPresent`) — devices that download commit 1, then commit 5, then commit 1 again (downgrade via TestFlight) will see commit 1 ignore the two new keys, which is the correct degrade. Confirm this growth pattern is acceptable rather than "profile must be final on day one." If you want all 4 fields in commit 1, the editor in commit 4 needs to gracefully degrade the two skip toggles (e.g. show them disabled with a "Coming with intro/outro detection" footer) until commit 5 lands.

4. **Outro semantics: fire end-of-episode early — DECIDED.** When the user crosses `outroStart`, the implementation seeks to `duration`, which routes through `engine.didReachNaturalEnd` → existing `onEpisodeFinished` path. Composes with `autoMarkPlayedOnFinish`, `autoPlayNext` (and its per-show override), and the `endOfEpisode` sleep-timer mode for free. No separate end-of-episode code path needed.

5. **Intro re-skip when scrubbing.** Today's ad-skip extension throttles to one skip per `AdSegment.id` per session via `skippedAdSegmentIDs`. Mirror exactly for intros: one auto-skip per episode-session, reset in `setEpisode`. So a user who scrubs back to t=10 after the auto-skip fires can rewatch the intro without being yanked forward. Same UX promise the ad path makes today.

6. **`setSubscriptionPlaybackProfile` wholesale-replace risk.** `SubscriptionRefreshService` calls `state.subscriptions[idx] = updated` (line 91 of `AppStateStore+Podcasts.swift`) on feed refresh — if `updated` doesn't carry the existing `playbackProfile`, the user's saved profile dies on the next feed poll. Spot-check that `SubscriptionRefreshService` does read-modify-write or preserves the field explicitly before merging. If it currently doesn't, fix in commit 2.

7. **LLM detection quality.** Same risk as ad detection today: a misfire seeks past actual content. Mitigations: (a) keep both per-show toggles OFF by default — opt-in only; (b) the one-shot-per-session throttle on intro means a user can scrub back to recover; (c) outro defensive clamp (`outroStart > duration/2`) catches the worst LLM hallucinations. No global "auto-skip intros" toggle in Settings — per-show only forces a deliberate opt-in per show. If detection quality proves bad, the toggles stay off and nothing user-facing breaks. Same risk surface as the existing `Settings.autoSkipAds` toggle.

8. **iCloud sync of profiles.** Out of scope. `iCloudSettingsSync` only handles `Settings`. Per-show profiles and intro/outro markers (on `Episode`) don't sync across devices today. Follow-up if desired — separate design exercise.

9. **Whats-new entries.** Per AGENTS.md, each user-facing commit needs an entry in `App/Resources/whats-new.json`. Commit 1 is internal-only (no entry); commits 2, 3, 4, and 5 each get one. Impl agent should keep entries short and user-meaningful.

10. **Editor file size after commit 5.** `ShowPlaybackProfileEditor.swift` with four rows + reset + footers should land around 130–150 lines. Well under 300-line soft cap.

---

## Critical files for implementation

- `App/Sources/Podcast/PodcastSubscription.swift`
- `App/Sources/Podcast/Episode.swift`
- `App/Sources/Features/Player/PlaybackState.swift`
- `App/Sources/Services/AIChapterCompiler.swift`
- `App/Sources/App/RootView.swift`

Supporting (read-only, for reference):
- `App/Sources/Features/Player/PlaybackState+AdSkip.swift` (mirror exactly for intro/outro skip)
- `App/Sources/State/AppStateStore+AdSegments.swift` (mirror exactly for intro/outro markers store)
- `App/Sources/Features/Player/PlayerPrerollSkipButton.swift` (mirror if you decide to add a "Skip intro" chip in a follow-up)
- `App/Sources/State/AppStateStore+Episodes.swift` (preserve new fields in RSS merge at line ~96)
