---
title: Project Workflow
slug: project-workflow
topic: project-setup
summary: "The repository uses exactly three canonical planning/status files: docs/plan.md for the overarching plan, active milestones, and current implementation focus; d"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-25
updated: 2026-06-14
verified: 2026-05-25
compiled-from: conversation
sources:
  - session:rollout-2026-05-25T22-18-09-019e6092-b9c8-7f53-b3e0-898bfeec48c4
  - session:rollout-2026-05-25T22-38-52-019e60a5-b1f5-7883-b0d3-a8d1826b1709
  - session:rollout-2026-05-26T09-10-44-019e62e8-3112-7080-98ca-48ac46d8b8d2
  - session:rollout-2026-05-26T10-15-57-019e6323-e656-7e70-b925-0b0c837b24a1
  - session:rollout-2026-05-26T10-16-01-019e6323-f6b1-7f03-85ec-1a51289f331a
  - session:rollout-2026-05-26T10-16-06-019e6324-08b0-7ca3-9b29-a0e8cf84941d
  - session:rollout-2026-05-26T10-16-10-019e6324-1915-7ba0-91ff-8397304bb76a
  - session:rollout-2026-05-26T10-26-17-019e632d-5c5d-7bb3-8b90-fb176055c79d
  - session:rollout-2026-05-26T12-25-40-019e639a-a8b2-7311-b4c4-c76fb89c77a8
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Project Workflow

## Planning & Status Files

The repository uses exactly three canonical planning/status files: docs/plan.md for the overarching plan, active milestones, and current implementation focus; docs/BACKLOG.md for the tactical queue, active violations, pending decisions, and follow-up work; and WIP.md for live in-flight branch tracking. docs/plan.md must only be updated when milestone status or active focus changes. docs/BACKLOG.md must be updated on every PR that touches a listed item.

No new top-level planning files (e.g., PLAN.md, TODO.md, ROADMAP.md, NEXT.md, STATUS.md, or ad-hoc plan files) may be created at the repo root or directly under docs/. New detailed implementation plans belong under docs/plan/ and must be linked from docs/plan.md; deleted historical plan directories must not be recreated. The Plans/ directory and all historical plan files under it must not exist in the repository. (Previously: Existing files under Plans/ were historical reference material and new active plans were forbidden there.)

State must not be duplicated across planning files; a plan or backlog item should have one canonical home, and if an item is actively being fixed, docs/BACKLOG.md keeps the item while WIP.md records only the active branch/worktree. Existing plan or backlog entries must be edited in place rather than appending parallel ones. Inline TODO comments are not a planning system; if they represent follow-up work, they must be tracked in docs/BACKLOG.md.

docs/plan.md and docs/BACKLOG.md must be updated to reflect real current state, not PR-era state; they must be kept current with actual PR and milestone status.

New feature fan-out must stop until existing correctness issues are resolved. Compat stubs (ServiceStubs.swift and related identity/domain/utility shims) must be burned down; done means user-visible behavior works, not just that a view compiles. AI/platform surfaces (comments, social graph, RAG, wiki, briefings, voice) are scaffold-level, not feature parity; they must be treated as incomplete until user-visible behavior works.

Four compat delete items targeting nonexistent ios/Podcast/Podcast/Compat paths are struck from BACKLOG; episode-comments-relay-wiring is marked done; social-publish-relay-target already routes via NMP Auto rather than hardcoded relay.primal.net.

<!-- citations: [^rollo-209] [^rollo-217] [^rollo-231] [^rollo-236] [^rollo-242] [^rollo-249] [^rollo-255] [^rollo-264] [^c1691-272] -->
## Pull Requests

When work is complete, a pull request must be opened before reporting completion, and the PR description must include a short TLDR, a detailed overview of the work performed, validation performed, and any subjective decisions, tradeoffs, or assumptions. Completed work must not be opened as a draft pull request; draft PRs are only for intentionally incomplete work or when explicitly requested. Scope-local validation must be run on the files and behavior touched by the change, and the exact commands must be recorded in the PR. git diff --check must always be run before opening a PR.

<!-- citations: [^rollo-210] [^rollo-237] [^rollo-243] [^rollo-250] [^rollo-261] -->
## Architecture & Staging Discipline

No temporary hacks or 'for now this is ok' approaches are allowed; proper fixes built on solid ground are required. Staged work is acceptable only when the staging is captured in docs/BACKLOG.md with clear follow-up ownership. The long-term correct architecture must be preferred over a local patch that only makes the immediate build green. Every durable concept should have one canonical representation and one code path to avoid fragmentation.

The NMP doctrine requires Rust to own decisions and UI to execute/render only. The store implementation uses JSON-backed storage rather than the plan's sled. Push delivery is still incomplete relative to the plan.

<!-- citations: [^rollo-211] [^rollo-218] [^rollo-244] [^rollo-265] [^c1691-367] -->
## Stale & Conflicting PRs

Many PRs are experiencing merge conflicts because they were opened in parallel from older snapshots of the NMP migration and edit the same small set of central files (snapshot.rs, projections.rs, host_op_handler.rs, store/mod.rs, store/tests.rs, PodcastTypes.generated.swift, whats-new.json, docs/BACKLOG.md). For dirty PRs, the feature should be reapplied on top of current main rather than blindly resolving old branch structure; PRs whose intent no longer matches the current NMP architecture should be closed or superseded.

PR #1 is for a prior UniFFI/Rust-core attempt and should be closed.

The pr-icloud-sync worktree has one unique commit (feat: iCloud settings sync via NSUbiquitousKeyValueStore) that needs review/landing or an explicit discard decision before cleanup.

<!-- citations: [^rollo-212] [^rollo-238] -->

## Agent WIP Discipline

Before starting work, every agent must read WIP.md from the project base directory to understand what other agents are currently doing. All implementation work must happen in a git worktree owned by the agent doing the work, not in the shared root checkout. When an agent starts work, it must add an entry to WIP.md in the project base directory with a timestamp, a one-line description of the work, the branch name, and the git worktree path.

WIP.md is gitignored and must never be committed; it lives only in the main checkout directory, is shared across all worktrees, and agents must read and write it at its absolute path without staging, adding, or including it in any commit.

Review agents are forbidden from performing working-tree git operations in the shared root; only git diff/git show on object-DB reads are permitted.

After a PR merges, the agent must remove its own entry from WIP.md in the project base directory and clean up its worktree.

Important notifications must be sent via the say command. <!-- [^c1691-393] -->

<!-- citations: [^rollo-239] [^rollo-251] [^rollo-263] [^c1691-111] [^c1691-368] -->
## Implementation & Testing Discipline

The local main branch must be reconciled with origin/main before continuing implementation from the root checkout. After NIP-F4 correctness is fixed, focused Rust tests for podcast-discovery/nmp-app-podcast must be run, then full cargo test --workspace, git diff --check, and the iOS build/test gate.

<!-- citations: [^rollo-245] [^rollo-252] -->
