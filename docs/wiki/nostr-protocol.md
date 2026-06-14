---
title: Per-Podcast NIP-F4 Signing Migration
slug: nostr-protocol
topic: nostr-protocol
summary: "Per-podcast NIP-F4 keys are registered via AddSigner { make_active: false } and routed through PublishRaw { signer_pubkey } / nmp.blossom.upload { signer_pubkey"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-13
updated: 2026-06-14
verified: 2026-06-13
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Per-Podcast NIP-F4 Signing Migration

## Per-Podcast NIP-F4 Signing Migration

Per-podcast NIP-F4 keys are registered via AddSigner { make_active: false } and routed through PublishRaw { signer_pubkey } / nmp.blossom.upload { signer_pubkey }; app-side secp256k1 signing and hand-rolled BUD-02 Blossom upload are retired. AddSigner with make_active: false is idempotent: IdentityRuntime::add keys by pubkey hex, so re-registration overwrites the slot without duplicating order entries or flipping the active account. The publish path is FIFO-safe because register_podcast_signer_in_kernel and the publish dispatch both go through the single MPSC actor queue (nmp-ffi send_cmd), and local-key registration is synchronous, so the signer is always present when the sign-time lookup fires. nmp_signer_broker_init is safe to add to Android's nativeNew: it is idempotent (OnceLock), called once, and the broker module is independent of the NIP-55 external-signer path.

<!-- citations: [^c1691-300] [^c1691-315] [^c1691-331] [^c1691-344] [^c1691-361] [^c1691-375] [^c1691-388] [^c1691-407] [^c1691-417] [^c1691-436] [^c1691-452] -->
## Blocking Gaps for Full Blossom-Audio-Path Retirement

Blossom audio-path migration is blocked on NMP issue #1321 because per-podcast keys registered via AddSigner{make_active:false} appear in the user-visible account switcher and do not persist across kernel restarts without app re-registration; only a partial retirement (register inactively, accept temporary UX pollution) is actionable now. Issue #1321 requests a hidden/app-managed account flag and non-active-key persistence so per-podcast NIP-F4 keys can be registered as signers without appearing in the user-visible account switcher and without the app needing its own PodcastKeyStore.

<!-- citations: [^c1691-316] [^c1691-333] [^c1691-345] [^c1691-362] [^c1691-376] [^c1691-389] [^c1691-408] [^c1691-418] [^c1691-453] -->
## Publish Routing and E2E Assertions

The social-publish-relay-target uses target: Auto (NMP's pool-aware routing), not hardcoded relay.primal.net; the doc comment in host_op_publish.rs:169 was stale but the implementation was correct. last_published_at is stamped unconditionally before the PublishRaw dispatch and is NOT a valid proxy for signing success; a test asserting only the stamp would pass even if signing were deleted. The per-podcast NIP-F4 publish e2e headless scenario asserts on the actual signed event's pubkey and signature via the sign-and-return observable, not on the unconditional last_published_at stamp, with a mutation-check proof (commenting out the register call makes the test FAIL). Per-podcast NIP-09 deletion emits a single kind:5 event with both ['k','10154'] and ['k','54'] tags, tombstoning the full owned-podcast footprint.

<!-- citations: [^c1691-317] [^c1691-332] [^c1691-346] [^c1691-363] [^c1691-377] [^c1691-409] [^c1691-419] [^c1691-437] [^c1691-454] -->
## Stale Items to Drop

Relay-config persistence is already done (commit 0dcf9680) and should be dropped from the candidate list; the BACKLOG entry and register.rs comment are stale. The publish_via_nmp pre-signed-event variant is dead code after #436 (zero callers remain) and should be removed. NostrPendingApprovals and NostrApprovalPresenter are deleted from iOS (they were orphaned: nothing populates the pending queue and the allow/block sets gate nothing in the kernel).

<!-- citations: [^c1691-318] [^c1691-364] -->

## Headless Harness HTTP Requirement

The headless harness required a real async HTTP capability host (not a stub) for RSS subscribes to produce episodes; the stub caused subscribe scenarios to produce empty placeholders and fail. <!-- [^c1691-390] -->

## Android EditProfile Dispatch

Android EditProfile dispatches {op:'publish_profile', name:required, display_name/about/picture:optional} via the existing generic KernelBridge.dispatchAction seam with no Rust or JNI changes required; blank optional fields must be omitted entirely rather than sent as empty strings. about and name are cached locally because AccountSummary does not project them.

<!-- citations: [^c1691-410] [^c1691-420] [^c1691-438] [^c1691-455] -->
