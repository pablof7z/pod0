---
title: Blossom Upload FFI
slug: blossom-upload-ffi
topic: nostr-protocol
summary: Blossom uploads route through the kernel's `nmp.blossom.upload` typed action; Swift writes a temp file, dispatches `dispatchSilent`, and then awaits the `BlobDe
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-13
updated: 2026-06-13
verified: 2026-06-13
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Blossom Upload FFI

## Blossom Upload FFI

Blossom uploads route through the kernel's `nmp.blossom.upload` typed action; Swift writes a temp file, dispatches `dispatchSilent`, and then awaits the `BlobDescriptor.url` from the `action_results` sidecar — no signing or URLSession in Swift (D13/D0 compliance). `ActionResultsRegistry` mirrors `SignedEventsRegistry` with drain-once semantics, `NSLock`-protected buffering, and a 60-second race timeout; a result arriving between dispatch and await registration is buffered and consumed without loss. The `BlossomUploader.swift` and `BlossomUploaderTests.swift` were both deleted, with zero remaining references in the Swift target (verified via grep and `build-for-testing`). Sign+transport uses `SignEventForAccount` supporting both nsec and NIP-46 bunker. NMP v0.6.0+ provides nmp-blossom (typed upload action with D8-safe worker-thread HTTP) and nmp-nip02 (reactive FollowListProjection + ActiveFollowSet), making previous BACKLOG designs for hand-rolled versions obsolete. The kernel bunker-signing path is real (nmp-core `publish_unsigned_event` → `sign_active_nonblocking` → handles NIP-46 `PendingSign`); the Swift comment claiming 'bunker stays Swift-side' was false and has been corrected. Blossom audio-path migration (per-podcast NIP-F4 keys) is blocked upstream because nmp-blossom's `signer_pubkey` resolves against the NMP account roster, and there is no API to register per-podcast keys.

<!-- citations: [^c1691-229] [^c1691-237] [^c1691-247] [^c1691-264] [^c1691-279] -->
