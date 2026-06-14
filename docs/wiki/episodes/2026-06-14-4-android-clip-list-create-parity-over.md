---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: active
subjects:
  - android-clip-ui
  - clip-handler
  - podcast-misc-projection
supersedes: []
related_claims: []
source_lines:
  - 11949-12004
captured_at: 2026-06-14T05:59:05Z
---

# Episode: Android clip list+create parity over existing kernel seam

## Prior State

Android had no clip surface at all; iOS had a full clip vertical (ClippingsView, ClipComposerSheet, ClipVideoComposer, ClipShareSheet). The kernel seam already existed (podcast.clip action module + podcast.misc clips projection)

## Trigger

Cycle-17 planner identified the gap; kernel seam was already solid and required zero Rust changes

## Decision

Build Android Compose clip list + create sheet over the existing kernel seam, scoping to list+create and explicitly deferring video composition/share export as device-only-verifiable long-tail

## Consequences

- Android now has clip list (newest-first, swipe-to-delete, empty state) and create sheet (start/end sliders + optional title) matching iOS ClippingsView/ClipComposerSheet
- 21 contract/wire tests guard the ClipAction payload and ClipSummary decode seam
- ClipSummary rides the podcast.misc domain frame with null=unchanged, empty-list=deleted merge semantics
- Video/share export explicitly deferred — noted as device-only long-tail

## Open Tail

- Video composition and share sheet (ClipVideoComposer/ClipShareSheet equivalents) remain iOS-only until on-device verification is available

## Evidence

- transcript lines 11949-12004

