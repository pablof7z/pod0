---
title: Divergent Branch Resolution
slug: divergent-branch-resolution
summary: Resolving a material fork between local M1 worktree and origin PR #133 by creating a validated superset branch and closing origin PRs as superseded.
tags:
  - migration
  - workflow
  - git
volatility: cold
confidence: medium
created: 2026-05-30
updated: 2026-06-04
verified: 2026-05-30
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:7811686b-0a34-439c-9dd6-187a294c905b
---

# Divergent Branch Resolution

> Resolving a material fork between local M1 worktree and origin PR #133 by creating a validated superset branch and closing origin PRs as superseded.

## Overview

The M1 Part 3 integration encountered a material fork: the local worktree branch (9e85566f) and the origin PR #133 branch (5106179c) both diverged from the same ancestor (PR #131, commit ba386e85) into two separate M1/Part3 implementations. The local branch had 3 unpushed commits whose commit messages matched the very codex findings being re-fixed. Origin's #133 had 16 commits but was also 20 commits behind main and CI-failing. When a local main branch diverges from origin/main with local feature commits, the main checkout must be repointed to the real origin/main, provided the local commits are already preserved on another pushed branch. When merging a branch with main, the merge must honor deletions made by main (e.g., briefings files deleted in #238). Project.pbxproj merge conflicts must be resolved by regenerating the file via tuist generate.

<!-- citations: [^14943-59] [^78116-1] -->
## Resolution

The resolution created a new branch feat/m1-part3-integrated with the validated, main-merged, codex-hardened superset. Origin's #132 and #133 were closed as superseded (non-destructive — their commits stay on the refs). A new PR #138 was opened from the integrated branch and merged to main. The key discriminator: the local branch's 3 base commits were stamped 2026-05-28 12:41, newer than origin's entire #133 line (00:07-10:56), confirming it was a later parallel re-implementation. Local branches with an empty diff versus origin/main are superseded and must be deleted. Local branches associated with merged GitHub PRs must be deleted despite showing non-empty three-dot diffs caused by squash-merge ancestry artifacts. Ambiguous unmerged branches without PRs must be preserved and assessed via content-level supersession analysis rather than auto-deleted, to prevent losing novel work.

<!-- citations: [^14943-60] [^78116-2] -->
## Convergence Check

Before superseding, a comparison confirmed the two lines were largely convergent: the player-file diff was modest (~93 lines, origin being leaner), and the local tree already had origin's named fixes (SyncCapabilityBridge download routing, applyNowPlayingSnapshot with dedup keys). The integrated branch was the newer, codex-hardened, validated superset. <!-- [^14943-61] -->


Overview

Lesson learned: when working on a worktree created from a local branch (not origin), always verify pushability before making substantial changes. A worktree created from a stale local branch may carry unpushed commits whose commit messages overlap with the fixes being applied — this looks like duplicate work but is actually a later parallel re-implementation. The discriminator is commit dates: if the local commits are newer than origin's, the local line is the active one and the origin line is stale. Before force-pushing over an origin PR branch (which would discard someone else's work), verify convergence by comparing file diffs and checking whether the local tree already has origin's named fixes. The safe resolution is to create a new branch (non-destructive to origin's refs), open a new PR, and close the origin PRs as superseded. <!-- [^14943-87] -->
