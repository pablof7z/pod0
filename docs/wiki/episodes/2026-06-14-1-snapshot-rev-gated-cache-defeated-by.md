---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - snapshot-cache
  - build-snapshot-payload
  - rev-counter
supersedes:
  - 2026-06-13-1-snapshot-cache-defeated-by-global-rev
related_claims: []
source_lines:
  - 30-170
captured_at: 2026-06-14T00:26:33Z
---

# Episode: Snapshot rev-gated cache defeated by global rev counter

## Prior State

build_snapshot_payload had a rev-gated cache intended to skip re-serialization when nothing changed — assumed to provide a cheap fast path on unchanged revs

## Trigger

CPU profile showed 57% of samples in build_snapshot_payload → serde_json::to_string (1,633/2,856 samples), with 14.6 GB physical footprint. Diagnosis revealed rev is bumped by every command handler (comments_handler, feed_fetch, knowledge, agent_note_handler, categorization, etc.) on every actor tick, defeating the cache entirely

## Decision

Root-cause identified: the global atomic rev counter is the structural problem — any command dispatch bumps rev, invalidating the cache and forcing full-library JSON serialization. Fix directions documented: (1) delta snapshots, (2) per-podcast serialized-form cache keyed by per-entity rev, or (3) push individual PodcastSummary updates instead of the full PodcastUpdate envelope

## Consequences

- The rev-gated cache is structurally defeated — it never hits on a live actor tick
- 14.6 GB footprint likely from accumulated allocations from repeated full-library serializations
- Per-entity rev gating or structural push is needed, not a global rev check
- String escaping (format_escaped_str) is the leaf bottleneck but is just serde on an oversized payload — the real fix is reducing what gets serialized

## Open Tail

- Which of the three fix directions to adopt (delta snapshots, per-entity cache, or structural push) is unresolved
- Memory pressure from 14.6 GB footprint may need separate mitigation

## Evidence

- transcript lines 30-170

