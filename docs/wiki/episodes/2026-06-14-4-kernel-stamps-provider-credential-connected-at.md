---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - d9-kernel-time
  - provider-credential
  - connected-at
  - cross-shell-contract
supersedes:
  - 2026-06-14-5-provider-credential-connectedat-timestamps-move-to
related_claims: []
source_lines:
  - 10954-10955
  - 11109-11137
  - 11149-11178
captured_at: 2026-06-14T03:48:31Z
---

# Episode: Kernel stamps provider-credential connected_at (D9); shell wall-clock dropped

## Prior State

Both iOS and Android stamped connectedAt using native wall-clock time (iOS: Date() as default parameter in 10 mark* methods; Android: System.currentTimeMillis()/1000 at 5 sites) and sent it to the kernel — violating D9 (kernel owns time)

## Trigger

Session planner identified the D9 violation: both shells stamp wall-clock time the kernel should own

## Decision

Kernel stamps connected_at via kernel_now_secs() (chrono::Utc::now().timestamp()) on receipt — connected on source!="none", None on disconnect; inbound shell field renamed to _shell_connected_at (ignored) in all 5 Rust handler arms; field removed from iOS Settings+Helpers/AppStateStore+Settings and Android ProviderCredentialActions/ActionDispatcher payload classes; display path unchanged (SettingsSnapshot projects the kernel-stamped value)

## Consequences

- Native shell wall-clock can no longer re-enter the kernel — D9 satisfied atomically across Rust + iOS + Android (7 files)
- connected_at display still works — it reads the kernel-stamped value from SettingsSnapshot
- Android Kotlin CI gate (#441) independently validates the Android change
- Future shell implementations cannot drift the timestamp — the kernel is the sole source of truth

## Open Tail

*(none)*

## Evidence

- transcript lines 10954-10955
- transcript lines 11109-11137
- transcript lines 11149-11178

