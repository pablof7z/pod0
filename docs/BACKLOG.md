# Backlog

This is the tactical queue for active work, follow-ups, and pending decisions.
Do not duplicate these items in `WIP.md`; `WIP.md` only records branches and
worktrees currently in flight.

## Active

- **P0 - Pod0 rename.** ~~Rename the working app identity from Podcastr to Pod0 where users or generated project surfaces see the app name.~~ Done via PR #52 — stable identifiers preserved.
- **P0 - NIP-F4 owned podcast publishing.** Implement `docs/plan/pod0-nostr-publishing.md`: per-podcast keys, kind `10154` show events, kind `54` episode events, kind `10064` author claims, and deletion cleanup.
- **P0 - NIP-F4 discovery.** Update discovery parsing and episode fetches for kind `10154`/`54`, no `d` tags, and stable UUID derivation from `10154:<podcast-pubkey>`.

## AppIntents / Siri follow-ups

- **appintents-siri-rust-policy** — `PodcastAppIntents.swift` currently selects "latest unplayed episode" in iOS; per D7, policy belongs in Rust. Register `SiriActionModule` in `nmp_app_podcast_register` and add a `podcast.siri.play_latest` action that resolves the episode server-side. iOS intent then dispatches `podcast.siri.play_latest` with no args.
- **appintents-skip-forward-op** — Skip-forward intent hard-codes 30s. Add a `podcast.player.skip_forward { secs: f64 }` Rust action (rather than client-side seek offset math) so the skip interval can come from user settings and be shared with CarPlay/HW remotes.

## Player follow-ups

- **speed-chip-clamp-mismatch** — ~~Fixed via PR #55: raised PlayerActor clamp + AudioCapability clamp to 3.0×.~~

## Episode ID stability (P0 bug)

- **episode-id-stability** — `Episode::new` generates a fresh random `EpisodeId` (UUID) on every RSS parse. This means the same episode gets a new ID on every feed refresh, which breaks: (1) playback position persistence (`position_secs` keyed by ID), (2) download path lookups (`local_paths` in the store), (3) auto-download deduplication (must match by `guid` as a workaround). **Fix**: derive `EpisodeId` deterministically from the feed URL + episode GUID, e.g. `UUIDv5(namespace=PODCAST_NS, name="<feed_url>:<episode_guid>")`. Update `Episode::new` in `podcast-core`, add a roundtrip test, and remove the `guid`-based workaround in `store/auto_download.rs`. High priority — affects every feature that stores per-episode state.

## NMP Feature Parity — PR 1 follow-ups

- **pr1-store-persistence** — `PodcastStore` is in-memory only; app restart clears the library. Persist to sled or SQLite. Must happen before any milestone that depends on durable subscription state (feed refresh, playback position, downloads). Owner: PR 2 or whichever agent picks up feed refresh.
- **pr1-500ms-poll-to-push** — The podcast snapshot uses a 500ms Task poll in `KernelModel`. Replace with push-style delivery via the NMP `KernelUpdateSink` callback once the podcast projection is wired into `nmp-ffi`'s push surface. Tracked here; not blocking PR 2.
- **pr1-capability-bridge-unify** — `SyncCapabilityBridge` (synchronous, actor-thread) and `PodcastCapabilities.shared.handleJSON()` (async, main-thread) are two parallel routers. Each capability should decide its own threading model internally; the bridge should be the single router. Resolve before audio wiring (PR 3).
- **pr10-episode-description-htmlstrip** — `EpisodeSummary.description` is now projected and rendered, but RSS `<description>` is frequently HTML. `EpisodeDetailView` renders it raw, so users can see literal tags. Strip / convert to plain text (the legacy `EpisodeShowNotesFormatter.plainText(from:)` did this) before rendering, or use an attributed-string renderer. Tracked separately from the original `pr10-episode-description-projection` item which has been resolved.

## NMP Migration — M2.F Android proof follow-ups

The M2.F PR landed a working Rust→JNI→Compose proof; the items below are downstream of that landing.

- **m2f-gradle-wrapper** — vendor `gradlew` + `gradle/wrapper/gradle-wrapper.*` under `android/Podcast/` so first-time contributors don't need Android Studio just to compile. Pre-M3 (when Android picks up audio capability work).
- **m2f-library-snapshot-payload** — wire `LibraryProjection` (already typed in `apps/podcast-core/`) through `nmp_app_podcast_snapshot` so the Compose `LazyColumn` renders real subscriptions. Blocked on the M2.A snapshot serializer landing. Then drop the README's "renders nothing yet" caveat.
- **m2f-jni-shim-location** — once `apps/nmp-app-podcast/src/android.rs` grows beyond ~500 LOC or a sibling crate (e.g. a separate Android app) needs to consume the JNI, split it into `nmp-app-podcast-android-ffi` mirroring NMP's `nmp-android-ffi` pattern. Not blocking M3; defer until pain hits.
- **m2f-real-signin-sheet** — replace the stub `signinNsec` button in `MainActivity.kt` with a real nsec entry sheet that routes through the Keychain capability (mirror of `ios/Podcast/Features/Identity/`). Blocked on Android Keychain capability impl.

## NMP Migration — M4.B iOS DownloadCapability follow-ups

The M4.B PR landed the iOS `DownloadCapability` (`URLSession` background downloads). PR 17 wired the `DownloadReport` back-channel so completed downloads populate `EpisodeSummary.downloadPath`. The items below were observed during PR 17 validation.

- **m4b-downloadcapabilitywiretests-actor-isolation** — `ios/Podcast/PodcastTests/DownloadCapabilityWireTests.swift:132,139` references `DownloadCapability.namespace` / `DownloadCapability.sessionIdentifier` from a nonisolated `XCTAssertEqual` autoclosure; the host class is `@MainActor`, so the test fails to compile under Swift 6 strict concurrency. Predates PR 17 (verified by `git stash` + retry on the base commit). Two fixes: (a) mark the test methods `@MainActor`, or (b) make the two static properties `nonisolated`. Not blocking PR 17 (Rust dispatch tests cover the wire shape); fix before re-enabling the iOS test target in CI.
- **m4b-downloadreport-queue-projection** — PR 17 projects only `DownloadReport::Completed` (and defensively `Cancelled`) onto `PodcastStore::local_paths`. `Progress` / `Failed` / `Paused` decode cleanly and no-op; the richer `DownloadQueueSnapshot` projection (per-item state, bytes-downloaded, total-bytes) needs to be wired in a follow-up PR alongside the `DownloadQueue` writes already present in `apps/nmp-app-podcast/src/download/`.

## NMP Migration — M5 HTTP capability follow-ups

The M5 PR landed the Rust `HttpRequest`/`HttpResult` schema mirroring the iOS executor, plus `FeedClient` request/response bridge in `podcast-feeds`. The items below were deferred to keep that PR tight.

- **m5-non-utf8-feed-bodies** — `HttpCapability.swift` lossy-converts response bytes to a UTF-8 string via `String(data:encoding:.utf8)` before the bytes cross FFI. RSS feeds declared as Windows-1252 / ISO-8859-1 lose their original bytes here, so `quick_xml::Reader::from_reader` can't honour their `<?xml encoding=...?>` declaration. Pre-existing limitation also present in the legacy Swift `RSSParser`. Fix path: widen the HTTP capability wire to carry body bytes (base64 or a length-prefixed binary channel) and update both Swift + Rust to skip the lossy string round-trip. Track impact via feed-refresh telemetry once that exists. Not blocking M5–M13.
- **m5-podcastcapabilities-syntax-fix** — the iOS `PodcastCapabilities.swift:38` initializer is missing a `,` between `legacyIO` and `audio` parameters (introduced by M3.B `aae317c`). Independent of M5; tracked here so the next iOS-touching PR sweeps it.
- **m5-chirp-headers-parity** — `HttpResult.ok` now carries a `headers: [[String]]` field in podcast-player's executor; Chirp's `ios/Chirp/Chirp/Capabilities/HttpCapability.swift` does not. When the canonical `nmp-core::capability::http` lands upstream, reconcile both implementations against the canonical schema (likely lifting the header round-trip into the shared shape).

## NMP Migration — M1.E compat shims to remove

The M1.E build-compat layer at `ios/Podcast/Podcast/Compat/` is staging
scaffolding. Every entry below is a placeholder type with a no-op or
throwing implementation; each must be deleted (and the corresponding
migrated view re-wired) before the milestone it is anchored to closes.

- **M1 exit — kernel-side identity actions.** PR 16 deleted
  `Compat/UserIdentityStoreCompat.swift` and migrated every Identity view
  to read from `IdentityViewModel` (a projection of
  `PodcastUpdate.activeAccount`). The Rust kernel currently exposes the
  `active_account` projection field but no identity actions; all mutation
  views (`EditProfileView`, `UseMyOwnKeyView`, `RemoteSignerView`,
  `NostrConnectView`, `AdvancedView`, `ChangePhotoSheet`) surface a stable
  "actions land later" toast via `KernelModel.surfaceStagedIdentityAction`.
  Land the following actions in `apps/nmp-app-podcast/src/ffi/actions/`
  to close the loop:
    - `identity.import_nsec { nsec: String }`
    - `identity.generate` (silently regenerates a local key)
    - `identity.clear`
    - `identity.publish_profile { name, display_name, about, picture }`
    - `identity.connect_remote_signer { uri: String }`
    - `identity.disconnect_remote_signer`
    - `identity.connect_via_nostrconnect` (NIP-46 nostrconnect:// pairing)
  Also widen `AccountSummary` to carry the raw hex pubkey so
  `AccountDetailsView` can render its hex + SHA-256 fingerprint rows
  (currently `"—"`) without a Bech32 decode on the iOS side.
- **M1 exit — Keychain-backed credential stubs.** Delete the
  `NostrCredentialStore`, `NostrKeyPair`, and `Bech32` shims in
  `Compat/ServiceStubs.swift` + `Compat/UtilityStubs.swift`. Replace with
  the real BYOK Keychain capability + `nmp-keys` Swift bindings.
- **M2 — `Podcast` + `SubscriptionService` shims.** Delete from
  `Compat/DomainStubs.swift` and `Compat/ServiceStubs.swift` once the
  podcast / subscription projections land in `nmp-app-podcast`. Re-wire
  `KernelModel.podcast(feedURL:)` and `KernelModel.subscription(podcastID:)`
  in `Compat/KernelModelCompat.swift` to pure snapshot reads.
- **M3 — `Settings` projection (in progress).** `hasCompletedOnboarding` is
  now a real kernel projection: `PodcastStore::has_completed_onboarding` →
  `SettingsSnapshot.has_completed_onboarding` on `PodcastUpdate.settings` →
  `model.snapshot?.settings.hasCompletedOnboarding` on iOS. The
  `podcast.update_settings` action writes the flag and persistence rides
  on the existing `podcasts.json` envelope. `Compat/SettingsCompat.swift`
  is deleted; `Compat/KernelModelCompat.swift` no longer exposes `Settings`
  or `updateSettings(_:)`. Remaining work: surface OpenRouter mode /
  BYOK-imported credentials on the kernel side and delete the
  `OpenRouterCredentialStore` + `BYOKConnectService` shims in
  `Compat/ServiceStubs.swift`. PR 11 moved
  `nostrProfileName/About/Picture/PublicKeyHex/RelayURL` to
  `@AppStorage("agent.profile.*")` keys read by `AgentIdentityView`
  (seeded by `OnboardingView+Handlers`); a future M3 follow-up should
  reclaim those keys and reroute writes through dispatch.
- **pr9-onboarding-settings-dispatch (resolved for onboarding flag).**
  `OnboardingView+Handlers.finishOnboarding()` now dispatches
  `podcast.update_settings { has_completed_onboarding: true }` through
  the Rust kernel; the flag survives launches. Manual OpenRouter key save
  and BYOK credential import still write only to Keychain — the kernel
  `settings` projection has no LLM-provider credential surface yet.
  Identity name / picture continue to seed `agent.profile.*` UserDefaults
  keys (see PR 11). The onboarding-gate UI itself is still future work:
  no surface currently checks `model.snapshot?.settings.hasCompletedOnboarding`,
  so returning users will not re-see onboarding until a presenter is
  wired (see M3 follow-up above).
- **M7 — Agent / Nostr conversation projections.** Delete the
  `NostrConversationRecord/Turn/ProfileMetadata/PendingApproval` stubs in
  `Compat/DomainStubs.swift`, plus the agent surface (`nostrConversations`,
  `nostrProfileCache`, `pendingNostrApprovals`, `allowNostrPubkey`,
  `blockNostrPubkey`) in `Compat/KernelModelCompat.swift`. Delete the
  `Nip46ConnectCard` + `AgentConnectionSettingsView` stubs in
  `Compat/ServiceStubs.swift`. PR 11 audited the consumers — they all
  render compat empty state today:
    - `Features/Settings/Agent/NostrConversationsView.swift` reads
      `store.state.nostrConversations` + `store.state.nostrProfileCache`;
      both are always empty in compat → view perpetually shows
      "No conversations yet". Needs Rust-side conversation projection +
      a kind:0 profile cache before it can render real data.
    - `Features/Settings/Agent/NostrConversationDetailView.swift` is reached
      from the list above; same blocker.
    - `Features/Settings/Agent/NostrApprovalPresenter.swift` reads
      `store.pendingNostrApprovals` (always empty) and calls
      `allowNostrPubkey` / `blockNostrPubkey` (no-ops). Needs a Rust
      trust-list projection + `podcast.agent.{allow,block}_pubkey` actions
      before its sheet can ever surface.
  No iOS shell work to land before M7; the views compile and render
  empty state through the compat layer until then.
- **M10 — Blossom + image cache.** Delete `BlossomUploading` + `BlossomUploader`
  stubs in `Compat/ServiceStubs.swift`. Delete `CachedAsyncImage` shim in
  `Compat/UtilityStubs.swift` and replace with the disk + memory cache
  served from the HTTP capability.
- **Design system → Capabilities.** `Compat/UtilityStubs.swift` houses
  view helpers (`Haptics`, `glassSurface`, `dismissKeyboardToolbar`,
  `copyToClipboard`, `SystemShareSheet`, `DeepLinkHandler`, `String.trimmed/isBlank`,
  `Data(hexString:)`). These are pure UI utilities — promote them to a
  proper design-system module (`ios/Podcast/Podcast/Design/`) when the
  M2+ design-system work begins.

## NMP Migration — M12 deletion sweep deferral

M12's nominal job is `git rm -r App/Sources/` once every Swift file
under it has either been migrated to `ios/Podcast/Podcast/` or is
explicitly named in an earlier milestone's deletion list. Auditing
the tree on 2026-05-25 against the M1 + M2 exit checklists shows
both milestones are still in flight (per `WIP.md`: M2.A/B/C/D/E
remain on branches; M1.E build-compat is still active). Every file
named for deletion is still imported by ≥1 file inside
`App/Sources/`, so none of them can be removed yet without breaking
the legacy build.

Status: M12 is **blocked on M1+M2 fully landing**. The audit + the
specific blockers are recorded below so the next agent that picks up
the deletion sweep doesn't redo the cross-reference work.

- **m12-defer-m1-identity-files.** The M1 exit checklist lists
  `App/Sources/Services/{NIP19,Bech32,NIP65RelayFetcher,
  UserIdentityStore,UserIdentityStore+NIP46,
  UserIdentityStore+ProfileFetch,UserIdentityStore+Publishing,
  NostrCredentialStore,NostrKeyPair,NostrProfileFetcher}.swift` and
  `App/Sources/Services/Nip46/*.swift` (9 files). Every file is
  still referenced by ≥1 site in `App/Sources/`:
    - `UserIdentityStore` referenced in `AppMain.swift`,
      `App/RootView.swift`, `App/AppSidebarView.swift`, all of
      `Features/Identity/*.swift`, `Features/Feedback/*.swift`,
      `Features/Settings/Agent/*.swift`, `Agent/*.swift`,
      `State/AppStateStore+*.swift` (~30 files total).
    - `NostrKeyPair` / `NostrCredentialStore` / `NIP65RelayFetcher`:
      same shape, used pervasively.
  Removable only after the legacy `App/` target no longer compiles
  against them — i.e. once `ios/Podcast/` becomes the sole source.
  Owner: whichever agent closes M1.E + the eventual M12.B unit.

- **m12-defer-m2-domain-files.** The M2 exit checklist lists
  `App/Sources/Podcast/*.swift` (20 files), several `App/Sources/
  Domain/*.swift`, all of `App/Sources/State/*.swift` (27 files),
  and `App/Sources/Services/{SubscriptionRefreshService,
  SubscriptionService,ITunesSearchClient,EpisodeMetadataIndexer,
  EpisodeAuditLogStore,NowPlayingSnapshotStore}.swift`. Every one
  is still imported by feature views in `App/Sources/Features/`
  and/or by the `AppMain.swift` boot path. Same defer rule as
  above. Owner: whichever agent closes M2.E + M12.B.

- **m12-codegen-widget-snapshot.** The M11 stubs added a
  `WidgetSnapshot` to the Rust `PodcastUpdate`, but
  `ios/Podcast/Podcast/Bridge/Generated/PodcastTypes.generated.swift`
  is a hand-trimmed M0 placeholder (`running` + `rev` only). When
  the projection-schema codegen lands
  (`cargo run -p nmp-app-podcast --features codegen-schema --bin
  dump_projection_schemas | cargo run -p nmp-codegen -- gen swift`),
  the iOS-side `PlatformCapability` will be able to read
  `update.widget` directly off the typed decoder instead of going
  through the hand-mirrored `WidgetSnapshot` Codable in
  `Capabilities/PlatformCapability.swift`. Track + delete the
  hand-mirror once codegen is wired.

## Pending Decisions

- None currently. If a change would alter bundle IDs, App Group identifiers, URL schemes, persisted state keys, or relay/event compatibility beyond the active plan, add the decision here before implementation.

## Done

- 2026-05-25 - Moved the active Pod0/NIP-F4 implementation plan into `docs/plan/pod0-nostr-publishing.md` and added canonical planning files.
- 2026-05-25 - Deleted the historical plan tree and removed active references to it. Use `docs/plan.md`, `docs/BACKLOG.md`, and linked files under `docs/plan/` for planning.
