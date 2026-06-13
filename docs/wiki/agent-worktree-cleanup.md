---
title: Agent Worktree Cleanup
slug: agent-worktree-cleanup
topic: agent-system
summary: The kernel-signing migration branch must be committed and merged to main as soon as possible
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-25
updated: 2026-06-13
verified: 2026-05-25
compiled-from: conversation
sources:
  - session:17b3001c-dc04-462c-86bf-33867057c4dc
  - session:c43d5e77-d667-4e71-a574-47aaab5b6a7a
  - session:7811686b-0a34-439c-9dd6-187a294c905b
  - session:ede5e5c5-01cb-4985-aae5-6a4e1b09fc08
  - session:04b5f843-fdbe-4aa1-ae41-6770eac82957
  - session:736b6230-f894-4073-bbcc-d920819c1940
  - session:rollout-2026-05-09T17-51-25-019e0d38-c712-70c3-9607-bb9c5c518360
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
  - session:rollout-2026-05-25T12-53-35-019e5e8d-dcce-7582-85bd-8c4b7d017c17
  - session:rollout-2026-05-25T12-53-43-019e5e8d-f919-7521-a540-9ca4b95f10ff
  - session:rollout-2026-05-25T22-18-09-019e6092-b9c8-7f53-b3e0-898bfeec48c4
  - session:rollout-2026-05-26T09-10-44-019e62e8-3112-7080-98ca-48ac46d8b8d2
  - session:rollout-2026-05-26T10-15-57-019e6323-e656-7e70-b925-0b0c837b24a1
  - session:rollout-2026-05-26T10-16-01-019e6323-f6b1-7f03-85ec-1a51289f331a
  - session:rollout-2026-05-26T10-16-06-019e6324-08b0-7ca3-9b29-a0e8cf84941d
  - session:rollout-2026-05-26T10-25-30-019e632c-a2ae-7783-b58a-24f557011da1
  - session:16ac1219-405e-4d37-bcba-f2ad417a7e1e
---

# Agent Worktree Cleanup

## Cleanup Policy

The kernel-signing migration branch must be committed and merged to main as soon as possible. When cleaning up stale agent worktrees, only definitively safe ones (those with only .package.resolved untracked or zero changes) are removed; worktrees with real code changes, meaningful commits, or uninspected diffs are preserved to avoid losing work. Git worktree removal uses `git worktree remove` rather than plain `rm -rf` to ensure no uncommitted work is destroyed. Uncommitted changes in dirty worktrees must be preserved as WIP commits on their respective branches before the worktrees are removed. Worktrees with uncommitted changes belonging to genuinely-active agents must be left intact to avoid disrupting live sessions. Worktrees locked by a running agent are left alone. Stale Rust target/ directories in worktrees belonging to completed (unlocked) agents are safe to remove; active (locked) agent worktrees are not touched during cleanup. All stale lock files from merged, finished, or abandoned agent worktrees must be removed. Stale agent worktrees with dead PIDs and stale locks are force-removed using double-force (`-f -f`). Branches with an empty diff versus origin/main are definitively landed and safe to delete. The reliable test for whether a stale branch's work is already in main is whether its cherry-pick onto main comes out empty, not merge-base diffs or git cherry patch-ids. Squash-merged branches that git does not recognize as merged due to differing SHAs are safe to force-delete once their PR merge is confirmed. Feature branches whose content is already squash-merged into main are treated as cruft and deleted rather than force-landed. Branches that are superseded by content already in main are deleted rather than force-landed. Obsolete branches with unpushed local commits are archived to an origin ref (e.g. origin/archive/<branch-name>) before the local branch is deleted, to prevent work loss. The `ndkswift` local branch is preserved because it contains real unique work with broad divergence and overlap with the current Rust/NMP direction; it should be harvested into fresh sliced branches if wanted, not merged wholesale. The `worktree-agent-a01617defe8303e30` branch is large and old (641 commits behind origin/main, 18 ahead, 11 patch-unique commits, plus untracked .package.resolved) and needs review/rebase/splitting before landing. The `worktree-agent-a2c9bc9ab8af41429` worktree is safe to clean (PID dead, clean, fully merged/equivalent to origin/main). The `worktree-agent-a64989b1f057146c0` worktree was only read, not modified, and has no unique code changes to land. The `worktree-agent-a7cee8f86e19c3968` and `worktree-agent-a88ffb74431995e32` worktrees overlap on the same NDKSwift migration base commit and diverge; they must be reconciled into one coherent branch before landing. The `worktree-agent-a6bd4c32c26faca2e`, `worktree-agent-a71d085ae0dfd7142`, `worktree-agent-ac3030a8ae5ebdb46`, and `worktree-agent-acf88cd652f0c9654` worktrees have no unique committed branch work but have uncommitted changes that must not be cleaned until someone preserves or explicitly discards them. The pr-test-extraction worktree has real in-flight work: it is ahead of origin/main by two commits and has six modified files, and must be finished/landed before cleanup. The pr-3-library-ux worktree has eight unique commits that are not git-equivalent to main and must not be deleted without manual audit. The codex-delete-historical-plans worktree has a deletion commit that is not patch-equivalent to origin/main and must not be deleted without confirming it is redundant. The podcast-player-m13a worktree (branch m13a/android-shell) is already landed via squash/patch-equivalent merge into origin/main and is a safe cleanup candidate. The podcast-player-m13cd worktree (branch m13cd/android-ui) is already landed via squash/patch-equivalent merge into origin/main and is a safe cleanup candidate. The podcast-player-ndkswift-plan worktree has local unstaged modifications to Plans/nmp-migration/02-crates.md, Plans/nmp-migration/milestones/M10-peer-nostr-blossom.md, and WIP.md that need salvage review before cleanup. The podcast-player-pod0-impl worktree (branch codex/pod0-nipf4) has one unique patch not yet in origin/main and needs diff review/rebase/explicit abandonment before cleanup to avoid losing unique work. A dirty, non-PR worktree at `/Users/pablofernandez/Work/podcast-player-queue-persist` on `feat/queue-persistence` with an uncommitted change in `apps/nmp-app-podcast/src/store/persistence.rs` must not be deleted without preserving its work. Content-level supersession analysis for ambiguous NO-PR branches must be delegated to a background agent rather than risk auto-deleting novel work. Orphaned wiki docs from old agent worktrees are discarded rather than committed to the main repo. External repos (ios-shake-feedback, nostrmultiplatform) stored inside worktrees are left alone, not removed. Auto-generated derived cache trees (docs/wiki/ and android/.../docs/) are excluded from commits and should be gitignored rather than committed. Transient `.claude/scheduled_tasks.lock` is excluded from commits, and `.claude/worktrees/` remains ignored by `.gitignore`. NIP-46 bunker signing goes through the kernel (not a Swift-side path): `nmp_signer_broker_init` is called at init, `publish_unsigned_event` calls `sign_active_nonblocking` which checks `identity.active_remote()` first and parks via `PendingSign` for NIP-46 bunker accounts. The `social-bunker-signing-kernel` BACKLOG item is DONE. (Previously: Blossom audio-path migration stays blocked upstream because NMP's signer_pubkey selects from registered signers and there is no API to register per-podcast NIP-F4 keys into the kernel roster.)

<!-- citations: [^04b5f-2] [^17b30-1] [^17b30-2] [^78116-1] [^04b5f-1] [^736b6-1] [^rollo-23] [^rollo-219] [^rollo-232] [^rollo-240] [^rollo-247] [^rollo-257] [^c1691-218] [^16ac1-1] [^c1691-274] -->
## Plan File Management

Completed/implemented plan files are deleted from docs/plan/ and docs/plan.md tracks only active work, not done items. <!-- [^c43d5-2] -->

## Checkout Management

The primary checkout must be repointed to origin/main immediately, prioritizing this over resolving merge conflicts in the current branch. <!-- [^78116-2] -->

## Workflow

Implementation work is done in git worktrees, and work must be done on a branch (not on stale main), per the project's AGENTS.md. Implementation work must follow the NMP protocol: use an agent-owned git worktree, read and update the root `WIP.md` before touching code, keep the root checkout untouched, open a PR when finished, and remove the WIP entry. PRs must include validation in the PR body. Agent outputs must point to `docs/plan.md`, `docs/BACKLOG.md`, and `docs/plan/` rather than `.claude/...` paths. When operating in a multi-agent shared-root environment with concurrent git mutations, an isolated worktree with fast-forward-only pushes is used for safe orchestration and landing commits. All implementers work in isolated worktrees branched from `origin/main`, never the shared root checkout, to prevent cross-agent clobbering. Reviewer agents must use git diff/show only and never perform working-tree git operations in the shared root. Reviewers must never run git checkout or git restore in the shared working root (only git diff/show), because doing so overwrites uncommitted WIP from concurrent agents with no recovery path. All implementation work must happen in a git worktree owned by the agent doing the work, not from the shared root checkout. Worktree isolation is mandatory for all implementer agents; merging and pruning happen on origin/main, and disk is monitored before launching heavy Rust builds. Every agent must read `WIP.md` from the project base directory before starting work to understand what other agents are currently doing. When an agent starts work, it must add an entry to `WIP.md` in the project base directory with a timestamp, a one-line description, the branch name, and the git worktree path. After a PR merges, the agent must remove its own entry from `WIP.md` at the project base directory and clean up its owned worktree. WIP.md must be cleaned to remove merged PR entries and leave only genuinely active worktrees. WIP.md must be updated when an agent starts work and when its PR merges.

<!-- citations: [^ede5e-1] [^04b5f-3] [^c1691-50] [^rollo-174] [^rollo-188] [^rollo-204] [^rollo-220] [^rollo-233] [^c1691-128] [^c1691-147] [^c1691-176] [^c1691-275] -->
