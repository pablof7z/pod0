# Agent Guidelines

## Agent workflow

- All implementation work must happen in a git worktree owned by the agent doing the work.
- Each agent is responsible for its own branch/worktree lifecycle. Do not edit from the shared root checkout for feature, fix, or refactor work.
- Before starting work, every agent must read `WIP.md` from the **project base directory** (`/path/to/podcast-player/WIP.md`) to understand what other agents are currently doing.
- When an agent starts work, it must add an entry to `WIP.md` **in the project base directory** (not in its worktree) with a timestamp, a one-line description of the work, the branch name, and the git worktree path it is using.
- **`WIP.md` is gitignored and must NEVER be committed.** It lives only in the main checkout directory and is shared across all worktrees. Agents read and write it at its absolute path — do not stage it, do not `git add` it, do not include it in any commit.
- When the work is complete, open a pull request before reporting completion. The PR description must include a short TLDR, a detailed overview of the work performed, validation performed, and any subjective decisions, tradeoffs, or assumptions.
- Do not open completed work as a draft pull request. Use draft PRs only when explicitly asked or when the work is intentionally incomplete.
- After the PR merges, remove the agent's own entry from `WIP.md` (at the project base directory) and clean up the agent-owned worktree.

## Planning discipline

This repository uses three canonical planning/status files:

| File | Role | Update cadence |
|---|---|---|
| `docs/plan.md` | Overarching plan, active milestones, and current implementation focus. | Only when milestone status or active focus changes. |
| `docs/BACKLOG.md` | Tactical queue, active violations, pending user decisions, and follow-up work. | Every PR that touches a listed item. |
| `WIP.md` | Live in-flight tracker for branches currently on worktrees. | When an agent starts work and when its PR merges. |

- Do not create new top-level planning files. No new `PLAN.md`, `TODO.md`, `ROADMAP.md`, `NEXT.md`, `STATUS.md`, or ad-hoc plan files at the repo root or directly under `docs/`.
- New detailed implementation plans belong under `docs/plan/` and must be linked from `docs/plan.md`. Do not recreate deleted historical plan directories.
- Do not duplicate state across files. If a backlog item is actively being fixed, `docs/BACKLOG.md` keeps the item and `WIP.md` only records the active branch/worktree.
- Edit existing entries instead of appending parallel ones. A plan or backlog item should have one canonical home.
- Inline `TODO` comments are not a planning system. If they represent follow-up work, track them in `docs/BACKLOG.md`.

## Validation

- Scope local validation to the files and behavior touched by the change, then record the exact commands in the PR.
- Run `tuist generate` whenever project shape, target membership, resources, entitlements, packages, or generated Xcode project state may have changed.
- Prefer focused `xcodebuild test` runs for the touched test bundles during development. Full-suite simulator validation is the merge/supervisor gate unless the change is broad enough to require it earlier.
- Always run `git diff --check` before opening a PR.

### xcodebuild plugin trust

The `secp256k1.swift` package (version ≥ 0.23.2) ships a `SharedSourcesPlugin`
BuildToolPlugin that requires explicit trust. Interactive Xcode builds prompt the
user once and remember the answer; headless / CI builds must pass the flag:

```
-skipPackagePluginValidation
```

Add this flag to every `xcodebuild` invocation in CI scripts and to any MCP
`build_sim` / `build_device` call via `extraArgs: ["-skipPackagePluginValidation"]`.
Without it the build fails with "Validate plug-in SharedSourcesPlugin… BUILD FAILED".

## Whats-new changelog

Every commit that ships a user-facing change to the iPhone MUST add an entry to `App/Resources/whats-new.json` with a one-liner the user will read. An entry needs only `shipped_at` (current UTC, ISO-8601) and `lines`. The app surfaces entries whose `shipped_at` is newer than the user's last-seen marker — no commit SHA needed. Timestamps must be unique across entries; if two land in the same minute, bump one by a minute. Skip entries for purely-internal commits (encoder caches, log line tweaks, formatting). When in doubt: would the user notice? If yes, add a line.

## Typography

**No serif fonts, ever.** Do not use `.serif` font design, `NewYork`, `NewYork-SemiboldItalic`, or any other serif typeface anywhere in the app. All text must use SF (system font). For italic style, use `UIFont.italicSystemFont` or `.italic()` modifier — never a serif variant.

## File Length Limits

- **Soft limit: 300 lines** — prefer splitting into smaller files when approaching this threshold.
- **Hard limit: 500 lines** — files must not exceed 500 lines. Refactor before adding more code.

## Engineering discipline

- No temporary hacks. Staged work is acceptable only when the staging is captured in `docs/BACKLOG.md` with clear follow-up ownership.
- Avoid fragmentation: every durable concept should have one canonical representation and one code path.
- Prefer the long-term correct architecture over a local patch that only makes the immediate build green.
