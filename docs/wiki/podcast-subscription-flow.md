---
title: Podcast Subscription Flow
slug: podcast-subscription-flow
topic: data-persistence
summary: Subscribing to a podcast inserts the row and marks it followed synchronously (no network) so it appears instantly in the library
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-08
updated: 2026-06-14
verified: 2026-06-08
compiled-from: conversation
sources:
  - session:8eb3f00f-b245-4f03-80f0-15151d9aba28
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
  - session:rollout-2026-05-10T00-02-36-019e0e8c-9c89-7dc1-9942-c63cb0efd9c4
  - session:rollout-2026-05-11T08-21-02-019e157b-4b15-77a0-8003-a3ae75cf8c26
  - session:rollout-2026-05-11T08-21-02-019e157b-4d93-7042-aab8-cc756f719dcd
  - session:rollout-2026-05-13T16-51-04-019e219a-f6d8-78d2-8c63-e09938281252
---

# Podcast Subscription Flow

## Subscribe Flow

Subscribing to a podcast inserts the row and marks it followed synchronously (no network) so it appears instantly in the library. The Rust kernel `handle_subscribe` previously blocked on the entire RSS feed download and parse before inserting the podcast row, causing multi-second delays. <!-- [^8eb3f-1] -->

On a feed-fetch error, the optimistic row is kept with no episodes until pull-to-refresh; surfacing that error is a tracked follow-up. <!-- [^8eb3f-2] -->


Initial back-catalog episodes from a new subscription do not trigger auto-download; only episodes discovered via refresh after the subscription exists are eligible for auto-download. <!-- [^rollo-30] -->

Initial subscribe and import paths disable auto-download evaluation so back-catalog imports do not immediately download everything, while future refreshes do evaluate auto-download. <!-- [^rollo-91] -->

The 'newly subscribed' heuristic must use `PodcastSubscription.subscribedAt` rather than inferring newness from low signal counts, to avoid over-surfacing long-followed sparse feeds. <!-- [^rollo-153] -->
## Async Feed Fetch

The feed fetch and episode hydration runs in the background via an async HTTP capability, not on the actor thread. The async feed fetch is dispatched fire-and-forget over a new `nmp.http.async.capability` namespace, and the platform reports results back via a new `nmp_app_podcast_http_report` FFI (mirroring the download capability pattern). The `FeedFetchCoordinator` parses and merges episodes off the actor thread and re-projects after hydration completes. <!-- [^8eb3f-3] -->

Feeds route through the iOS HTTP capability by doctrine (D7 — capabilities execute, never decide), not by technical necessity; the kernel already does direct `reqwest` for LLM/transcription. The user chose the async HTTP capability with report-back approach (like downloads) rather than direct `reqwest`, making the fix a multi-platform change across the kernel contract, iOS, Android, and TUI. <!-- [^8eb3f-4] -->

The existing synchronous HTTP path remains unchanged for iTunes search, transcripts, and chapters. <!-- [^8eb3f-5] -->


The headless async HTTP capability host (capability_host.rs) must handle nmp.http.async.capability by spawning a std thread for reqwest and calling nmp_app_podcast_http_report to feed results back to the kernel's FeedFetchCoordinator; without it RSS subscribes produce empty placeholders. <!-- [^c1691-365] -->
## iOS Implementation

The iOS `SyncCapabilityBridge` is the live capability-callback router and must own the async route and report sink (not `PodcastCapabilities.shared`). The iOS `HttpCapability` `Result<URLRequest, HttpResult>` was replaced with a purpose-built enum because `HttpResult` does not conform to `Error`. The iOS async HTTP path uses `executeAsync` on `HttpCapability`, which starts a background URLSession task and returns immediately with an ack envelope. <!-- [^8eb3f-6] -->

## Android Implementation

The Android `nativeHttpReport` JNI returns void because `nmp_app_podcast_http_report` always returns NULL (no follow-up command), unlike the download report which returns a `jstring`. The Android `HttpReport.result` field must be a nested JSON object (not a stringified JSON) to correctly round-trip through Rust serde; stringifying it would silently fail the decode. The Android `HttpCapability` shares a single `perform` function between the sync and async paths, reusing the existing OkHttp client. <!-- [^8eb3f-7] -->

## Architecture Constraints

The `PodcastHandle`'s `self.app` pointer is only valid on the actor thread (per ADR-0023), so `dispatch_http` cannot be moved to a background task without a separate async report-back path. <!-- [^8eb3f-8] -->

## Housekeeping

The `ios/Podcast/` mirror has no build target and is an unbuilt legacy tree; changes need not be mirrored there. A pre-existing broken test (`opml_roundtrip.rs` referencing the deleted `PodcastKind`) was fixed as part of this change. <!-- [^8eb3f-9] -->

The owned-podcast kind:54 backfill uses self-enqueue (nmp_app_dispatch_action) to dispatch per-episode publish_episode actions in separate actor ticks, avoiding a synchronous blocking loop that would stall the actor during a private-to-public flip. <!-- [^c1691-33] -->

OPML import fetches all feeds first, then commits new subscriptions and episodes to the store in a single batched mutation, deferring projection rebuilds, persistence, Spotlight, widgets, and iCloud side effects until the batch completes. <!-- [^rollo-31] -->

OPML import's 'background' copy overstates durability because fetched feeds are held in memory and committed only after the whole loop completes, meaning killing the app mid-import loses successful work. <!-- [^rollo-90] -->

Subscription context menus (Refresh/Unsubscribe) must be extracted into a shared `SubscriptionContextMenu` component instead of being duplicated across list rows and grid cells. <!-- [^rollo-102] -->

The feed-304-rev-bump audit item is done: podcast_actions_feed.rs explicitly skips the rev bump on NotModified with a comment. <!-- [^c1691-366] -->
