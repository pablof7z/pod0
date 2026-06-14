---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - connected-at
  - provider-credential
  - d9-kernel-time
supersedes:
  - 2026-06-14-4-kernel-stamps-provider-credential-connected-at
related_claims: []
source_lines:
  - 10954-10957
  - 11102-11138
captured_at: 2026-06-14T04:04:34Z
---

# Episode: Kernel owns provider-credential connected_at (D9) — shell wall-clock dropped

## Prior State

Both shells stamped wall-clock time for provider credential `connectedAt`: Android called `System.currentTimeMillis() / 1000` at five save-credential sites in `ProviderCredentialActions.kt`; iOS called `Date()` as a default parameter on ten `mark*` methods in `Settings+Helpers.swift`, then converted to `Int($0.timeIntervalSince1970)` before dispatching. This violated D9 (kernel owns time).

## Trigger

Cycle-15 planner identified the D9 violation: both shells stamp wall-clock the kernel should own, with the timestamp flowing from native code into the kernel action payload.

## Decision

Kernel stamps `connected_at` via `kernel_now_secs()` (`chrono::Utc::now().timestamp()`) on receipt of `Set*Credential` actions. All five kernel handler arms rename the inbound field to `_shell_connected_at` (explicit ignore). Both shells remove the field: iOS drops `connectedAt: Date()` from all ten `mark*` signatures and `AppStateStore+Settings` dispatch; Android removes `@SerialName("connected_at")` from all five credential payload data classes and deletes the `epochSeconds()` helper.

## Consequences

- Atomic 3-platform change required (Rust + iOS + Android must land together or the field drifts back)
- Display path unchanged — `SettingsSnapshot` still projects `*_connected_at` from the store to both shells, now reading the kernel-stamped value
- Two D9 gate tests added: one verifying payloads without `connected_at` deserialize cleanly, one asserting `kernel_now_secs()` returns a plausible recent Unix timestamp

## Open Tail

*(none)*

## Evidence

- transcript lines 10954-10957
- transcript lines 11102-11138

