## Context

See [proposal.md](proposal.md) for motivation and [specs/nostr-free-product/spec.md](specs/nostr-free-product/spec.md) for required behavior.

Nostr currently crosses the whole product: a pinned local NMP Swift package, app startup/account composition, Keychain and engine storage, Rust publication intent and receipt state, UniFFI APIs, generated Swift/Kotlin bindings, agent publication tools, persistence migrations, conformance inventories, tests, and architecture/planning claims. The working checkout also contains uncommitted and locally committed reconciliation work outside `master`, plus other Git worktrees and stashes that must not be silently lost.

The app's existing ownership rule remains: Rust owns durable product meaning; Swift owns UI and narrowly typed platform capabilities. Removing Nostr should simplify that boundary rather than replace NMP with a Pod0-owned protocol implementation.

## Goals / Non-Goals

**Goals:**

- Remove every executable and dormant Nostr/NMP path from the shipped product and build graph.
- Retire old Pod0 Nostr state locally without causing a network effect.
- Preserve and qualify unrelated podcast, listening, transcription, clipping, search, memory, and agent behavior.
- Reconcile all Git state, land retained work through one reviewable candidate, merge it to `master`, and finish on clean synchronized `master`.

**Non-Goals:**

- Replacing NMP with another Nostr library or an in-house Nostr implementation.
- Preserving Nostr identity, pending publications, receipt history, relay preferences, or source compatibility.
- Adding a feature flag, optional build flavor, migration fallback, or future-facing protocol abstraction.
- Rewriting immutable Git history or historical records to pretend Nostr never existed.
- Shipping unrelated unfinished product expansion merely because it exists in local WIP; such work receives an explicit retain, defer, or discard disposition.

## Decisions

### 1. Delete the capability from its product roots

Removal starts at Pod0 commands and user/agent surfaces, then follows references through domain/application state, persistence, facade and bindings, Swift composition, dependency preparation, tests, and documentation. This prevents a bottom-up dependency deletion from leaving dead product concepts or compatibility branches behind.

Alternative: keep publication types behind a disabled flag. Rejected because it preserves business logic, generated API surface, migration burden, and an easy path for accidental reactivation.

### 2. Do not replace NMP

NMP, its Swift package, local build preparation, relay configuration, account store, receipt mapping, and all NMP-specific generated or cached artifacts are removed. No generic "social protocol" interface is introduced.

Alternative: retain a generic publication port for future use. Rejected because there is no current consumer and the port would encode speculative requirements from the removed feature.

### 3. Use one effect-free retirement migration

The Rust schema migration atomically removes publication obligations, leases, receipt links, observations, and projections. A narrow native retirement step deletes the known Pod0-specific NMP engine store and Keychain entry using platform storage APIs without constructing an NMP engine. Both operations are idempotent and run before ordinary services start.

The retirement marker records only that cleanup completed; it contains no Nostr identity or recoverable publication data. Interrupted cleanup retries deletion. It never signs, publishes, reattaches, or opens a relay connection.

Alternative: leave old stores orphaned. Rejected because private key material and retryable publication state would remain on device. Alternative: initialize NMP once to delete its account. Rejected because removal must not execute Nostr code or risk a network effect.

### 4. Regenerate bindings after deleting the source API

Publication-related facade methods and types are removed at their Rust owners. Swift and Kotlin bindings are regenerated from the reduced facade rather than hand-edited. Binding drift, architecture conformance, and forbidden-surface checks become ratchets against reintroduction.

Alternative: retain deprecated facade methods returning unsupported errors. Rejected because that is source compatibility and a dormant surface.

### 5. Preserve honest history, remove current claims

Current architecture, ownership inventories, roadmap/state files, release evidence, and tracker descriptions are rewritten to describe a Nostr-free product. Historical ADRs and completed change records remain only when deletion would falsify history; they are prominently marked superseded and excluded from current conformance scans.

Alternative: delete every textual occurrence of "Nostr" or "NMP". Rejected because migration code and historical decisions need auditable context. Active source, APIs, manifests, generated files, and current-state documents receive a zero-tolerance check; explicitly allowlisted retirement/history records do not.

### 6. Reconcile Git state before destructive cleanup

Before changing source, enumerate the current checkout, local and remote branches, upstream divergence, worktrees, stashes, untracked files, and existing safety refs/archives. Every unique item gets a ledger row with one disposition:

- retain and integrate;
- superseded by an identified retained commit;
- defer to a named open tracker item with a recoverable ref;
- discard with a recorded archive/ref and rationale.

The existing `reconcile-and-finish-repository-state` change is reconciled into this delivery: completed unrelated fixes are retained if they pass current gates; Nostr work is superseded by deletion; speculative or incomplete work is not silently promoted. Cleanup happens only after the candidate is pushed and reviewable.

Alternative: reset directly to `master` and reapply selected patches. Rejected because unique local work could be lost and the current checkout already contains validated fixes.

### 7. Land one qualified candidate, then return to master

Implementation is committed in reviewable units on the current reconciliation branch or a direct successor. The final candidate must pass strict Rust, binding, architecture, dependency, Apple release-input, portability, full iOS test, clean simulator launch, and Nostr-absence gates. It is pushed, reviewed through one pull request, and merged only when hosted checks pass for the exact candidate ancestry.

After merge, the default branch is fetched and checked out, temporary branches/worktrees/stashes are removed according to the ledger, and final audit proves a clean synchronized `master`. No publish or TestFlight action is implied.

## Risks / Trade-offs

- [Existing users permanently lose a Pod0-created Nostr identity] → Treat deletion as intentional and breaking; remove the Pod0-specific Keychain entry and document that rollback cannot recover it.
- [A pending publication could be delivered during upgrade] → Run retirement before constructing services and prove migration uses no NMP code or network capability.
- [Cross-cutting deletion leaves a hidden path] → Inventory by concept, symbol, dependency, persisted schema, generated API, runtime connection, and UI surface; enforce ratchet scans plus clean builds.
- [Schema deletion breaks unrelated data] → Delete only Nostr-owned rows/tables and test migration against empty, completed, pending, leased, ambiguous, and interrupted fixtures.
- [Removing NMP masks the separate startup defect] → Require a clean install and migrated install to reach the usable library UI; diagnose any remaining bootstrap failure independently.
- [Large WIP reconciliation obscures review] → Use atomic commits, a disposition ledger, exact-SHA evidence, and one PR whose body separates retained fixes, Nostr removal, migration, and cleanup.
- [Destructive Git cleanup loses unique work] → Create and verify recoverable refs/archives before removal, push retained commits first, and apply cleanup only after merge.

## Migration Plan

1. Freeze the current checkout and refresh the full Git-state disposition ledger with recoverability evidence.
2. Commit or archive retained pre-existing reconciliation work in coherent units; explicitly supersede or discard the rest.
3. Remove product-level Nostr commands and workflows, then delete Rust state/persistence/facade surfaces and add the effect-free retirement migration.
4. Remove Swift NMP composition and storage, dependency/build tooling, generated bindings, tests, UI, configuration, and current documentation claims.
5. Regenerate all bindings and project artifacts; run focused migration and absence tests, then the complete local qualification matrix.
6. Push the exact candidate, update the existing review path and trackers, and require hosted checks to pass on candidate ancestry.
7. Merge to `master`; fetch and check out the merged default branch.
8. Apply every recorded worktree, branch, stash, untracked-artifact, and generated-debris disposition; run the final clean-repository audit.

Rollback is source-only: the merged commit can be reverted if necessary, but deleted Nostr keys and retired publication obligations are intentionally unrecoverable from the app. Safety archives protect repository work, not user identity material.
