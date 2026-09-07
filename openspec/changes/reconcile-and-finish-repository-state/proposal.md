> **Delivery status (2026-09-07): superseded by `remove-nostr-support`.**
> Completed non-Nostr reconciliation work is retained by the replacement;
> NMP-specific work is deleted, and unfinished Voice/audio/Siri expansion stays
> deferred to GitHub issues #142 and #84 rather than being silently claimed.

## Why

Pod0 has meaningful completed work, but no single trustworthy repository state: published `master`, the local 38-commit-ahead branch, the dirty root worktree, two auxiliary worktrees, and a stash each contain different pieces, while the current checkout fails architecture, formatting, Rust, binding, package-resolution, and product-acceptance gates. The repository must be reconciled before further feature work so every retained change has provenance and proof, every discarded change is intentionally superseded, and completion again means one green, published, reproducible state.

## What Changes

- Inventory every tracked modification, untracked artifact, stash, local-only commit, auxiliary worktree, and remote branch; classify each item as retain, supersede, archive, or discard with an evidence-backed rationale.
- Build one canonical integration lineage from `origin/master`, preserving user work and applying only the best non-duplicative implementation of each retained behavior.
- Reconcile the headless-host Phase 1 record with the actual code: integrate the later contract-fixture and facade/bootstrap work where correct, resolve overlapping root WIP, remove obsolete test ignores, and make planning state agree with repository state.
- Restore all mandatory repository gates: architecture documentation and inventories, Rust formatting/clippy/tests/BDD, file-length limits, generated bindings, package locks, Apple toolchain inputs, simulator build/tests, shared-workflow recovery, portability, and non-publishing archive.
- Make diagnostic tooling robust to legitimate binary repository artifacts without weakening its source or secret-boundary checks.
- Complete the existing Voice-to-Rust roadmap: one durable shared voice/text agent session, Rust-acknowledged cancellation and approval, real audio interruption/route handling, physical-device qualification, then Siri/Shortcuts re-enablement.
- Publish the reconciled branch only after local and hosted checks cover the same commit; leave no unexplained WIP, stale worktree, or contradictory completion record behind.
- **BREAKING**: remove or replace obsolete stubs, disabled routing guards, duplicate authority paths, and superseded local work rather than retaining compatibility shims for unfinished behavior.

## Capabilities

### New Capabilities

- `repository-reconciliation`: Defines the evidence, provenance, disposition, cleanliness, and publication requirements for converging all repository lineages and WIP into one authoritative state.
- `delivery-readiness`: Defines the reproducible toolchain, mandatory validation gates, simulator/archive evidence, and hosted-CI conditions required before Pod0 work is considered landed or releasable.
- `voice-agent-authority`: Defines one durable Rust-owned conversation authority shared by text and voice, including exclusivity, recovery, cancellation, failures, and explicit approvals.
- `audio-route-qualification`: Defines native audio interruption/route behavior and the required physical-device qualification matrix.
- `siri-voice-routing`: Defines the gated cold/warm Siri and Shortcuts behavior that may be enabled only after voice authority and audio qualification are proven.

### Modified Capabilities

None. This repository currently has no main OpenSpec capability specifications; the change formalizes existing roadmap requirements as new capabilities.

## Impact

- Repository state: `master`, `origin/master`, local commits, the dirty root worktree, `.claude/worktrees/*`, stashes, untracked planning/runtime artifacts, and GitHub CI/release state.
- Rust: `pod0-application`, `pod0-storage`, `pod0-facade`, `pod0-cli`, the five other host crates, BDD fixtures, generated UniFFI contracts, and architecture/policy inventories.
- Apple app: shared agent session, Voice Mode, audio-session coordination, App Intents/Shortcuts, generated Xcode project/package inputs, simulator/device tests, and archive workflow.
- Delivery: GitHub Actions, branch publication, milestone/issue status, and proof artifacts. No TestFlight upload is implied; publishing to TestFlight remains a separately confirmed action.
