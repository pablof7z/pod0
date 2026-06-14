---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - nip46-remote-signer
  - identity-projection
  - signer-broker-init
  - android-identity-parity
supersedes: []
related_claims: []
source_lines:
  - 11479-11644
captured_at: 2026-06-14T04:29:51Z
---

# Episode: NIP-46 handshake completion is unreachable (mode projection flattening + missing broker init)

## Prior State

Android had no NIP-46 remote signer support. The new `RemoteSignerScreen` and `NostrConnectScreen` gated completion on `activeAccount.mode == "bunker"/"nip46"`. Additionally, `nmp_signer_broker_init` was never called in Android's `nativeNew` (a latent bug — iOS calls it in `PodcastHandle.init`), meaning NIP-46 handshakes could not work at all on Android. The pre-existing `ModeBadge` also string-matched on `"bunker"/"nip46"` — a cosmetic dead branch.

## Trigger

Opus review (PR #455) proved statically that `apps/nmp-app-podcast/src/ffi/snapshot_identity.rs` only ever emits `"local_key"` or `"nip55"`. A bunker/NIP-46 account is kernel-owned (no local secret), so it routes through `external_account_summary`, which flattens every external signer to `MODE_NIP55`. The completion-check tokens (`"bunker"`, `"nip46"`) are never emitted. iOS explicitly avoids this pattern: `KernelIdentityProjection.swift` reads `signer_is_remote` (boolean) and warns 'never string-match on `signer_kind`' (documented as 'Diagnostic only').

## Decision

Fix the completion gate to use the remote-signer indicator / external-account transition instead of the diagnostic mode token, matching iOS doctrine. Call `nmp_signer_broker_init` in `nativeNew` (idempotent via `OnceLock`, fixes the latent NIP-46-can't-work-at-all bug). Extract URI validators from test file into production code so tests guard the shipped code path.

## Consequences

- Without the fix, a successful NIP-46 handshake would render as `mode == "nip55"`, `isPaired` would stay `false`, and both screens would spin forever — sign-in actually works but the UI can never detect completion.
- The `nmp_signer_broker_init` omission was a latent bug affecting ALL Android signing; its addition is idempotent and does not conflict with the working NIP-55/Amber path (separate modules, separate globals).
- JNI wrappers for the three FFI symbols (`signin_bunker`, `cancel_bunker_handshake`, `nostrconnect_uri`) are correctly mangled/marshalled/leak-free per review — plumbing is sound; only the reactive completion signal was broken.
- Live NIP-46 handshake still requires on-device verification with a real signer (Amber/nsec.app).
- Pre-existing `ModeBadge` dead branch (`"bunker"/"nip46"`) needs the same correction.

## Open Tail

- On-device handshake verification with real signer
- ModeBadge should use the remote-signer indicator rather than string-matching diagnostic tokens

## Evidence

- transcript lines 11479-11644

