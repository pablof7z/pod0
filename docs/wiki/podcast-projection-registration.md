---
title: Podcast Projection Registration
slug: podcast-projection-registration
summary: The podcast projection is registered through the canonical nmp_app_register_snapshot_projection seam and rides the reactive push frame.
tags:
  - nmp
  - projection
  - push
  - registration
  - snapshot
volatility: cold
confidence: medium
created: 2026-05-29
updated: 2026-06-02
verified: 2026-05-29
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:e1cfd663-230d-4f78-9078-0c9ed8b6a4bb
---

# Podcast Projection Registration

> The podcast projection is registered through the canonical nmp_app_register_snapshot_projection seam and rides the reactive push frame.

## Registration

The podcast projection is registered through the canonical `nmp_app_register_snapshot_projection` seam. In Rust, this is `NmpApp::register_snapshot_projection(key, f)` where `f: impl Fn() -> serde_json::Value + Send + Sync + 'static`. The podcast crate registers `"podcast.snapshot"` with a closure that calls `build_snapshot_payload(&handle)` — the same function the pull path uses, ensuring byte-identical output and benefiting from the rev-gated cache. <!-- [^14943-7] -->


When an agent publishes a podcast, the user is not auto-subscribed to it; the synthetic podcast creates a Podcast row but no PodcastSubscription row, so it appears in 'See All' but not in the user's followed-podcasts list. <!-- [^e1cfd-7] -->
## Handle Ownership

The `PodcastHandle` is wrapped in `Arc<PodcastHandle>` so the projection registry can hold one reference while Swift holds the other. Registration uses `Arc::into_raw(handle) as *mut PodcastHandle`. The unregister path uses `Arc::from_raw`. This is sound because `PodcastHandle` is already `unsafe impl Send + Sync` and no FFI function takes `&mut *handle`. <!-- [^14943-8] -->

## Projector Output

The projector closure calls `build_snapshot_payload(&proj)`, then parses it back to `serde_json::Value` via `serde_json::from_str`. On failure it returns `serde_json::Value::Null`. The `build_snapshot_payload` function includes a D5-consistent fallback: when serialization fails, it returns a valid stub `{"running":true,"rev":0,"schema_version":1,...}` rather than an empty or null payload. This ensures the Swift decoder always receives a structurally valid `PodcastUpdate`, even under error conditions. <!-- [^14943-9] -->

## Delivery Model

Once registered, the projection is appended to `KernelSnapshot::projections["podcast.snapshot"]` on every kernel tick where `changed_since_emit` is true. This is a pure push — no polling, no pull. The kernel emits on dispatched host-ops (via `maybe_emit_after_dispatch`) and on change-gated ticks. The projector function is called during `make_update`; it must be non-blocking (D8) and fast, hence the rev-gated cache inside `build_snapshot_payload`. <!-- [^14943-10] -->

## Poll Replacement

With the projection registered on the push seam, the old 500ms `startSnapshotPoll` loop is deleted. All updates are event-driven:
- Dispatched host-ops: already trigger a kernel emit, so the push carries the projection
- Shell-initiated reports (audio, download, voice): the shell fires a one-shot `pullPodcastSnapshotIfChanged()` via `onSnapshotMaybeChanged` hook
- Startup: a one-shot pull after `start()` captures the persisted library
This is not polling — it's event-driven, with zero idle work. <!-- [^14943-11] -->

## See Also
- [[nmp-update-transport|NMP Update Transport (FlatBuffers Push)]] — related guide
- [[reactive-update-model|Reactive Update Model (No Polling)]] — related guide
- [[nmp-integration-rules|NMP Integration Rules]] — related guide

