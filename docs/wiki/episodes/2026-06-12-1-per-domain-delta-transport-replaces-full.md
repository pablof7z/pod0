---
type: episode-card
date: 2026-06-12
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: reversal
status: superseded
subjects:
  - podcast-snapshot-transport
  - kernel-projections
  - push-path
  - android-clobber-bug
supersedes:
  - 2026-06-12-4-android-empty-clobber-bug-push-frames
  - 2026-06-12-5-cold-start-re-seed-insurance-hashydrated
related_claims: []
source_lines:
  - 1-57
  - 122-160
  - 4700-4920
  - 4923-5007
captured_at: 2026-06-12T15:48:16Z
---

# Episode: Per-domain delta transport replaces full-library serialization

## Prior State

Every kernel command dispatch re-serialized the entire podcast library to JSON on the actor thread (PodcastUpdate → Vec<PodcastSummary> → Vec<EpisodeSummary> → format_escaped_str), even when only one domain changed. A rev-gated snapshot-string cache existed but was defeated by rev bumps on essentially every actor tick. The push path was dead on both shells since v0.3.0 — iOS was pull-only, Android was clobbering its UI to empty on every emit. Physical footprint: 14.6 GB from accumulated allocations. ~57% of CPU samples in build_snapshot_payload → serde_json::to_string.

## Trigger

CPU profiling of process 21680 showed 1,633/2,856 samples (~57%) in full-library serialization. Investigation revealed the rev cache was defeated by per-tick rev bumps. Further diagnosis uncovered that the push path had never been functional on either shell — a pre-existing reactivity bug masquerading as a performance issue, with the Android shell actively destroying its own state every frame.

## Decision

Adopted per-domain typed sidecars: each domain (library, playback, downloads, settings, identity, widget, misc) emits an independent projection frame with its own scoped rev, replacing the monolithic PodcastUpdate envelope on every tick. Tombstone signals (domain key = null) communicate unsubscription. Both shells consume per-domain frames via monotonic rev drop-guards and merge only accepted domains into composite state. Cold-start uses pull-then-push ordering with a hasHydrated flag so the initial full pull can re-seed even if a partial push already consumed the rev. A playback tick now ships ~1KB instead of ~3.9MB.

## Consequences

- 10x per-tick payload reduction (playback tick: ~1KB vs ~3.9MB full-library pull)
- Android empty-clobber bug fixed — every kernel emit had been blanking the UI; the old decodeEnvelope path that decoded the slim v as a full snapshot is deleted entirely
- Both shells now have functional push paths for the first time since v0.3.0
- hasHydrated cold-start guard prevents blank-library race where a partial first push beats the startup pull
- Tombstone contract: domain key = null at a higher rev signals intentional unsubscription (e.g., all-unsubscribed), not an absent domain
- Per-domain rev drop-guards use the global update.rev for sidecar comparison, enabling monotonic rejection of stale frames
- The full-library serialization hot path is structurally eliminated — only changed domains are serialized per tick

## Open Tail

- Blossom active-account upload (kernel async-sign pattern, cycle-6 candidate)
- Kernel AI-chapters + ad-spans consolidation (delete Swift AIChapterCompiler)

## Evidence

- transcript lines 1-57
- transcript lines 122-160
- transcript lines 4700-4920
- transcript lines 4923-5007

