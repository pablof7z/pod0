---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: active
subjects:
  - player-download-bump
  - domain-rev-bump
  - download-projection
supersedes: []
related_claims: []
source_lines:
  - 12487-12534
captured_at: 2026-06-14T08:23:24Z
---

# Episode: Player-download path never emitted snapshot frames — real-bump bug caught by headless scenario

## Prior State

Downloads triggered via podcast.player { op: "download" } appeared to work locally but the download row never reactively surfaced in the UI. The sister path podcast.download correctly called bump_domain(Domain::Downloads) after enqueueing.

## Trigger

The new download_lifecycle headless scenario (built to guard the twice-crashed download projection) failed on clean CI: 'download row never materialized in projection: wait_for timed out after 5000 ms'. It passed locally because prior scenarios left domain state that incidentally flushed snapshots carrying the queued row. Root cause: handle_player_download called start_episode_download but never called bump_domain(Domain::Downloads) — the rev counter stayed flat, no push frame emitted.

## Decision

Add self.bump_domain(crate::state::Domain::Downloads) after successful start_episode_download in handle_player_download, mirroring the identical pattern in podcast_actions_downloads.rs (3-line fix with comments).

## Consequences

- Downloads via the player namespace now emit snapshot frames — the UI reactively shows them immediately
- Same real-bump class as #399/#400/#423 (missing domain bump after mutation)
- The headless scenario permanently CI-gates this path: re-dispatch idempotence (#442), pause/resume/cancel visibility (#463), and now player-path reactive emission
- Validates the durability strategy: building a regression net on the crash-prone projection caught a third, independent live bug that unit tests and manual testing both missed

## Open Tail

*(none)*

## Evidence

- transcript lines 12487-12534

