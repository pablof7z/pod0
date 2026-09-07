## Why

Pod0 still carries a Nostr/NMP integration that introduces identity, relay, publication, recovery, and startup behavior outside the product's core podcast and agent responsibilities. The integration is incomplete, contradicts the intended Rust ownership boundary, and has already appeared in a stalled runtime path; retaining it adds risk without a required product capability.

## What Changes

- **BREAKING** Remove all Nostr and NMP functionality from Pod0, including account creation, keychain identity storage, relay configuration, event publication, receipt tracking, restart recovery, and status projection.
- **BREAKING** Remove every Pod0 command, facade type, persisted table/state transition, generated binding, UI entry point, and agent-tool path whose purpose is Nostr publication.
- Remove the pinned NMP dependency, package-preparation scripts, build inputs, lock entries, and CI/tooling checks that exist solely for that dependency.
- Remove Nostr/NMP fixtures, tests, architecture surfaces, ownership claims, ADR requirements, planning claims, and tracker language; preserve historical records only where rewriting history would be misleading, clearly marking the capability as removed.
- Migrate existing installations by deleting Nostr/NMP-owned local state and retiring Pod0 publication records without attempting delivery, retry, or compatibility fallback.
- Ensure application startup, shared-library bootstrap, agent workflows, and user-data erasure no longer initialize or consult Nostr/NMP components.
- Reconcile the entire checkout before delivery: retain and finish valid work, discard superseded or rejected work with recoverability evidence, and assign every branch, worktree, stash, modified file, and untracked artifact an explicit disposition.
- Land the qualified result on `master`, remove temporary reconciliation state only after retained work is remote-backed, and leave the repository on a clean, synchronized `master` checkout.

## Capabilities

### New Capabilities

- `nostr-free-product`: Defines Pod0 as having no Nostr/NMP runtime, persistence, API, dependency, user surface, or product workflow, including deterministic retirement of previously stored Nostr-related state.

### Modified Capabilities

None. The repository has no established main OpenSpec capabilities to modify.

## Impact

- Swift: removes `App/Sources/NMP`, NMP composition from the shared-library client, related account/publication UI, and native receipt translation.
- Rust and UniFFI: removes publication commands, domain models, application transitions, storage/outbox tables, facade methods, generated Swift/Kotlin bindings, and tests.
- Build and dependencies: removes the local NMP Swift package, preparation scripts, pinned revision inputs, generated NMP binaries, project linkage, and corresponding CI checks.
- Persistence: requires an explicit one-way schema migration that retires publication obligations and removes Nostr-specific durable state without performing network effects.
- Documentation and planning: supersedes the NMP boundary ADRs and removes Nostr ownership, conformance, roadmap, issue, and release-readiness claims.
- Product behavior: any existing or planned Nostr identity, publication, relay, remote-agent, or private-recipient feature ceases to exist; no compatibility shim or dormant feature flag remains.
- Repository delivery: the active reconciliation change and all local Git state must be reviewed together so no unique work is silently lost, duplicated, or left stranded outside `master`.
