---
title: Audio Engine
slug: audio-engine
topic: playback
summary: AudioEngine's handleEndOfItem() is intentionally neutral; autoplay-next is deferred to Lane 2/4
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-13
updated: 2026-06-14
verified: 2026-05-13
compiled-from: conversation
sources:
  - session:0f3f24f7-54de-49f8-b160-a92f735f6a00
  - session:f11c47b8-a7bd-47d3-9eb0-79dd02904d04
  - session:ce9e0cdb-a00d-4c13-ad7e-93e3dced2648
  - session:e1ab0629-64bc-4383-bd22-c0843ca16a99
  - session:c33b9adb-9d1a-4717-9314-b45a61e6cbc3
  - session:38f8143c-c90d-49e3-a8fa-8d5ca17ac319
  - session:rollout-2026-05-10T20-46-07-019e12ff-1573-7b82-ba04-59c91f91ebce
  - session:rollout-2026-05-11T08-21-01-019e157b-4863-7563-a43b-8405491d88a1
  - session:rollout-2026-05-11T09-10-30-019e15a8-9491-7d33-9bbf-ee806e2f875c
  - session:rollout-2026-05-26T09-10-44-019e62e8-3112-7080-98ca-48ac46d8b8d2
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Audio Engine

## Playback Behavior

AudioEngine's handleEndOfItem() is intentionally neutral; autoplay-next is deferred to Lane 2/4. queue_position:"now" preserves existing Up Next items and pushes the new item to the front. A PlaybackState.insertNext method inserts a queue item at position 0 to support queue_position:"next". When an AVPlayerItem fails to load a stale or invalid file URL, the audio engine enters a .failed state with an EngineError, resulting in no audio playback and the current time remaining at 0. Audio position polling and fake progress (issue #322) is fixed by PR #344.

AudioEngine.load(_:) must reset currentTime and duration to prevent a new episode from inheriting a prior episode's stale playhead state.

Same-episode setEpisode must refresh AudioEngine metadata and rewind replay after natural end. An explicit engine.refreshEpisodeMetadata(_:) should be added, and on same-ID "play again" the player should seek to the incoming persisted position (usually 0) before play. Duration refresh only replaces the engine duration when the engine was still using feed metadata, leaving an already-resolved asset duration alone.

playNext must prune stale queue IDs until it finds a resolvable episode, preventing autoplay from stalling behind a single stale queue head item.

Natural-end handling must replace the indirect 1-second polling loop with direct observation, and queue tests must cover the finish-to-mark-played path using actual setEpisode/play/AVFoundation semantics.

Playback rate presets must be centralized so the remote command center exposes the same 0.5...3.0 range supported by the UI and clamped by the engine.

The iOS audio engine plays on its own path independent of the kernel Load, which is why the mini-player can show playback advancing and MPNowPlayingInfoCenter populate while the widget JSON stays idle.

Playing/BufferingProgress audio ticks do not bump the global kernel rev; their now_playing state rides an inline AudioReportResponse payload so the scrubber/Dynamic Island/lock screen stay live without triggering a full-library rebuild.

Structural audio events now use per-domain DomainRevs and typed sidecar projections wired at every mutation site; a playback mutation bumps only the playback domain rev while the global rev still advances for the pull path, rather than triggering the full-library snapshot path. (Previously: Structural audio events (play/pause/stop/track-end) are durable and still bump the global rev, triggering the full snapshot path, superseded — see kernel-projections.)

During confirmed steady playback, applyKernelState and recomputeEpisodeProjections fire zero times on the main thread. (Previously: applyAudioReport still calls the rev-gated pull every tick (cheap atomic probe, no decode), preserving the side-channel that flushes background work (triage, categorization) to the UI during playback, superseded — see podcast-app-state.)

NowPlayingCenter must remove MPRemoteCommandCenter targets and AudioEngine must provide a cleanup path to prevent unit tests from registering global command handlers. Now Playing command registration must be made explicit via either a dedicated teardown method or a singleton command registrar.

maybe_auto_advance in audio_report.rs now returns early if the actor lock is poisoned, preventing the dispatch of Load+Play without a staged record — the exact bug class of the earlier lock-screen-play fix.

PlaybackQueueTests and PlaybackAutoPlayNextTests must be consolidated and include a test case for a stale queue head followed by a valid tail.

PR #373 includes a regression-pin seam test (snapshot_widget_seam_tests.rs) that drives the real uppercase play host-op + AudioReport::Playing and asserts both now_playing and widget populate, verified red-without/green-with.

Play Latest is intentionally hidden from shortcuts until it routes through the Rust-owned Siri action instead of Swift choosing the episode.

Swift symbol deletions must be verified with xcodebuild build-for-testing because the test target globs AppTests/** and may reference deleted symbols that the main target won't catch.

<!-- citations: [^e1ab0-1] [^e1ab0-2] [^e1ab0-3] [^0f3f2-14] [^f11c4-2] [^ce9e0-1] [^c33b9-1] [^38f81-1] [^rollo-53] [^rollo-76] [^rollo-124] [^rollo-221] [^c1691-348] [^c1691-369] [^c1691-381] [^c1691-398] [^c1691-430] -->
