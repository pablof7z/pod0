## Context

See `proposal.md` for motivation. The remote default branch is an ancestor of the local `master`, which is 38 commits ahead, so the local commit chain is valuable provenance rather than a divergent rewrite candidate. Additional unique work exists in a dirty root checkout, a clean auxiliary branch two commits ahead of local `master`, a detached physical-device worktree with a project-file modification, one stash, and untracked source/planning/audio artifacts. Current validation is mixed: strict Rust linting passes, while formatting, contract fixtures, BDD, facade/storage tests, architecture inventories, generated binding freshness, package resolution, and Apple tool version checks do not.

The permanent ownership rule remains: native code executes platform primitives; Rust owns durable product decisions. The repository-wide SF typography rule and 500-line hard limit remain mandatory. Physical-device and TestFlight claims require evidence distinct from source, simulator, and archive evidence.

## Goals / Non-Goals

**Goals:**

- Preserve all unique authored work while reducing the repository to one explainable lineage.
- Restore a green baseline before product migration continues.
- Finish the already-scoped Voice-to-Rust milestone without introducing another conversation or audio authority.
- Produce exact-SHA local, hosted, simulator, archive, and device evidence.
- Remove obsolete branches, stashes, guards, stubs, ignores, and generated debris only after their dispositions are recoverable and proven.

**Non-Goals:**

- No history rewrite of the 38-commit direct extension merely to make the graph prettier.
- No Android application work or Android investment-gate decision.
- No move of speech recognition, synthesis, AVFoundation, audio sessions, Keychain, Siri, or other platform primitives into Rust.
- No TestFlight upload or App Store release. Repository WIP cleanup is authorized by this change, but only after the disposition and recoverability gates defined here pass.
- No compatibility layer for unfinished stub behavior or duplicate state owners.

## Decisions

### 1. Preserve the direct commit lineage and reconcile forward

Use the current local `master` tip as the integration ancestry because `origin/master` is its direct ancestor. Create a safety reference and a dedicated reconciliation branch before applying any WIP disposition. Review the two auxiliary commits independently: the fixture-version correction is a narrow candidate for direct integration; the later facade/CLI/bootstrap commit overlaps the dirty root state and must be decomposed by behavior rather than blindly cherry-picked.

Alternative considered: reset to `origin/master` and rebuild the work from patches. Rejected because it would erase useful commit provenance and increase the chance of losing already verified Phase 1 work.

### 2. Use an explicit disposition ledger as the deletion gate

Add a repository-state ledger to the change evidence. Each item records identity, ownership, semantic purpose, overlap, decision, retained destination, validation, and recovery reference. Cleanup of a branch, stash, worktree, or unique file is allowed only after its ledger row is complete.

Alternative considered: rely on Git reflog and ad hoc notes. Rejected because reflog is local and expiring, and it does not explain semantic disposition.

### 3. Restore baseline correctness before extending product behavior

The implementation order is: reconcile contract/version and bootstrap variants; fix all existing mandatory gates; regenerate bindings and project inputs using pinned tools; then begin voice/audio work. Existing failing tests are treated as unresolved behavior until their assertions and ownership contracts are reconciled—not simply updated to accept current output.

Alternative considered: build Voice Mode while cleanup proceeds. Rejected because failing baseline gates make regressions and ownership mistakes impossible to attribute confidently.

### 4. Keep one durable agent-session adapter

Introduce one native adapter from Voice Mode into the existing shared Rust-owned conversation session. It observes the same conversation identity, availability, revisions, streaming state, approval stage, and terminal outcome as text chat. Barge-in first stops local speech and then completes only when the revision-fenced Rust cancellation is acknowledged; late output is ignored by authoritative state.

Alternative considered: a voice-specific session or transcript store. Rejected because it recreates dual-writer recovery and approval problems.

### 5. Keep audio ownership native and event-driven

Extend the native audio owner with a typed event stream consumed by playback and Voice Mode. The stream reports system facts; Rust continues to own durable playback/agent decisions where applicable. Avoid polling and avoid `voiceChat` mode because AirPlay qualification is required.

Alternative considered: let Voice Mode configure `AVAudioSession` directly. Rejected because playback and voice would again compete for one process-global platform resource.

### 6. Treat physical-device qualification as a hard dependency

Phase 2 voice authority and Phase 3 audio work can be implemented independently after the green baseline, but Siri/Shortcuts enablement is a final small change gated on both. Device evidence names the hardware, OS, candidate SHA, scenario, and result. Simulator results cannot satisfy device requirements.

Alternative considered: merge Siri routing disabled and enable later through an untracked flag. Rejected because it permits planning and production state to drift.

### 7. Publish and prove the same commit

After local validation, publish the reconciliation branch, open or update one reviewable PR, run hosted CI, address only evidence-backed failures, and merge without modifying the tested tree. Re-run or verify CI on the resulting default-branch SHA when the merge strategy changes the commit identity. Update issues, milestones, and planning records from that evidence.

Alternative considered: push the dirty local `master` directly. Rejected because the current state lacks reviewable WIP dispositions and same-SHA validation.

## Risks / Trade-offs

- [Overlapping WIP hides unique behavior] → Compare patches and tests at the behavior level; keep safety refs and a disposition ledger until hosted validation passes.
- [Fixing fixtures masks a real contract migration omission] → Verify every generated binding and cross-language fixture for version 55 before accepting the numeric bump.
- [BDD expectations and implementation have both drifted] → Resolve scenarios against product requirements and authoritative domain ownership, then fix implementation or scenario—not whichever is easier.
- [Long-running or flaky tests create false confidence] → Reproduce failures in isolation, retain the full-suite result, and require repeated full-suite stability for concurrency-sensitive tests.
- [Toolchain repair regenerates broad project output] → Pin the declared versions first, regenerate from canonical inputs, and review generated diffs separately from behavioral code.
- [Binary handling weakens secret scanning] → Scope checkers by declared file type and content detection; never blanket-ignore directories or extensions containing configuration/source.
- [Physical hardware is unavailable] → Complete all non-device work and keep Siri routing blocked; do not redefine simulator proof as completion.
- [Repository cleanup becomes destructive] → Delay branch, stash, worktree, and file removal until retained commits are remote-backed and recovery references are verified.

## Migration Plan

1. Capture the full inventory, hashes, refs, diffs, and current validation results; create non-destructive safety refs.
2. Create the reconciliation branch from the local direct-extension tip and integrate the narrow fixture correction.
3. Decompose the overlapping facade/bootstrap/CLI work, retain one coherent implementation, and validate each behavior with focused tests.
4. Fix baseline test, BDD, architecture, formatting, file-length, checker, toolchain, binding, and package-resolution failures; obtain a clean full local gate.
5. Implement and validate shared voice authority and native audio events as separate reviewable slices.
6. Execute the physical-device matrix. If unavailable or failing, stop with Siri routing disabled.
7. Enable and validate cold/warm Siri/Shortcuts routing only after all prerequisite evidence is green.
8. Publish, pass hosted CI and archive on the exact candidate, merge, confirm the default-branch SHA, and reconcile issues/planning.
9. Apply the disposition ledger cleanup only after the retained state is remote-backed and recoverable.

Rollback is forward-safe: revert the smallest landed slice while retaining durable schema compatibility and keeping Voice/Siri entry points disabled. Do not restore stub or duplicate authorities. Repository cleanup itself rolls back through the recorded safety refs and archived WIP patches.
