---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: active
subjects:
  - android-nip46
  - identity-projection
  - snapshot-identity-mode
supersedes:
  - 2026-06-14-1-nip-46-completion-signal-gated-on
related_claims: []
source_lines:
  - 11541-11730
captured_at: 2026-06-14T07:49:05Z
---

# Episode: NIP-46 remote signer completion signal gates on external-account transition, not mode token

## Prior State

Android RemoteSignerScreen and NostrConnectScreen gated 'connected' on `activeAccount.mode == "bunker" || "nip46"` — string-matching the diagnostic mode token from the Rust identity projection.

## Trigger

Opus review (PR #455) found that `snapshot_identity.rs` only ever emits `"local_key"` or `"nip55"`; a kernel-owned bunker account flattens to `"nip55"`, making the completion gate unreachable — the UI would spin forever even on a successful handshake. iOS explicitly warns 'never string-match on signer_kind' and uses a dedicated `signer_is_remote` boolean instead.

## Decision

Gate completion on the external-account transition (`mode != "local_key"`, true for the `"nip55"` value the projection actually emits for bunker accounts) instead of string-matching mode tokens. No Rust projection change needed. Also extracted URI validators from test-only helpers into shared production `Nip46Uri`, and relabeled `ModeBadge` to honest 'Remote Signer' since the projection cannot distinguish Amber from bunker.

## Consequences

- Completion signal is now reachable — a successful NIP-46 handshake resolves the spinner
- The Android pattern adapts iOS doctrine (gate on a boolean/transition, not a diagnostic mode string) without requiring a `signer_is_remote` field that doesn't exist in Android's snapshot
- Pre-existing `ModeBadge` dead branch (cosmetic mislabel) acknowledged as separately scoped; needs a projection field to fix properly
- Live end-to-end NIP-46 handshake still requires on-device verification with a real signer (Amber)

## Open Tail

- ModeBadge Amber-vs-bunker distinction needs a projection field (separately scoped)
- On-device NIP-46 handshake verification with real signer app

## Evidence

- transcript lines 11541-11730

