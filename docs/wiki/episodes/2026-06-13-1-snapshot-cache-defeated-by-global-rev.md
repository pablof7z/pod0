---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - snapshot-perf
  - podcast-rev
  - build-snapshot-payload
supersedes:
  - 2026-06-13-1-per-domain-projection-gates-kill-snapshot
related_claims: []
source_lines:
  - 30-57
  - 122-161
captured_at: 2026-06-13T21:21:24Z
---

# Episode: Snapshot cache defeated by global rev — per-domain projections replace whole-library rebuild

## Prior State

build_snapshot_payload had a rev-gated snapshot-string cache intended to skip re-serialization when nothing changed. The assumption was that the cache would hit on most ticks, making the push-frame snapshot path cheap.

## Trigger

Profiling process 21680 showed 57% of samples (~1,633/2,856) inside build_snapshot_payload → serde_json::to_string, plus 14.6 GB physical footprint. Investigation revealed the cache IS structurally correct, but rev.fetch_add(1, Ordering::Relaxed) is called from many handlers (comments_handler, feed_fetch, knowledge, agent_note_handler, social_publish_handler, categorization, etc.), bumping the global rev on essentially every actor tick and defeating the cache entirely.

## Decision

Replace the single global rev + whole-library-serialization model with slice-local payload builders using per-domain/per-podcast revs, so unchanged domains skip serialization entirely. (Implemented as 'slice-local payload builders' — kills the 1 Hz whole-library rebuild.)

## Consequences

- Each projection domain (downloads, queue, categories, social, etc.) carries its own rev and serializes independently; an unchanged domain's cache hits
- The global rev's role is eliminated or subsumed by per-domain revs, removing the O(all-podcasts × all-episodes) serialization per tick
- 14.6 GB memory footprint from accumulated full-library allocations should collapse
- New per-domain projections must be wired into the same push-frame seam (NmpApp::register_snapshot_projection) already used by the prior monolithic path

## Open Tail

- Whether the push-projection path fully replaces the old pull-symbol path or leaves a deprecated compat shim
- Whether per-domain revs introduce ordering risks if projections are delivered at different ticks

## Evidence

- transcript lines 30-57
- transcript lines 122-161

