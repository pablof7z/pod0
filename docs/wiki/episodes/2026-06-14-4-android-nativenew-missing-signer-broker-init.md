---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: active
subjects:
  - nmp-signer-broker-init
  - android-init
  - nip-46
supersedes: []
related_claims: []
source_lines:
  - 11549-11556
  - 11648-11649
captured_at: 2026-06-14T04:44:55Z
---

# Episode: Android nativeNew missing signer-broker init — NIP-46 was entirely non-functional

## Prior State

Android's `nativeNew` in `android.rs` never called `nmp_signer_broker_init`. iOS calls it in `PodcastHandle.init`. Without it, the global broker (`GLOBAL_BROKER`) is never initialized, so NIP-46 bunker/nostrconnect handshakes could never work on Android.

## Trigger

Implementation of Android NIP-46 (PR #455) revealed the missing init call. The broker module uses `OnceLock` (`signer_broker.rs:41`), meaning even a double-call is a no-op — so adding it is safe.

## Decision

Added `nmp_signer_broker_init(app)` to Android's `nativeNew`, after `nmp_external_signer_init` and before `nativeStart`. The broker is independent of the NIP-55/Amber path (separate module, separate global state) — purely additive.

## Consequences

- NIP-46 remote signing can now function on Android (broker is initialized)
- NIP-55/Amber path unaffected (independent module)
- Init order is safe — broker only needs `app.actor_sender()`, available immediately post-`nmp_app_new`
- Pre-existing platform divergence noted: iOS does not call `nmp_external_signer_init` in its init; Android does

## Open Tail

*(none)*

## Evidence

- transcript lines 11549-11556
- transcript lines 11648-11649

