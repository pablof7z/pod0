## Context

The removed subsystem crossed product commands, agent permissions, Rust state machines, SQLite tables, UniFFI, Swift composition, generated bindings, Tuist inputs, CI scripts, tests, and historical planning material. The repository also contains committed and uncommitted work outside `master` that must be reconciled without losing unrelated fixes.

## Goals / Non-Goals

**Goals**

- Delete the capability at its product roots and follow every reference through the build and runtime graph.
- Establish literal absence of the retired protocol names in tracked paths and content.
- Preserve unrelated product behavior and prove startup reaches a usable library.
- Account for all Git state, merge retained work, discard superseded work, and end on clean synchronized `master`.

**Non-goals**

- A disabled feature, compatibility shim, generic replacement port, or future-facing abstraction.
- Native cleanup code that preserves knowledge of the deleted integration.
- Rewriting Git object history; the zero-reference rule applies to the resulting checkout, not unreachable historical commits.
- Publishing a release or changing TestFlight.

## Decisions

### Delete rather than deprecate

User, agent, command, domain, persistence, facade, generated, Swift, dependency, and documentation surfaces are removed. Former calls are absent; they do not return compatibility errors.

### Use a literal zero-reference gate

A repository checker constructs the forbidden identifiers internally so its own source does not contain them contiguously. It scans tracked source and path names with no exception for OpenSpec, history documents, fixtures, migrations, or generated output. A self-test seeds both content and path violations.

### Keep only a neutral Rust schema deletion

The supported Rust database advances through one atomic and idempotent migration that removes obsolete publication tables and related journal/outbox rows. It performs no network operation. No native store/keychain retirement adapter is retained because that would keep deleted integration knowledge in production Swift.

### Regenerate, do not hand-maintain, bindings

Facade removals are made at the Rust owner, the contract version advances, and Swift/Kotlin bindings are regenerated. Drift checks prove generated artifacts match the reduced source API.

### Reconcile Git state before cleanup

Every unique branch, worktree, stash, dirty path, and untracked artifact receives a recoverable reference and a retain/supersede/discard disposition. Retained commits are remote-backed before destructive cleanup. One pull request carries the qualified candidate to `master`.

## Risks / Trade-offs

- Old standalone integration data may remain on devices because retaining a native cleanup adapter would violate the zero-reference requirement. The app no longer links or accesses it.
- Broad document deletion can remove useful unrelated context, so dedicated records are deleted while mixed records retain only unrelated content.
- Cross-cutting removal can leave cached artifacts; clean regeneration and an explicit build-output scan close that gap.
- Git cleanup can lose unique work; the verified bundle, patch, archive, and backup ref remain until retained work is merged.

## Migration Plan

1. Freeze and classify all repository state with recoverability evidence.
2. Delete runtime and build surfaces from Rust and Swift.
3. Apply the neutral Rust schema deletion migration and regenerate bindings/project files.
4. Delete dedicated documentation and scrub mixed records; enable the literal absence ratchet.
5. Run architecture, Rust, Kotlin, Apple, simulator, and full iOS qualification.
6. Integrate retained WIP, push one candidate, merge it, clean auxiliary Git state, and finish on synchronized `master`.

Rollback is source-only. Repository safety archives preserve pre-change development work; no removed capability is kept for runtime rollback.
