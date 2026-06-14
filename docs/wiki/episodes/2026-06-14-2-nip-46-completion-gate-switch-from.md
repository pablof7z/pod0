---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - android-nip46
  - identity-projection
  - snapshot-identity
supersedes:
  - 2026-06-14-2-nip-46-completion-gate-must-use
related_claims: []
source_lines:
  - 11480-11730
captured_at: 2026-06-14T05:59:05Z
---

# Episode: NIP-46 completion gate: switch from mode-string-match to external-account transition signal

## Prior State

Android NIP-46 screens gated 'connected' on activeAccount.mode == 'bunker'/'nip46', which the Rust identity projection never emits — a bunker account flattens to 'nip55'. Also, nmp_signer_broker_init was never called in Android's nativeNew (latent bug: NIP-46 couldn't work at all). Completion signal was unreachable; screen would spin forever on successful sign-in

## Trigger

Opus review of PR #455 found that snapshot_identity.rs only emits 'local_key' or 'nip55', making the string-match gate dead. iOS doctrine explicitly warns against string-matching signer_kind ('Diagnostic only'); reads signer_is_remote boolean instead

## Decision

Gate completion on the external-account transition (mode != 'local_key') rather than string-matching diagnostic mode tokens, matching iOS doctrine adapted for Android's transition-only snapshot. Also added missing nmp_signer_broker_init call, extracted URI validators to production code, relabeled ModeBadge honestly

## Consequences

- Completion signal is now reachable: a bunker account emitting 'nip55' triggers isPaired = true
- nmp_signer_broker_init now called in Android nativeNew (was missing — NIP-46 was completely non-functional without it)
- URI validators shared between screens and tests (no parallel reimplementation)
- ModeBadge relabeled 'Remote Signer' since projection can't distinguish Amber from bunker
- Live end-to-end handshake still needs on-device verification with real signer

## Open Tail

- On-device NIP-46 handshake test needed (requires real Android device + Amber)
- ModeBadge Amber-vs-bunker distinction needs a projection field (separately scoped)

## Evidence

- transcript lines 11480-11730

