---
title: Worktree WIP Protocol
slug: worktree-wip-protocol
summary: All implementation work must happen in a git worktree with a WIP.md entry registered in the main repo, per AGENTS.md.
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-01
updated: 2026-06-04
verified: 2026-06-01
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:8bfa1b91-b40c-44b3-acb9-245b36f4c841
  - session:7811686b-0a34-439c-9dd6-187a294c905b
---

# Worktree WIP Protocol

## Worktree WIP Protocol

All implementation work must happen in a git worktree with a WIP.md entry registered in the main repo, per AGENTS.md. The main checkout at /Users/pablofernandez/Work/podcast-player must point to the main branch, and the current branch must be committed and merged to main. Worktree cleanup must use a lossless preserve-then-remove pattern where uncommitted changes in a worktree are committed to their branch as a WIP-preserve commit before the worktree is removed. Locked worktrees named with the worktree-agent-<id> pattern must not be removed, as they map to potentially active agent sessions. Worktrees with file writes older than 55 hours are considered abandoned scratch and can be removed using the lossless preserve-then-remove pattern. All identified documentation fixes and the 18 new untracked wiki files must be staged and committed together before merging open PRs.

<!-- citations: [^14943-130] [^8bfa1-8] [^78116-3] -->
