---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: active
subjects:
  - snapshot-perf
  - domain-revisions
  - podcast-misc-split
  - android-social-slice
supersedes: []
related_claims: []
source_lines:
  - 30-57
  - 122-161
  - 8474-8497
captured_at: 2026-06-13T18:48:50Z
---

# Episode: Snapshot hot-path already fixed; misc split deprioritized in favor of Android parity

## Prior State

The per-domain podcast.misc projection split was the highest-conviction cycle-10 item, believed necessary because the rev-gated snapshot cache was being defeated on every actor tick (57% CPU in serde_json serialization of the full library).

## Trigger

Profiling (sample_21680) showed 57% CPU in build_snapshot_payload → serde_json::to_string. Deeper code investigation revealed (a) the rev-gated cache in build_snapshot_payload works correctly — it only misses when rev advances, and (b) #425 already split domain revisions from the global signal bump: high-frequency mutators (agent-chat, voice, clips, comments) use signal.bump() which touches only the GLOBAL rev, not domain counters, so misc domain closures return None and no rebuild fires. Only low-frequency wiki/knowledge/picks edits advance the misc domain rev.

## Decision

Explicitly deprioritized the podcast.misc split as not worth a contract-heavy PR for near-zero perf gain (the hot path was already killed by #425). Reprioritized cycle-10 headline to the Android social/conversations vertical slice (the clear feature-parity gap: kernel emits podcast.social, Android decodes it but renders nothing).

## Consequences

- Avoided a contract-sensitive PR (snake_case + golden + real-bump gates across both shells) that would have produced negligible perf improvement
- Android conversations list + detail screens shipped (PR #428) consuming the already-decoded podcast.social frame with zero Rust contract change
- Latent correctness question remains: push frames never carry agent-chat/voice deltas since those mutators never advance a domain rev (pull path remains the hydration fallback, not a regression)
- Android build-unverified at merge time due to disk exhaustion; later verified post-merge that app Kotlin compiles clean

## Open Tail

- Does the push frame need to carry agent-chat/voice deltas, or is the pull-path fallback sufficient?
- Android profile resolution (resolved-profiles map) deferred — screens show hex-pubkey fallback for now

## Evidence

- transcript lines 30-57
- transcript lines 122-161
- transcript lines 8474-8497

