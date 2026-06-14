---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - connected-at
  - d9-doctrine
  - settings-credentials
supersedes:
  - 2026-06-14-3-kernel-owns-provider-credential-connected-at
related_claims: []
source_lines:
  - 11109-11200
captured_at: 2026-06-14T04:29:51Z
---

# Episode: Kernel owns connected_at timestamps (D9 enforcement)

## Prior State

Both iOS and Android stamped `connected_at` using their local wall-clock time (iOS: `Date()` default parameter converted to `timeIntervalSince1970`; Android: `System.currentTimeMillis() / 1000`). The kernel accepted these shell-provided timestamps as authoritative — a D9 violation (kernel must own its authoritative data).

## Trigger

D9 violation confirmed across both shells: iOS called `Date()` at ten `mark*` methods, Android called `epochSeconds()` at five credential-save sites. Neither shell should own the authoritative connection timestamp.

## Decision

Kernel now stamps `connected_at` via `kernel_now_secs()` (`chrono::Utc::now().timestamp()`). All five kernel handler arms rename the inbound field to `_shell_connected_at` (explicit ignore) and compute `connected_at` from the kernel clock on connect (`Some(kernel_now_secs())`) or `None` on disconnect. Both shells removed their wall-clock generation entirely (iOS removed `Date()` defaults and `connectedAt` from `dispatchCredentialMetadata`; Android removed `@SerialName("connected_at")` fields and the `epochSeconds()` helper).

## Consequences

- Timestamp authority is now kernel-exclusive; shells cannot inject wall-clock skew.
- Display path unchanged — `SettingsSnapshot` still projects the kernel-stamped value to both shells.
- Golden fixture `settings_fresh_install.json` byte-identical (timestamp was never persisted in that snapshot).
- 7-file atomic change across Rust + iOS + Android, CI-validated on all three platforms.

## Open Tail

*(none)*

## Evidence

- transcript lines 11109-11200

