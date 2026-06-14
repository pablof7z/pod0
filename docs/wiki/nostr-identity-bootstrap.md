---
title: Nostr Identity Bootstrap
slug: nostr-identity-bootstrap
topic: nostr-protocol
summary: The app must never show a 'no identity' state — if no identity exists for either the agent or the human user, the app must auto-create one
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-10
updated: 2026-06-14
verified: 2026-06-10
compiled-from: conversation
sources:
  - session:4243e533-7577-4916-afae-773f1c45b9f2
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Nostr Identity Bootstrap

## No-Identity Auto-Creation

The app must never show a 'no identity' state — if no identity exists for either the agent or the human user, the app must auto-create one. On fresh install, `applyKernelIdentity` auto-generates a key on the first nil-pubkey tick using a one-shot `_autoKeygenDispatched` guard to prevent re-dispatch on subsequent nil ticks. `clearIdentity()` resets `_autoKeygenDispatched` to `false` so that a fresh keygen can fire after identity is cleared. <!-- [^4243e-3] -->

In the Android `MainActivity`, on first snapshot with `activeAccount == null` and no stored nsec, the app dispatches `IdentityActions.generate(bridge)` and then explicitly pulls the updated snapshot via `bridge.podcastSnapshot()` to surface the new identity, because `dispatchAction` is async and no NMP-core push frame fires after Generate. <!-- [^4243e-4] -->

## Generate Key Pair Button

The 'Generate Key Pair' button must produce a visible identity when tapped. <!-- [^4243e-5] -->

The Android `IdentityScreen` 'Generate Key Pair' button handler calls `IdentityActions.generate(bridge)` then `onSnapshotPull()` to explicitly pull the updated snapshot, because the podcast app's rev bump does not trigger an NMP-core push frame. <!-- [^4243e-6] -->

## Kernel Dispatch for Identity Actions

`dispatchKernelKeygen()` dispatches `podcast.identity Generate` via the action module (`dispatchToKernel`) instead of calling `nmp_app_create_new_account` FFI directly, because the FFI path never updates the app-local `IdentityStore` that drives `PodcastUpdate.active_account`. `importNsec()` dispatches `podcast.identity ImportNsec` via the action module instead of calling `nmp_app_signin_nsec` FFI, for the same reason as `dispatchKernelKeygen`. After `dispatchKernelKeygen()` and `importNsec()`, `kernel?.requestSnapshotPull()` is called to surface the new identity in the UI, mirroring the pull-after-dispatch pattern used by other kernel actions. <!-- [^4243e-7] -->

`IdentityHandler` in Rust uses `SnapshotUpdateSignal.bump()` (which sends `ActorCommand::MarkChangedSinceEmit`) instead of direct `self.rev.fetch_add(1)`, so NMP-core emits a fresh push frame with updated identity after Generate/ImportNsec/Clear actions. <!-- [^4243e-8] -->

## Android Staleness Guard

Android push consumption replaces the empty-snapshot-clobber path with per-domain sidecar merging; absent domains are left untouched via copy() and frames with no accepted domains are skipped entirely, fully removing the blank-library bug. (Previously: In the Android push loop, a staleness guard must preserve `cur.rev` when a stale push frame is intercepted: `next.copy(activeAccount = cur!!.activeAccount, rev = cur.rev)` — not just `next.copy(activeAccount = cur!!.activeAccount)` — to prevent subsequent stale frames with intermediate revs from bypassing the guard and wiping the account. <!--  -->, superseded — see domain-revisions.)


Social state (following list + agent notes) must be cleared on account switch to prevent cross-account data leakage. <!-- [^c1691-245] -->
## Testing

Seven unit tests in `UserIdentityBootstrapTests.swift` cover both identity bugs (auto-keygen and generate-key dispatch) using `_keygenCallRecorder` and `_pullCallRecorder` test seams rather than `KernelDispatchRecorder`, because `dispatchKernelKeygen` routes through `dispatchToKernel` not direct FFI. The identity fix must be tested on both iOS (Xcode MCP) and Android (ADB) before merging. <!-- [^4243e-10] -->

## Profile Publication Gap

After publishing a kind:0 profile, the published fields never appear in the local identity projection because handle_publish_profile never touches IdentityStore and no kind:0 observer exists to catch relay echoes. <!-- [^c1691-451] -->
