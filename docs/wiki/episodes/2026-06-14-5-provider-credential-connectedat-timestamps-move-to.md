---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: superseded
subjects:
  - connected-at-kernel-time
  - d9-time-authority
  - settings-credential-timestamp
supersedes:
  - 2026-06-14-6-provider-credential-connectedat-must-be-kernel
related_claims: []
source_lines:
  - 10954-10956
  - 11102-11138
  - 11159-11178
captured_at: 2026-06-14T02:13:47Z
---

# Episode: Provider-credential connectedAt timestamps move to kernel clock (D9 doctrine)

## Prior State

Both shells stamped wall-clock time for connected_at — Android used System.currentTimeMillis()/1000 at 5 sites; iOS passed Date() as default parameter on 10 mark* methods — violating D9 (kernel owns time)

## Trigger

D9 violation identified: both shells provide timestamps that the kernel should authoritative-stamp on receipt

## Decision

Kernel stamps connected_at via chrono::Utc::now().timestamp() (kernel_now_secs()); both shells stop sending the field — iOS removes Date() default from all 10 mark* signatures, Android removes connectedAt from all 5 payload classes and deletes the epochSeconds() helper; kernel arms rename inbound field to _shell_connected_at (explicit ignore)

## Consequences

- Kernel is now the sole authority for credential connection timestamps (D9 satisfied)
- Display path unchanged — SettingsSnapshot still projects kernel-stamped values to both shells
- Atomic 3-platform change: must land all three together or the field drifts back
- Android Kotlin CI gate (#441) independently validates the Android change on merge

## Open Tail

*(none)*

## Evidence

- transcript lines 10954-10956
- transcript lines 11102-11138
- transcript lines 11159-11178

