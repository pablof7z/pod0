---
title: Kernel Projections
slug: kernel-projections
topic: agent-system
summary: Projections are read-only views of kernel state computed on every tick and delivered to the app as part of the snapshot envelope, but only when changed_since_em
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-03
updated: 2026-06-13
verified: 2026-06-03
compiled-from: conversation
sources:
  - session:55bedfc3-dd9e-4b1c-b7d7-cea0c699d4d1
  - session:deb49f4f-f275-419a-ab1c-b68c123af73b
  - session:cf4c4d92-a662-4077-8787-9cfba26007a1
  - session:7e35e451-81d2-4832-8c6e-34d44fc29e12
  - session:04b5f843-fdbe-4aa1-ae41-6770eac82957
  - session:4243e533-7577-4916-afae-773f1c45b9f2
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
  - session:38f8143c-c90d-49e3-a8fa-8d5ca17ac319
  - session:rollout-2026-05-10T00-02-36-019e0e8c-9c89-7dc1-9942-c63cb0efd9c4
  - session:rollout-2026-05-17T17-40-02-019e3661-3d9a-76d3-a4a5-f5779f6a0ee8
---

# Kernel Projections

## Overview

Projections are read-only views of kernel state computed on every tick and delivered to the app as part of the snapshot envelope, but only when changed_since_emit is true — idle ticks produce no emission or projection work. Since NMP v0.3.0 ADR-0044, the Tier-3 frame encoder encodes only the typed envelope + typed-projection FlatBuffer sidecars and discards the generic KernelSnapshot::projections map, making the old register_snapshot_projection_gated podcast.snapshot registration pure actor-thread waste whose output was never consumed; it has been deleted (PR #396), yielding an immediate actor-thread CPU win and eliminating per-tick multi-MB serde_json::from_str and Value::clone overhead. Per-domain sub-projections must use the v0.5.0-native seam register_typed_snapshot_projection returning Option<TypedProjectionData>, where None omits the key from the frame entirely, not the generic register_snapshot_projection_gated, because the v0.5.0 encoder discards the generic projections map entirely. The kernel signs events and returns the signed JSON via a correlation-id in the signed_events snapshot projection, ensuring no private key bytes cross the FFI boundary. PR #385 fixed the silently-dropped signed_events push path by adding decode_signed_events_sidecar to snapshot.rs, which decodes the typed FlatBuffer sidecar and injects it under v.projections["signed_events"] in decode_update_frame so that iOS SignedEventsRegistry.ingest receives data again, un-breaking the silent regression where every signEventForReturn result was dropped post-v0.3.0. Per-domain DomainRevs and typed sidecar projections are wired at every mutation site via domain-scoped Infra, with bump_domain advancing both the domain counter and the global rev; proof tests confirm that a playback mutation bumps only the playback domain rev while the global rev still advances for the pull path. The domain rev must be bumped at the real mutation site via state.social.infra.bump() (the Infra::bump idiom that advances both the domain counter and global signal), never via a test-only fetch_add on the counter — the #423 lesson. Domain frame decoders must not use explicit snake_case CodingKeys enums in Swift; they rely on .convertFromSnakeCase from the KernelDecoding seam, which was the class of bug in issue #371 where one wrong CodingKey froze the entire UI. kind:0 profile hydration for conversation participants rides the existing resolved_profiles seam (claimProfile → kernel resolve → projections.resolved_profiles → nostrProfileCache), not a new hydration inside the conversation projection. The social_handler's one-shot 8s-timeout hardcoded-relay.primal.net fetch is replaced with a reactive FollowListProjection registered via nmp_app_register_snapshot_projection, and ActiveFollowSet membership drives AgentNoteSummary.trusted (the declared blocker for the agent-to-agent responder and conversations approval).

<!-- citations: [^c1691-157] [^38f81-3] [^38f81-4] [^cf4c4-1] [^55bed-4] [^deb49-1] [^c1691-19] [^c1691-41] [^c1691-54] [^c1691-68] [^c1691-116] [^c1691-135] [^c1691-156] [^c1691-222] [^c1691-254] [^c1691-298] -->
## Performance Profile

A single mark-played action on a 3,600-episode library re-ships the entire 3.9 MB snapshot (~3,286× the 1.2 KB that actually changed). The Swift JSONDecoder decode of the full-library snapshot takes ~35.5 ms on the simulator and ~70–140 ms on a real iPhone for a 3,600-episode library, ~18× the Rust serialize cost. The O(N²) cost in build_podcast_update is caused by position_for performing a linear scan of every episode (with a UUID→String allocation per comparison) to find the position of an episode already in hand; reading ep.position_secs directly makes rebuild linear and yields a 30× speedup (224 ms → 7.6 ms at 3,600 episodes). Home inbox and triage projections must be cached as derived store projections rebuilt only when episodes/triage change, not recomputed from full episode scans on each render. Observation narrowing, O(1) lookup projections, and Home caching must be implemented first as one hardening slice before refresh batching.

The iOS PlatformCapability.applyWidgetSnapshot method deduplicates writes with position fraction quantized to 1% buckets.

<!-- citations: [^deb49-2] [^38f81-5] [^rollo-160] -->
## Threading Model

The pull-path snapshot decode and content hashing run on a serial dispatch queue off the MainActor, hopping back to the main thread to commit the update; the push path already uses this pattern. The synchronous parameter on applyPodcastUpdate and pullPodcastSnapshotIfChanged is removed as dead code, since both decode and hashing now always run off the MainActor. No caller (including AppIntents, Siri, Shortcuts, and CarPlay) reads library/podcastSnapshot/episodes synchronously after dispatch() — all 122 dispatch sites are fire-and-forget relying on @Observable reactivity. <!-- [^deb49-3] -->


pullPodcastSnapshotIfChanged is async (dispatches decode off-main then applies on main), creating a potential race between snapshot pull completion and event reads. <!-- [^7e35e-7] -->

During confirmed live playback, the kernel emits an idle WidgetSnapshot (is_playing=false, no now_playing_episode_title, position_fraction=0, duration_secs=0) while the iOS audio engine plays independently, meaning the #366 now-playing widget projection does not populate playing state. <!-- [^38f81-6] -->
## Revision Discipline

Playing ticks must not bump rev (no rebuild during playback); paused ticks must bump rev. This is empirically confirmed on the merged main. A 304 NotModified feed response no longer bumps rev, so no-op refreshes do not trigger full snapshot rebuilds. The categorization per-episode unconditional rev bump is gated on actual change, while the refresh_all 304 rev bump remains deliberate (not to be removed).

<!-- citations: [^deb49-4] [^c1691-25] -->
## Merged PRs

PR #264 (off-main decode), PR #265 (rev-discipline: playback ticks don't bump rev), and PR #267 (O(N²)→O(N) rebuild fix) are merged to main and verified together with a real seeded kernel measurement. <!-- [^deb49-5] -->

## Deferred Optimizations

The delta/wire-contract projection (Win B) is not recommended for implementation because, after the three landed fixes, a durable-change rebuild is ~7.6 ms Rust + an off-main decode, which does not justify the D5 wire-contract risk. <!-- [^deb49-6] -->


The raw feedback_events projection is retained only for an isLoading check and could be dropped as a follow-up. <!-- [^04b5f-5] -->
## Implementation Notes

The signEventForReturn functionality is defined in KernelModel.swift:602 and KernelBridge.swift:180, having been folded into those existing files rather than existing as a separate KernelModel+SignAndReturn.swift file. KernelModel.requestSnapshotPull() is a public wrapper for the private pullPodcastSnapshotIfChanged() method, enabling identity-layer callers to request a snapshot pull after keygen or nsec import. kernel.dispatch is a synchronous FFI round-trip (DispatchResult carries the handler's ok/error inline), so a loadEvents() call after await ingest() sees the skip event with no race condition. KernelModel.shared is a weak static reference that can become nil, causing kernelEpisodeEvents to return an empty array silently. The snapshot schema fail-closed gate (issue #356) rejects PodcastUpdate snapshots where schema_version != KERNEL_SCHEMA_VERSION, logging a .fault and returning nil, but is a no-op on the happy path since the kernel always emits schema_version: 1 matching the constant. Issue #356's naive prescribed fix (a != KERNEL_SCHEMA_VERSION gate at the PodcastUpdate decode sites) is safe because the Rust PodcastUpdate::default() sets schema_version: 1 and the field is serialized (no skip_serializing_if), so the projection always emits schema_version: 1. KERNEL_SCHEMA_VERSION is typed as Int and access-level internal (not private) so both decode sites in different Swift files can reference it. Episode projection caches store indexes into state.episodes rather than duplicating full Episode structs for every show. Episode mutations are batched so common actions produce a single projection rebuild and side-effect pass. The owned-podcast kind:54 backfill moves per-episode publish from a synchronous Swift loop into kernel-owned self-enqueued actions (one ActorCommand::DispatchHostOp per episode via nmp_app_dispatch_action, never blocking the actor), preserving D8 responsiveness on 50–100 episode flips.

<!-- citations: [^cf4c4-2] [^7e35e-8] [^7e35e-9] [^04b5f-4] [^4243e-2] [^rollo-27] [^c1691-137] -->
## Snapshot Registry

The SnapshotRegistry supports opt-in per-projection change-gating via a ChangeGate trait and register_gated method, with per-key memoization and panic isolation, preserving backward compatibility with the always-run register method. The podcast app originally registered podcast.snapshot via register_snapshot_projection_gated using handle.rev as the ChangeGate, so the registry skipped the full-library serializer on unchanged rev; this dead registration was subsequently deleted as PR-1 of the domain sub-projections work (PR #396), since the v0.5.0 typed-first encoder discards the generic projections map entirely.

<!-- citations: [^c1691-20] [^c1691-72] -->
## Host-Op Routing

The dispatch host-op router uses namespaced envelopes (ns + action) to route to the correct handler, eliminating the try-parse cascade that could cause wiki to silently hijack knowledge.search. <!-- [^c1691-21] -->

## Substate Migrations

The Inbox substate migration (PR #383) removed inbox fields from both god-structs and lifted maybe_enqueue_triage out of the snapshot builder (D8: projections must be pure closures), re-homing it to the feed-refresh and cold-start auto_download_evaluate paths, with a test asserting the projection does not set triage_in_progress. A cold-start triage trigger fires via the auto_download_evaluate dispatch, and a decoupled refresh_all fires triage even on all-304. The triage relocation was initially caught by an Opus review for dropping triage in three scenarios: cold launch (iOS skips RefreshAll), stale-Pending retry after failed triage, and fresh subscribe — the implementer fixed cold-start and 304-decouple before merge.

<!-- citations: [^c1691-22] [^c1691-43] [^c1691-69] -->
## Float & Duration Safety

PR #384 added finite guards at the parse_duration inlet (.ok().filter(|v| v.is_finite() && *v >= 0.0)), projection-side serialize_with guards on 9 required float fields, and Swift KernelDecoding golden fixture tests for embedded types. NaN and Infinity float values from malformed feed durations are rejected at the inlet (parse_duration) and clamped to zero at the projection boundary (finite_f64_or_zero / finite_f32_or_zero), preventing the remotely-triggerable UI-freeze vector (issue #371 class). The golden snapshot fixture (3789 bytes) remained byte-identical throughout all kernel-touching PRs across cycles 1-8, serving as the structural regression gate. The byte-identical output is preserved across all strangler steps and subsequent PRs. Swift KernelDecoding golden fixture tests are covered under this section.

<!-- citations: [^c1691-23] [^c1691-39] [^c1691-55] [^c1691-70] [^c1691-138] [^c1691-155] [^c1691-206] -->
## Swift Golden Fixture Tests

KernelDecoding golden fixture tests are covered under Float & Duration Safety.

<!-- citations: [^c1691-24] [^c1691-40] -->

## Push-Pipeline Status

The iOS push path was dead since NMP v0.3.0 (it required the podcast.snapshot projection which was deleted), and Android was clobbering the UI to an empty state on every kernel emit. The slice-local payload builders kill the 1 Hz whole-library rebuild during playback by building per-domain payloads from individual store queries rather than reconstructing the full snapshot. Android per-domain push consumption (PR #404) fully removes the old decodeEnvelope+SnapshotEnvelope path (which decoded the slim v (rev/running/schema_version only) directly as a PodcastSnapshot with all fields defaulted, clobbering the UI to an empty state on every emit) and replaces it with per-domain sidecar consumption plus rev-monotonic drop guards, applying merged state only when anyAccepted==true, preventing the empty-clobber bug. The domain-projection kernel tombstone contract emits {"rev":N,"<field>":null} for emptied domains and advances last_emitted, allowing shells to learn about cleared/unsubscribed/all-dismissed states rather than re-running the builder every tick forever; this breaks the empty-domain rebuild-every-tick loop. The ActionResultsRegistry uses a buffered-once pattern (NSLock-protected dictionary buffers results arriving before the await is registered), so a result frame arriving between dispatchSilent and awaitResult is never lost. The hasHydrated flag in KernelModel limits the rev>= guard to only the first cold-start pull, restoring the normal > monotonic guard for all subsequent push and pull frames, with resetAndRestart() resetting hasHydrated to false. This ensures that a redundant full cold-start pull can re-seed the composite even if a partial push already consumed the rev, while steady-state uses strict > monotonic guard.

<!-- citations: [^c1691-42] [^c1691-56] [^c1691-71] [^c1691-88] [^c1691-102] [^c1691-117] [^c1691-136] [^c1691-179] [^c1691-223] [^c1691-243] [^c1691-255] -->
## Android Kernel Projections

Android Tier-2 Inbox and Transcripts are now on the shared kernel: new InboxScreen.kt renders triaged items with shimmer off inbox_triage_in_progress, and EpisodeDetailScreen.kt shows transcript lifecycle status, both driven by kernel projection data through existing JNI. <!-- [^c1691-44] -->
