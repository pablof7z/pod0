---
title: Kernel Async Dispatch Patterns
slug: kernel-async-dispatch
summary: "Inbox triage runs off the actor thread via `tokio::spawn` with a re-entrancy guard (`compare_exchange`) and incremental rev bumps"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-31
updated: 2026-05-31
verified: 2026-05-31
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
---

# Kernel Async Dispatch Patterns

## Kernel Async Dispatch

Inbox triage runs off the actor thread via `tokio::spawn` with a re-entrancy guard (`compare_exchange`) and incremental rev bumps. Agent chat and wiki synthesis run off the actor thread with a `placeholder-is_generating` pattern and `find-by-id` in-place update. The social handler relay fetch runs off the actor thread so `kind:3` and `kind:0` contacts don't block the kernel; the social graph handler fetches `kind:3` and `kind:0` from the relay on a background thread. [^14943-104]

## See Also

