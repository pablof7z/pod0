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

## Pending Decisions

- None currently. If a change would alter bundle IDs, App Group identifiers, URL schemes, persisted state keys, or relay/event compatibility beyond the active plan, add the decision here before implementation.

## Done

- 2026-05-25 - Moved the active Pod0/NIP-F4 implementation plan into `docs/plan/pod0-nostr-publishing.md` and added canonical planning files.
