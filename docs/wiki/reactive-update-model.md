---
title: Reactive Update Model (No Polling)
slug: reactive-update-model
summary: The app is fully reactive — no polling. Updates flow via kernel push frames for dispatched changes and event-driven one-shot pulls for shell-initiated reports.
tags:
  - reactive
  - push
  - emit
  - poll
  - event-driven
volatility: cold
confidence: medium
created: 2026-05-29
updated: 2026-06-02
verified: 2026-05-29
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:8bfa1b91-b40c-44b3-acb9-245b36f4c841
---

# Reactive Update Model (No Polling)

> The app is fully reactive — no polling. Updates flow via kernel push frames for dispatched changes and event-driven one-shot pulls for shell-initiated reports.

## Emit Model

The NMP kernel emits push frames only when `changed_since_emit` is true — there is no steady timer. The `actor/tick.rs` `compute_wait`/`flush_due` functions gate emission on change. The `DispatchHostOp` arm in `actor/dispatch.rs` calls `maybe_emit_after_dispatch` after processing each dispatched host operation. The `idle_ticks_do_not_emit` test pins this contract. A host seam `ActorCommand::MarkChangedSinceEmit` exists to force an emit when needed. <!-- [^14943-22] -->

## Podcast rev Bumps

The podcast `rev` (`Arc<AtomicU64>`, starts at 1) is bumped synchronously inside `DispatchHostOp` handlers. No background tokio task bumps `rev` autonomously — the only `tokio::spawn` in the podcast crate is `relay.rs` (scoped, never touches `rev`). Everything else (categorization, ai_chapters, transcript, agent chat, discovery, comments) bumps `rev` inside a `DispatchHostOp` handler, and `DispatchHostOp` already calls `maybe_emit_after_dispatch`. The audio, download, and voice report FFI functions also bump `rev` but do NOT trigger a push emit — they were designed to be followed by a pull. <!-- [^14943-23] -->

## Event-Driven Replacement

NMP apps must be event-driven — UI reads from kernel snapshot projections reactively rather than using local polling-based loading states. Legitimate triage indicators (e.g., `triage_in_progress` for LLM operations) are allowed; the prohibition is on polling-based spinners, not on all loading indicators. The 500ms poll is replaced with an event-driven system using a `onSnapshotMaybeChanged` hook (PR #136 enforced this by removing the poll):

1. **Dispatched host-ops:** already trigger a kernel emit via `DispatchHostOp` → `maybe_emit_after_dispatch` → the registered projection rides the push frame → `apply()` handles it.

2. **Shell-initiated reports:** `PodcastHandle` exposes `var onSnapshotMaybeChanged: (() -> Void)?`. After each audio report, download report, and voice report FFI call, the callback fires `self.onSnapshotMaybeChanged?()`. `KernelModel` wires this to `pullPodcastSnapshotIfChanged()` — a one-shot, rev-gated pull.

3. **Startup:** `KernelModel.start()` and `resetAndRestart()` call `pullPodcastSnapshotIfChanged()` once after kernel start to capture the persisted library.

All three report channels (audio, download, voice) must fire the hook. Missing any channel causes that report's state updates to go stale.

<!-- citations: [^14943-24] [^14943-126] [^8bfa1-5] -->
## Poll Deletion

The `startSnapshotPoll()` method is deleted entirely. The `snapshotPollTask` field is removed from `KernelModel`. The 500ms `Task.sleep(for: .milliseconds(500))` loop is gone. Idle work goes from 2 pulls/second to zero. The `dispatch`/`dispatchSilent` methods retain their one-shot `pullPodcastSnapshotIfChanged()` call for instant post-action feedback — this is not polling; it fires exactly once per user action. <!-- [^14943-25] -->


The poll elimination was validated by a background NMP audit that confirmed all podcast rev bumps are synchronous (inside DispatchHostOp or shell-initiated FFI reports), and nothing bumps rev on a background tokio task. The 'empty library on relaunch proves the poll is needed' claim was confounded by an in-memory persistence quirk — after fixing persistence, the library loads with no poll. The final verification: launch loads the full library + inbox + picks reactively, inbox picks updated 6→8 via push (a host-op change), playback tracks via the audio-report hook, and 0 decode failures. <!-- [^14943-76] -->
## Verification

With the poll removed, the app was verified live: launch loads the full library (Hard Fork, Lex Fridman, The Daily) + inbox + picks reactively via the one-shot startup pull. The picks count updated 6→8 via the push frame (a host-op change, no poll). Playback tracks via the audio-report hook. Zero decode failures. <!-- [^14943-26] -->

## See Also
- [[podcast-projection-registration|Podcast Projection Registration]] — related guide
- [[security-and-constraints|Security and Constraints]] — related guide
- [[kernel-bridge-patterns|Kernel Bridge Patterns]] — related guide
- [[nmp-integration-rules|NMP Integration Rules]] — related guide
- [[playback-engine-m1-part3|Playback Engine (M1 Part 3 Engine Swap)]] — related guide

