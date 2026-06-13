---
title: Commit Conventions
slug: commit-conventions
topic: project-setup
summary: The default branch for this repo is main, not master
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-10
updated: 2026-06-13
verified: 2026-05-10
compiled-from: conversation
sources:
  - session:rollout-2026-05-10T20-45-04-019e12fe-1fe8-7d93-a41f-0cebfa991f0a
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
  - session:rollout-2026-05-11T09-10-30-019e15a8-96ed-76a3-9539-607404bb9a31
  - session:rollout-2026-05-25T12-53-43-019e5e8d-f919-7521-a540-9ca4b95f10ff
  - session:rollout-2026-05-25T22-18-09-019e6092-b9c8-7f53-b3e0-898bfeec48c4
  - session:rollout-2026-05-26T09-10-44-019e62e8-3112-7080-98ca-48ac46d8b8d2
  - session:rollout-2026-05-26T10-15-57-019e6323-e656-7e70-b925-0b0c837b24a1
  - session:rollout-2026-05-26T10-25-30-019e632c-a2ae-7783-b58a-24f557011da1
  - session:rollout-2026-05-26T10-25-39-019e632c-c6b9-7d93-ba3d-936a9b410c6b
---

# Commit Conventions

## Commit Structure

The default branch for this repo is main, not master. Completed integration work is committed to main with test/build context in the commit message. Unrelated live UI changes are committed separately from the main cleanup commit. Parallel workers edit disjoint slices in the codebase, so unrelated files must not be reverted or reformatted.

Reviewers must never run `git checkout`, `git restore`, `git stash`, or any working-tree git operation in the shared root — only `git diff`/`git show` reads from the object DB.

When work is complete, the agent must open a pull request whose description includes a short TLDR, a detailed overview of the work performed, validation performed, and any subjective decisions, tradeoffs, or assumptions. Completed work must not be opened as a draft pull request; draft PRs are only for intentionally incomplete work or when explicitly asked.

Local validation must be scoped to the files and behavior touched by the change, and the exact commands must be recorded in the PR.

git diff --check must always be run before opening a PR.

Local main must be reconciled with origin/main before continuing implementation from the root checkout.

The long-term correct architecture must be preferred over a local patch that only makes the immediate build green.

<!-- citations: [^rollo-206] [^rollo-207] [^rollo-43] [^c1691-37] [^rollo-116] [^rollo-222] [^rollo-234] [^rollo-258] [^rollo-259] [^c1691-238] [^c1691-280] -->
## Planning File Conventions

The canonical planning files are `docs/plan.md`, `docs/BACKLOG.md`, and root `WIP.md`, with `docs/plan/` for detailed active plans. The WIP tracker must live at root `WIP.md`, not `Plans/WIP.md`. WIP.md must contain only branch/worktree-level entries; milestone checklists, backlog text, and long status reports must not go there. The active planning surface must use `docs/plan.md`, `docs/BACKLOG.md`, and `docs/plan/<slug>.md` for detailed active plans. The existing tracked `Plans/` directory must be treated as historical/reference unless a specific item is promoted; it must not be edited as a live status tracker. When a `Plans/` item becomes active, it must be linked from `docs/plan.md` or moved/summarized into `docs/plan/<slug>.md`, with execution tracked in `docs/BACKLOG.md` and `WIP.md`. <!-- [^rollo-189] -->
