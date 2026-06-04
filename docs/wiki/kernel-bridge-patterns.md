---
title: Kernel Bridge Patterns
slug: kernel-bridge-patterns
summary: "Patterns for the Swift↔Rust kernel bridge: observation loop, audio report threading, play dispatch, store capture, and position persistence."
tags:
  - kernel
  - bridge
  - swift
  - rust
  - playback
  - observation
volatility: warm
confidence: medium
created: 2026-05-29
updated: 2026-06-04
verified: 2026-05-29
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:c43d5e77-d667-4e71-a574-47aaab5b6a7a
---

# Kernel Bridge Patterns

> Patterns for the Swift↔Rust kernel bridge: observation loop, audio report threading, play dispatch, store capture, and position persistence.

## attachKernel Loop Pattern

The `kernelObservationTask` in `AppStateStore+KernelProjection.swift` uses an apply-before-await pattern to eliminate a `withObservationTracking` race condition:

```swift
kernelObservationTask = Task { @MainActor [weak self] in
    while !Task.isCancelled {
        // Apply current state FIRST, then arm the observation for the next change.
        self?.applyKernelState(library: kernel.library, snapshot: kernel.podcastSnapshot)
        await withCheckedContinuation { continuation in
            withObservationTracking {
                _ = kernel.library
                _ = kernel.podcastSnapshot
            } onChange: {
                continuation.resume()
            }
        }
        guard !Task.isCancelled else { break }
    }
}
```

This ensures the latest state is applied regardless of when the task started relative to property changes. Without this, `withObservationTracking` arms on an already-final value and the `onChange` never fires, leaving the UI empty. <!-- [^14943-43] -->

## play() Must Dispatch to Kernel

The `PlaybackState.play()` method must call `store?.kernelLoad(episodeID: episode.id)` after `engine.play()`. This dispatches to the Rust `PlayerActor` so it receives the `episode_id` for position writeback, played marking, and auto-advance. Without this, Swift-initiated plays are invisible to Rust — position is never persisted and auto-advance is broken. <!-- [^14943-44] -->

## Audio Callback Store Capture

The `audio.commandHandler` closure in `PlaybackState+AudioCallbacks.swift` must capture the store via `[weak self]` and access `self.store?.episode(id: id)` inside the closure body — NOT via `[weak self, weak store = self.store]`. The latter captures `store` at init time when it may be nil, causing every Rust-originated `.load` command to fail. <!-- [^14943-45] -->

## Audio Report Threading

The `attachAudioReportChannel()` method must dispatch the `nmp_app_podcast_audio_report` FFI call to a background serial queue (`DispatchQueue(label: "podcast.audio-report", qos: .utility)`), not call it directly on `MainActor`. This prevents a deadlock: when `ItemEnd` triggers `maybe_auto_advance`, Rust calls `dispatch_capability` → `SyncCapabilityBridge.handle` → `DispatchQueue.main.sync` — which deadlocks if the calling thread is already main. The follow-up `AudioCommand` execution is re-dispatched to `@MainActor` via `Task { @MainActor in ... }`. <!-- [^14943-46] -->

## PodcastHandle Sendable

`PodcastHandle` is declared as `final class PodcastHandle: @unchecked Sendable`. This is intentional: it is an FFI bridge class passed across threads (e.g., captured in `@Sendable` `DispatchQueue.async` closures). The `@unchecked` annotation acknowledges that the compiler cannot verify thread safety for the raw FFI pointers it wraps. <!-- [^14943-47] -->


FFI dispatch functions (dispatch_audio, dispatch_download, dispatch_notification) must null-guard their app pointer dereferences. A null app causes SIGABRT, violating D6 (errors-as-data, never crash out of FFI). The publish handler already null-guards its dispatch; the audio, download, and notification paths must do the same. The guard also enables unit-testing of handler functions with a null app pointer. <!-- [^14943-39] -->
## Widget Position Updates

`NowPlayingSnapshotStore.updatePosition` must be called from `onPlayingTick` to keep the widget in sync. It calls `WidgetCenter.shared.reloadAllTimelines()` internally, so it should be throttled (not called on every 1-second tick). <!-- [^14943-48] -->

## Segment-End Advancement

The `onPlayingTick` handler in `PlaybackState+AudioCallbacks.swift` watches `currentSegmentEndTime`. When `position >= currentSegmentEndTime`, it pops the next item from the queue via `playNext(resolve:)` or pauses the engine if the queue is empty. The watch uses `[weak self]` to avoid retaining the playback state. <!-- [^14943-49] -->

## Seek and Skip Position Persistence

When paused, `seek(to:)`, `skipBackward`, and `skipForward` must persist the new position to Rust's store via `store?.kernelPersistPosition(episodeID:positionSecs:)`. This dispatches `op:"persist_position"` which writes `position_secs` directly to the store without returning an `AudioCommand`. When playing, the engine's time observer handles position reporting. <!-- [^14943-50] -->


## Kernel Dispatch API

Kernel action dispatches from Swift must use typed wrapper methods (e.g., `kernelPublishAgentNote`) that encapsulate dictionary construction; call sites never see raw string-keyed dictionaries.

<!-- citations: [^c43d5-13] [^c43d5-23] -->
## See Also
- [[reactive-update-model|Reactive Update Model (No Polling)]] — related guide
- [[known-bug-patterns|Known Bug Patterns]] — related guide
- [[playback-engine-m1-part3|Playback Engine (M1 Part 3 Engine Swap)]] — related guide

