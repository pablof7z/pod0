---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: active
subjects:
  - d9-doctrine
  - connected-at
  - credential-timestamps
  - kernel-authority
supersedes:
  - 2026-06-14-2-kernel-owns-connected-at-timestamps-d9
related_claims: []
source_lines:
  - 11094-11138
  - 11141-11178
captured_at: 2026-06-14T04:44:55Z
---

# Episode: Kernel stamps connected_at (D9 doctrine), shells drop wall-clock

## Prior State

iOS and Android shells sent their own wall-clock `connected_at` timestamps — iOS via `Date()` in `Settings+Helpers.swift`, Android via `System.currentTimeMillis()/1000` in `ProviderCredentialActions.kt`. This violated D9 (kernel is the authoritative source of truth for domain state).

## Trigger

D9 doctrine requires the kernel to own authoritative timestamps; shell wall-clock values are a trust-boundary violation (clock skew, timezone differences, replay).

## Decision

Kernel now stamps `connected_at` via `kernel_now_secs()` (`chrono::Utc::now().timestamp()`) across all 5 `Set*Credential` handler arms. Inbound shell field renamed to `_shell_connected_at` (explicit ignore). iOS removed `connectedAt: Date()` from all 10 `mark*` method signatures. Android removed `connectedAt` from all 5 credential payload data classes and deleted the `epochSeconds()` helper. Display path unchanged — `SettingsSnapshot` still projects `*_connected_at` from the kernel-stamped store value.

## Consequences

- Connected-at timestamps are now kernel-authoritative — no clock-skew between shells
- All three platforms build green with no regressions (cargo 1289 tests, iOS build-for-testing, Android compileDebugKotlin)
- Golden fixture `settings_fresh_install.json` byte-identical — display contract preserved

## Open Tail

*(none)*

## Evidence

- transcript lines 11094-11138
- transcript lines 11141-11178

