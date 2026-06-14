---
title: Podcast App State
slug: podcast-app-state
topic: data-persistence
summary: The PodcastAppState strangler refactor is complete (15/15 substates), eliminating the twin 34/36-field god-roots (PodcastHandle and PodcastHostOpHandler) and th
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-12
updated: 2026-06-14
verified: 2026-06-12
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:c43d5e77-d667-4e71-a574-47aaab5b6a7a
  - session:e1ab0629-64bc-4383-bd22-c0843ca16a99
  - session:56e47844-b4ff-4402-9528-c704eade1d7b
  - session:38f8143c-c90d-49e3-a8fa-8d5ca17ac319
  - session:rollout-2026-05-10T00-02-36-019e0e8c-9c89-7dc1-9942-c63cb0efd9c4
  - session:rollout-2026-05-11T08-21-01-019e157b-49b7-7663-891c-1c44d125ca44
  - session:rollout-2026-05-25T12-50-00-019e5e8a-9307-7903-9302-dbc867f91c61
  - session:rollout-2026-05-25T22-38-52-019e60a5-b1f5-7883-b0d3-a8d1826b1709
---

# Podcast App State

## Strangler Refactor

The PodcastAppState strangler refactor is complete (15/15 substates), eliminating the twin 34/36-field god-roots (PodcastHandle and PodcastHostOpHandler) and the 31-arg constructor in favor of a single Arc<PodcastAppState> of 16 durability-typed substates with a ~40-line composition root in register.rs. PodcastHandle is down to 4 fields (app, state, snapshot_cache, clean_html_cache) and PodcastHostOpHandler down to 2 fields (app, state). Slot<T, Durability> uses sealed Persisted/Session/Derived markers so that persist() only exists on Slot<_, Persisted>, making session-slot-persisted a compile error. A libraryGeneration O(1) counter gates applyKernelState; no-op/snapshot-only ticks skip the episode rebuild and projection invalidation, reducing warm ticks from ~29ms to ~0.04ms.

PR #376 established the PodcastAppState scaffold with Slot<T, Durability> (sealed type-level durability where persist() only exists on Slot<_, Persisted>), Infra struct, and the Knowledge substate as the pattern-setter. PR #378 PicksState eliminated the duplicate categorization_in_progress/picks_score_in_progress race between PodcastHostOpHandler and FeedFetchCoordinator by consolidating into single canonical guards owned by each substate. The category-in-progress re-entrancy guard was consolidated from duplicate Arc<AtomicBool> on both PodcastHostOpHandler and FeedFetchCoordinator into a single substate-owned guard per PicksState and CategoriesState, eliminating a real concurrency bug where a manual refresh and an async-subscribe completion could each think they owned the pass. Knowledge, Wiki, Discovery, and Social substates had their dead duplicate Arc fields (nostr_results, agent_notes) removed from PodcastHostOpHandler — these were second Arc clones never locked, with the live path via the handle.

The Library/store substate (step 15) plus feedback/feed_fetch (step 16) and shell collapse (step N+1) eliminate all remaining mirrored fields, reduce PodcastHostOpHandler::new to 2 args (app, state), and reduce register.rs to the ~40-line composition root.

HandoffState was deliberately left untouched in PR #371 because it is not embedded in PodcastUpdate (uses a separate updateHandoff/NSUserActivity path) and its CodingKeys are correct.

The subscribed-library deep clone in snapshot.rs was replaced with a borrow (Vec<&PodcastSummary>) since both consumers already take &[…].

<!-- citations: [^rollo-216] [^38f81-12] [^rollo-28] [^rollo-29] [^c1691-6] [^14943-16] [^c1691-30] [^rollo-71] [^c1691-61] [^c1691-79] [^c1691-91] [^c1691-122] [^c1691-143] [^c1691-163] [^c1691-171] [^c1691-187] -->
## Dispatch Router

The namespaced-envelope dispatch router replaces the try-parse cascade, with a match on ns preventing silent misrouting (e.g., wiki/knowledge collision), and includes per-collision regression tests. <!-- [^c1691-7] -->

## FFI Panic Safety

Every extern "C" / extern "system" FFI entry in the podcast crate wraps its body in ffi_guard with a lazy fallback (impl FnOnce() -> T), so panics degrade to a sentinel rather than aborting across the C ABI; panic="abort" is explicitly rejected to preserve nmp_core's catch_unwind around actor ticks. dispatch_audio_cmd and dispatch_download_cmd require null-app guards (return early on null app pointer) consistent with PodcastHostOpHandler::dispatch_audio doctrine.

<!-- citations: [^c1691-8] [^c1691-378] -->
## Serde & Decoding Guards

Nine required float fields across PodcastUpdate projections have serde serialize_with guards (finite_f64_or_zero / finite_f32_or_zero) so no required float can serialize to JSON null. parse_duration rejects non-finite values (NaN, infinity, negative) at the inlet, and projection-side finite_or_zero serialization guards clamp NaN/Inf to 0.0 on 9 required float fields across 6 files, closing the remotely-triggerable frame-drop vector. Swift XCTest golden-fixture decode tests through KernelDecoding (.convertFromSnakeCase) cover all embedded types, guarding against the #371-class Rust/Swift schema divergence. The golden snapshot byte-identity test (snapshot_bytes_match_golden_fixture) gates every strangler step, asserting PodcastUpdate JSON is byte-identical to a committed fixture (3821 bytes initially, 3789 after inbox_triage_in_progress was removed from the projection).

<!-- citations: [^c1691-9] [^c1691-31] [^c1691-92] [^c1691-109] -->
## 304 NotModified Handling

The 304 NotModified rev bump was removed from handle_ensure_podcast (it remains in handle_refresh_all by explicit design), so an all-304 foreground refresh no longer triggers a full snapshot rebuild; auto_categorize and auto_refresh_picks gating is preserved independently.

<!-- citations: [^c1691-10] [^c1691-80] [^c1691-93] -->
## Domain Sub-Projections

The domain sub-projections architecture uses per-domain revs (each substate's Infra carries its own domain AtomicU64 in addition to the global rev), with bump_domain bumping both domain and global, so a playback tick ships only ~1KB of playback JSON rather than the full library. Download progress reports bump a separate download rev instead of the global podcast rev, so progress ticks no longer force a full library snapshot decode on the main thread. Episode completion bumps the global rev (durable_changed=true), triggering a full library snapshot pull to flip the episode to 'Downloaded'. The correct v0.5.0-native seam for domain sub-projections is register_typed_snapshot_projection with Option-gating (returning None omits the key from the frame entirely that tick), not the generic register_snapshot_projection_gated which still clones+ships unchanged keys. The per-domain typed sub-projections PR (#399) was sent back incomplete because bump_domain was never wired at actual substate mutation sites — domains would fire once on first tick then go idle forever, delivering zero perf benefit. PR #396 deleted the verified-dead podcast.snapshot gated projection (its multi-MB serde_json::from_str/clone output was discarded by the v0.5.0 typed-first encoder). PR #252 (download-rev-decouple + case-sensitivity fix) is merged to main.

Domain sub-projections uses 7 domains: podcast.library (library+categories, the big one), podcast.playback (now_playing+queue), podcast.downloads, podcast.inbox, podcast.settings, podcast.identity, podcast.widget, and a podcast.misc catch-all for wiki/picks/clips/transcripts/social/agent/discovery/voice/publish/tasks.

The iOS push path was dead on both shells since NMP v0.3.0: iOS was 100% pull-driven (every push frame failed decode for the missing podcast.snapshot key), and Android decoded the slim envelope as a full PodcastSnapshot, clobbering the UI to empty on every emit. iOS per-domain push consumption (PR #403) uses a composite PodcastUpdate merged from accepted domain sidecars with per-domain rev-monotonic drop guards, retains pull as cold-start/fallback, and sets hasHydrated=true after the first pull to allow >= on the first cold-start pull then strict > afterwards. PR #403 replaces the dead PodcastHandle.decode path that required v.projections["podcast.snapshot"] (never encoded since v0.3.0) with PodcastDomainFrames decoding of per-domain sidecars, applying ~1KB deltas per domain with a composite merge.

The cold-start re-seed follow-up (PR #405) extracted mergeDomainFramesImpl as a static @MainActor method and added 3 @MainActor tests verifying library/downloads/widget tombstones clear the corresponding composite slice.

Android per-domain consumption removes the empty-snapshot clobber by decoding v.projections["podcast.*"] into domain frames and merging via copy() on only accepted domains, with the cold-start pull retained for initial hydration.

<!-- citations: [^c1691-11] [^56e47-9] [^c1691-62] [^c1691-94] [^c1691-108] [^c1691-123] [^c1691-144] -->
## Owned-Podcast Backfill

The owned-podcast kind:54 backfill (PR #397) moved per-episode publishing from a synchronous Swift loop into kernel-owned self-enqueued dispatches — update_owned detects a private→public flip, then self-dispatches N publish_episode actions (one per actor tick via nmp_app_dispatch_action) so the actor yields between episodes rather than blocking for N sequential Blossom uploads. <!-- [^c1691-12] -->

## PlaybackState (Step 14)

PlaybackState (step 14) uses Slot<PlayerActor, Session>, Slot<PlaybackQueue, Persisted>, and Slot<DownloadQueue, Session> — the cross-thread report writers (audio_report, download_report) rewire to access slots via state.playback.player.lock()/state.playback.downloads.share() with unchanged lock topology and no new deadlock hazard, confirmed by Opus review. The playback substate migration rewires player_actor, queue, and download_queue into state.playback with .share()/lock() preserving the identical Arc<Mutex<T>> across both seams, with unchanged lock topology (no new deadlock hazard in audio_report, download_report, or maybe_auto_advance).

<!-- citations: [^c1691-13] [^c1691-32] [^c1691-95] [^c1691-145] -->
## Episode Projections

Double recomputeEpisodeProjections per projection tick was eliminated by wrapping state-mutating epilogue in performMutationBatch; the explicit invalidateEpisodeProjections call is retained because it catches same-count merges that the fingerprint misses. triageCounts O(N) scan on every HomeView body pass eliminated by caching three per-show buckets in EpisodeProjections; reads are now O(1) for All and O(category size) when scoped. During confirmed steady playback, applyKernelState and recomputeEpisodeProjections fire zero times on the main thread. <!-- [^14943-17] -->


Per-episode HTML stripping (strip_html) is memoized on the PodcastHandle with a content-hash-keyed bounded cache, stopping redundant re-stripping of immutable descriptions on every snapshot rebuild. <!-- [^e1ab0-19] -->
## Kernel State Application

applyKernelState uses a summary-level diff to reuse unchanged Episode objects instead of rebuilding all N episodes every tick; toEpisode is called only for new or changed summaries. The applyingPositionCache slice copy per read eliminated by a disjoint-slice overlap guard that returns the input array unchanged when no episode in the slice has a pending position. Episodes promoted to a separate @Observable stored property (store.episodes) so episode mutations no longer re-render unrelated views reading settings/nostr/agent state. Empirical performance measurements on a 3,615-episode library: cold applyKernelState 38ms, warm applyKernelState 29ms (9.6ms of which is nested recomputeEpisodeProjections). <!-- [^14943-18] -->


Kernel action dispatches from Swift use typed wrapper methods in AppStateStore+KernelActions.swift rather than raw dictionary construction at call sites. <!-- [^c43d5-13] -->
## Off-Main Hash Computation

libraryMetaHash and snapshotContentHash computation moved off the MainActor onto Task.detached(.utility) for 4Hz playback frames, with a monotonic-rev newest-wins reentrancy guard. Hashes fire at 0.58Hz (not 4Hz) and run off-main via Task.detached; libraryMetaHash 3.17ms, snapshotContentHash 0.78ms. <!-- [^14943-19] -->

## Dispatch Synchronousness

User dispatch paths (dispatch, dispatchSilent, start, resetAndRestart) run applyPodcastUpdate with synchronous:true for same-runloop freshness; push frames and onSnapshotMaybeChanged run with synchronous:false. <!-- [^14943-20] -->

## Relay Configuration

NMP v0.2.1 upgrade adds configured_relays / AppRelay struct for app-level relay configuration, and the podcast app seeds initial relays via set_initial_relays_for_start (unconditionally for now, to be converted to seed-if-empty when the relay-edit UI ships). The Rust relay ops projection (configured_relays: [AppRelay] plus add_relay/remove_relay/set_relay_role on podcast.settings) was shipped in PR #202, including a DispatchHostOp rev-bump companion so relay edits reach the UI. Canonical relay role strings are read | write | both | indexer | both,indexer (composite always in that order). Relay configuration UI provides a top-level Agent Relay row in Settings→Agent and an App Relays editor in Settings→Networking; the App Relays editor was deferred until the Rust configured_relays projection and add_relay/remove_relay/set_relay_role ops exist. <!-- [^14943-21] -->

## Download Performance

During a download on a 198-episode library, JSONDecoder main-thread frames dropped from 77 to 0 and CPU pulses from 3–43% to 0.8–6.5%. <!-- [^56e47-10] -->

## Default Settings Architecture

Settings values cross three type systems (Rust domain, Rust persistence, Swift snapshot/domain) with no shared definition or code generation ensuring defaults stay in sync — this fragmentation causes the seven-default problem. PodcastStore::new() is the single canonical Rust default source for settings; PersistedSettings::default() derives from it via PodcastStore::new().persisted_settings(), and SettingsSnapshot::default() derives from build_settings_snapshot(&PodcastStore::new()) memoized in a OnceLock. The dead podcast_core::Settings type is deleted entirely. All literal fallback fields in disk.rs hydration are replaced with d.field references from a single PersistedSettings::default(). Swift Settings.swift derives its defaults from SettingsSnapshot() at runtime rather than maintaining independent Codable defaults. A cross-language fixture test (tests/fixtures/settings_fresh_install.json) enforces that the two remaining default sites stay in sync — a Rust test fails if the fixture is not regenerated after a default change, and a Swift test fails if the Swift initializer is not updated. The final default site count is 2, down from 7+; new settings need their default set in exactly one place.

Settings persistence and iCloud sync must centralize a settings field registry with sync policy, add exhaustive persistence/iCloud tests, and eliminate duplicated manual field lists that cause inconsistencies like `autoSkipAds` being excluded from iCloud sync.

<!-- citations: [^dced2-2] [^rollo-72] -->

## Inbox Triage Trigger

PR #383 Inbox substate migration lifted maybe_enqueue_triage out of the snapshot projection builder (D8 compliance) into the feed-refresh and cold-start paths (auto_download_evaluate and refresh_all), making the projection pure while preserving proactive triage cadence. The refresh_all path is decoupled from the any_succeeded gate so 304-only refreshes still fire triage.

<!-- citations: [^c1691-63] [^c1691-81] [^c1691-96] [^c1691-110] -->
## Android decodeEnvelope Bug

Android decodeEnvelope decodes the slim v as a full PodcastSnapshot, defaulting every field, so every kernel emit clobbers the UI to an empty state — an active correctness bug. <!-- [^c1691-64] -->

## Android Inbox and Transcripts UI

PR #398 added Android Tier-2 Inbox and Transcripts UI on the shared kernel (thin-shell Compose work, no Rust/JNI changes), including inbox with triage shimmer and transcripts UI surfaces. Android Tier-2 is fully shipped: Inbox (triage shimmer + dismiss), Transcripts, Agent chat (thin shell on the real kernel agent loop dispatching podcast.agent ops), AI picks rail (picks domain frame), and AI chapters + auto-skip ads (dispatching podcast.chapters compile and podcast.settings set_auto_skip_ads).

<!-- citations: [^c1691-65] [^c1691-82] [^c1691-164] -->
## Product Identity

The product identity is renamed to Pod0 while preserving `io.f7z.podcast`, `group.com.podcastr.app`, `podcastr://`, and entitlement filenames as stable compatibility identifiers. <!-- [^rollo-171] -->

Serde & Decoding Guards

The PodcastSettingsSnapshot generator emits explicit CodingKeys with per-field snake_case overrides and a custom decodeIfPresent-with-defaults init(from:) that cannot throw keyNotFound. <!-- [^c1691-172] -->

## Android Tier-2 Feature Parity

Android has no social/conversations/friends/follow UI surface at all; DomainFrames.kt decodes the podcast.social payload but nothing renders it — this is the clearest durable feature-parity gap.

<!-- citations: [^c1691-258] [^c1691-301] -->
## Backlog Hygiene

Four `compat-*-delete` BACKLOG items target `ios/Podcast/Podcast/Compat/*` paths that no longer exist in origin/main, and `episode-comments-relay-wiring` describes stubs that are already real kind:1111 publish code — both are stale and should be marked done/removed.

<!-- citations: [^c1691-259] [^c1691-291] -->
