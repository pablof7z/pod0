# Backlog

This is the tactical queue for active work, follow-ups, and pending decisions.
Do not duplicate these items in `WIP.md`; `WIP.md` only records branches and
worktrees currently in flight.

## Active

- **P0 - Pod0 rename.** Rename the working app identity from Podcastr to Pod0 where users or generated project surfaces see the app name. Preserve stable identifiers unless an explicit migration plan says otherwise: `io.f7z.podcast`, `io.f7z.podcast.widget`, `group.com.podcastr.app`, URL scheme/data identifiers, and existing Keychain/data continuity should not be changed as part of the display-name rename.
- **P0 - NIP-F4 owned podcast publishing.** Implement `docs/plan/pod0-nostr-publishing.md`: per-podcast keys, kind `10154` show events, kind `54` episode events, kind `10064` author claims, and deletion cleanup.
- **P0 - NIP-F4 discovery.** Update discovery parsing and episode fetches for kind `10154`/`54`, no `d` tags, and stable UUID derivation from `10154:<podcast-pubkey>`.
- **P1 - Planning cleanup.** Treat existing tracked files under `Plans/` as historical reference. Promote any active future work into `docs/plan.md`, `docs/BACKLOG.md`, or a linked `docs/plan/` detail file instead of adding new files under `Plans/`.

## NMP Migration — Cross-cutting backlog items

These items are prerequisites or follow-up work for specific milestones in `Plans/nmp-migration/`. Each is blocked until its milestone starts, then becomes Active.

- **nmp-foundation-audit** — replace every reference to `DomainModule`, `ViewModule`, `IdentityModule` in the migration plan with the shipped substrate traits (`ActionModule`, `CapabilityModule`, `DomainMigration`, `KernelEventObserver`). Pre-M0. (Partially done — substrate verified 2026-05-25.)
- **nmp-nip74-add** — new crate in NMP for podcast events (kind:30074, 30075). ADR pinning schema. Pre-M2.
- **nmp-blossom** — new crate in NMP for Blossom protocol. Pre-M10.
- **nmp-nip26-add** — delegation crate (verify if already inside `nmp-signer-iface` first). Pre-M10.
- **nmp-nip65-query** — explicit query module if `nmp-router` doesn't already export it. Pre-M1.
- **cap-audio** — `nmp.audio.capability` schema + ADR + Android stub. Pre-M3.
- **cap-download** — same pattern as cap-audio. Pre-M4.
- **cap-notifications** — same pattern. Pre-M11.
- **cap-stt** — no polling; webhook design required. Pre-M5.
- **cap-tts** — same pattern. Pre-M8.
- **cap-vector** — raw primitives only (`KnnSearch`, `BM25Search`, not `QueryHybrid`). Pre-M6.
- **cap-spotlight** — iOS-only. Pre-M11.
- **cap-carplay** — iOS-only. Pre-M11.
- **cap-handoff** — iOS-only. Pre-M11.
- **cap-icloud** — iOS-only. Pre-M11.
- **cap-review** — iOS-only. Pre-M11.
- **cap-data-export** — multi-platform. Pre-M11.
- **cap-legacy-io** — iOS-only, used only during migration for reading legacy data stores. Pre-M1.
- **cap-video** — clip export. May defer post-M13.
- **apps-podcast-scaffolding** — accept `apps/podcast/` tree into NMP repo (mirror `apps/chirp/`). This is M0.A.
- **per-view-emit-rate** — extend `nmp-core` tick loop to support per-view emit rates so agent streaming tokens can hit 30 Hz. Required before M7. File NMP BACKLOG entry when M7 starts.
- **threading-podcast-peer** — confirm `nmp-threading` exposes the API `podcast-peer` needs; extend if not. Pre-M10.

## NMP Migration — M2.F Android proof follow-ups

The M2.F PR landed a working Rust→JNI→Compose proof; the items below are downstream of that landing.

- **m2f-gradle-wrapper** — vendor `gradlew` + `gradle/wrapper/gradle-wrapper.*` under `android/Podcast/` so first-time contributors don't need Android Studio just to compile. Pre-M3 (when Android picks up audio capability work).
- **m2f-library-snapshot-payload** — wire `LibraryProjection` (already typed in `apps/podcast-core/`) through `nmp_app_podcast_snapshot` so the Compose `LazyColumn` renders real subscriptions. Blocked on the M2.A snapshot serializer landing. Then drop the README's "renders nothing yet" caveat.
- **m2f-jni-shim-location** — once `apps/nmp-app-podcast/src/android.rs` grows beyond ~500 LOC or a sibling crate (e.g. a separate Android app) needs to consume the JNI, split it into `nmp-app-podcast-android-ffi` mirroring NMP's `nmp-android-ffi` pattern. Not blocking M3; defer until pain hits.
- **m2f-real-signin-sheet** — replace the stub `signinNsec` button in `MainActivity.kt` with a real nsec entry sheet that routes through the Keychain capability (mirror of `ios/Podcast/Features/Identity/`). Blocked on Android Keychain capability impl.

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

- **M1 exit — `UserIdentityStore` shim.** Delete `Compat/UserIdentityStoreCompat.swift`
  and inject a real identity store backed by `nmp-signer-broker` via the
  KernelModel snapshot. Re-wire `.environment(UserIdentityStore())` in
  `PodcastApp.swift` to whatever the M1 exit deliverable lands.
- **M1 exit — Keychain-backed credential stubs.** Delete the
  `NostrCredentialStore`, `NostrKeyPair`, and `Bech32` shims in
  `Compat/ServiceStubs.swift` + `Compat/UtilityStubs.swift`. Replace with
  the real BYOK Keychain capability + `nmp-keys` Swift bindings.
- **M2 — `Podcast` + `SubscriptionService` shims.** Delete from
  `Compat/DomainStubs.swift` and `Compat/ServiceStubs.swift` once the
  podcast / subscription projections land in `nmp-app-podcast`. Re-wire
  `KernelModel.podcast(feedURL:)` and `KernelModel.subscription(podcastID:)`
  in `Compat/KernelModelCompat.swift` to pure snapshot reads.
- **M3 — `Settings` projection.** Delete `Compat/SettingsCompat.swift` and
  the `state` / `updateSettings` extensions in `Compat/KernelModelCompat.swift`.
  Settings should be a real kernel projection emitted by `nmp-app-podcast`.
  Also delete the `OpenRouterCredentialStore` + `BYOKConnectService` shims
  in `Compat/ServiceStubs.swift` and replace with the LLM-provider credential
  capability.
- **M7 — Agent / Nostr conversation projections.** Delete the
  `NostrConversationRecord/Turn/ProfileMetadata/PendingApproval` stubs in
  `Compat/DomainStubs.swift`, plus the agent surface (`nostrConversations`,
  `nostrProfileCache`, `pendingNostrApprovals`, `allowNostrPubkey`,
  `blockNostrPubkey`) in `Compat/KernelModelCompat.swift`. Delete the
  `Nip46ConnectCard` + `AgentConnectionSettingsView` stubs in
  `Compat/ServiceStubs.swift`.
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
