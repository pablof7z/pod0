---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - nip46-completion-signal
  - snapshot-identity
  - android-remote-signer
supersedes:
  - 2026-06-14-3-identity-projection-flattens-all-external-signers
related_claims: []
source_lines:
  - 11542-11634
  - 11647-11684
  - 11701-11732
captured_at: 2026-06-14T05:49:17Z
---

# Episode: NIP-46 completion gate must use external-account transition, not diagnostic mode string

## Prior State

RemoteSignerScreen and NostrConnectScreen gated 'connected' on account.mode == "bunker" or "nip46"; snapshot_identity.rs only ever emits "local_key" or "nip55" — a bunker account flattens to "nip55" because it has no local secret, making the completion signal unreachable and causing an infinite spinner on successful sign-in

## Trigger

Opus review of PR #455 proved that the identity projection never emits "bunker"/"nip46"; iOS KernelIdentityProjection.swift explicitly warns 'never string-match on signer_kind' (it is diagnostic-only)

## Decision

Gate completion on Nip46Uri.isRemoteSignerAccount(account) — true when mode != "local_key", i.e. the "nip55" value the projection actually emits for a bunker account. This matches iOS doctrine: detect the state transition (external signer appeared from NotSignedIn) rather than string-matching a diagnostic token

## Consequences

- Completion signal is now reachable: a successful bunker handshake surfaces as mode="nip55", which passes the mode != local_key gate
- ModeBadge relabeled honestly to 'Remote Signer' since the projection cannot distinguish Amber from bunker
- Nip46Uri validation helpers extracted to production code (screens + tests now call the same functions)
- Distinguishing Amber-vs-bunker in the badge requires a new projection field (separately scoped)
- On-device handshake test with a real signer (Amber/nsec.app) is still device-pending

## Open Tail

- Live NIP-46 handshake end-to-end needs real Android device + Amber verification
- ModeBadge Amber-vs-bunker distinction needs a projection field addition (separately scoped)

## Evidence

- transcript lines 11542-11634
- transcript lines 11647-11684
- transcript lines 11701-11732

