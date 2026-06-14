---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - identity-projection
  - account-mode
  - nip-46
  - android-identity
supersedes:
  - 2026-06-14-3-nip-46-handshake-completion-is-unreachable
related_claims: []
source_lines:
  - 11579-11631
  - 11650-11684
captured_at: 2026-06-14T04:44:55Z
---

# Episode: Identity projection flattens all external signers to nip55 — no bunker/nip46 mode token

## Prior State

Android NIP-46 screens (RemoteSignerScreen, NostrConnectScreen) and the pre-existing ModeBadge gated completion on `account.mode == "bunker"` or `"nip46"`, assuming the projection would emit those tokens for NIP-46 remote-signer accounts.

## Trigger

Opus review of PR #455 proved that `snapshot_identity.rs` only ever emits two mode values: `"local_key"` (kernel-active hex matches app-owned local key) or `"nip55"` (any external signer — including bunker). A kernel-owned bunker account flattens to `"nip55"` via `external_account_summary`. This made the completion gate unreachable — a successful NIP-46 handshake would spin forever despite sign-in actually working. iOS avoids this by reading a dedicated `signer_is_remote` boolean (documented as 'Diagnostic only — never string-match on signer_kind').

## Decision

Gate on external-account transition (`mode != "local_key"`) instead of string-matching diagnostic mode tokens. `Nip46Uri.isRemoteSignerAccount()` and `handshakeCompleted()` are shared production helpers used by both screens and tests. ModeBadge relabeled to honest 'Remote Signer' (the projection cannot distinguish Amber from bunker without a field addition).

## Consequences

- RemoteSignerScreen and NostrConnectScreen now correctly detect completion on `mode="nip55"` (the value actually emitted)
- Any future Android identity code must use the shared helpers, not string-match on mode tokens
- A distinct bunker/nip46 badge would require a projection field addition — flagged as separately scoped

## Open Tail

- On-device adb test with a real signer (Amber) needed to confirm end-to-end handshake flow

## Evidence

- transcript lines 11579-11631
- transcript lines 11650-11684

