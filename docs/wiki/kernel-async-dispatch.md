---
title: Kernel Async Dispatch Patterns
slug: kernel-async-dispatch
summary: "Inbox triage runs off the actor thread via `tokio::spawn` with a re-entrancy guard (`compare_exchange`) and incremental rev bumps"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-31
updated: 2026-06-04
verified: 2026-05-31
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:6706236b-c94a-4458-aa7b-6f71098aa55b
---

# Kernel Async Dispatch Patterns

## Kernel Async Dispatch

Inbox triage, agent chat, and wiki LLM handlers use a re-entrancy guard via AtomicBool::compare_exchange(false, true, Acquire, Relaxed) to prevent concurrent overlapping dispatches. Inbox triage also uses incremental rev bumps. Agent chat and wiki synthesis run off the actor thread with a placeholder-is_generating pattern and find-by-id in-place update. The social handler relay fetch runs off the actor thread so kind:3 and kind:0 contacts don't block the kernel; the social graph handler fetches kind:3 and kind:0 from the relay on a background thread. All LLM and network calls on the actor thread must be moved off-actor via runtime.spawn(async { tokio::task::spawn_blocking(||...).await }) to avoid blocking the kernel. Async tasks that fill placeholders must find their target row by UUID (find-by-id), never by last_mut(), to avoid corruption under concurrent sends or clears. The voice conversation manager uses an explicit shutdown() called from nmp_app_podcast_unregister before nmp_app_free, with abort+join of in-flight handles to prevent use-after-free. After the agent triage call completes, any needy episode still missing a fresh Ready entry in the cache must get stamp_pending (reconcile), preventing a hot-spawn loop that bypasses the 10-minute cooldown guarantee.

<!-- citations: [^14943-104] [^14943-109] [^14943-144] [^67062-12] -->
