---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - nip46-completion-signal
  - snapshot-identity-mode
  - android-remote-signer
supersedes:
  - 2026-06-14-2-nip-46-completion-gate-switch-from
related_claims: []
source_lines:
  - 11541-11633
  - 11648-11683
  - 11703-11730
captured_at: 2026-06-14T06:06:13Z
---

# Episode: NIP-46 completion signal gated on unreachable mode tokens — replaced by external-account transition

## Prior State

RemoteSignerScreen and NostrConnectScreen gated handshake completion on `activeAccount.mode == "bunker" || "nip46"`, assuming the identity projection would emit distinct mode tokens for NIP-46 accounts. The projection (`snapshot_identity.rs`) only ever emits `"local_key"` or `"nip55"` — a kernel-owned bunker account flattens to `"nip55"` via `external_account_summary`. The pre-existing `ModeBadge` had the same dead branch. iOS deliberately avoids this pattern, reading a `signer_is_remote` boolean instead and warning "never string-match on signer_kind."

## Trigger

Opus adversarial review of PR #455 proved statically that `isPaired` could never become true — a successful NIP-46 handshake would render as `mode="nip55"`, leaving the spinner forever. The projection never emits the tokens the screens checked.

## Decision

Gate completion on the external-account transition (`mode != "local_key"`, i.e. the `"nip55"` value the projection actually emits) via a shared `Nip46Uri.isRemoteSignerAccount()` helper, rather than string-matching diagnostic mode tokens. This adapts the iOS `signer_is_remote` doctrine to Android's transition-only snapshot. `ModeBadge` relabeled honestly to "Remote Signer". URI validators extracted to production `Nip46Uri` so tests guard the shipped code path.

## Consequences

- NIP-46 handshake completion signal is now reachable — `isPaired` fires on `mode="nip55"`, the value actually emitted for bunker accounts
- Mode tokens are now treated as diagnostic-only (matching iOS doctrine), never used for control flow
- The `ModeBadge` no longer claims a bunker/nip46 distinction the projection cannot provide (needs a projection field change, separately scoped)
- On-device handshake test still pending (requires real Android device + Amber signer)

## Open Tail

- Distinguish Amber vs bunker in `ModeBadge` — needs a new projection field
- On-device end-to-end NIP-46 handshake verification with real signer

## Evidence

- transcript lines 11541-11633
- transcript lines 11648-11683
- transcript lines 11703-11730

