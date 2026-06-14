---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - d9-kernel-time
  - connected-at
  - provider-credential
supersedes: []
related_claims: []
source_lines:
  - 11102-11137
captured_at: 2026-06-14T02:04:38Z
---

# Episode: Provider credential connectedAt must be kernel-stamped (D9 doctrine)

## Prior State

Both shells stamped wall-clock connectedAt locally: Android called System.currentTimeMillis()/1000 at five save-credential sites in ProviderCredentialActions.kt; iOS passed connectedAt: Date() as a default parameter across ten mark*/BYOK methods in Settings+Helpers.swift. This violated D9 (kernel owns time).

## Trigger

Cycle-15 planner identified the D9 violation as a real, both-shells ownership split — the kernel should own the timestamp, not the native clock.

## Decision

Kernel now stamps connected_at on receipt of the set-credential action (using chrono::Utc::now().timestamp()). The inbound field is renamed to _shell_connected_at (explicit ignore) in all five handler arms. On disconnect, connected_at is set to None. The connectedAt parameter was removed from all ten iOS methods and all five Android payload sites (plus the epochSeconds() helper deleted). Display path unchanged: SettingsSnapshot still projects kernel-stamped connected_at to both shells.

## Consequences

- D9 compliance: the kernel is now the sole source of truth for credential connection timestamps
- Both shell codebases are simpler (no local clock computation for this field)
- A future shell cannot regress by re-adding a local timestamp
- Atomic landing across Rust + iOS + Android (all three in one PR) prevents partial adoption

## Open Tail

*(none)*

## Evidence

- transcript lines 11102-11137

