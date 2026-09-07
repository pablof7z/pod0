## 1. Freeze and Reconcile Existing Work

- [x] 1.1 Refresh remote refs without changing the checkout, record the current branch/HEAD/upstream divergence, and verify the existing safety refs, bundle, patch, and untracked archive can recover the pre-change state.
- [x] 1.2 Enumerate every local and remote branch, linked worktree, stash, modified path, deleted path, and untracked artifact; verify every unique item has one row in the reconciliation ledger with provenance and a recoverable ref or archive.
- [x] 1.3 Assign every ledger row exactly one disposition—retain, superseded, deferred with a named tracker item, or discard with rationale—and verify no row is ambiguous or unapplied.
- [x] 1.4 Reconcile the existing `reconcile-and-finish-repository-state` change against this proposal; verify completed unrelated fixes remain represented, Nostr work is marked superseded, and speculative unfinished work is explicitly retained, deferred, or discarded.
- [ ] 1.5 Commit the retained pre-existing non-Nostr baseline in reviewable units before beginning destructive removal; verify each commit contains only ledger-approved paths and all unique retained work remains reachable.

## 2. Remove Pod0 Publication Behavior

- [ ] 2.1 Build an exact active-surface inventory covering Nostr/NMP product commands, agent tools, permissions, domain types, transitions, projections, persistence, facade APIs, generated bindings, Swift composition, dependencies, scripts, UI, and configuration; verify every discovered surface maps to a later deletion or retirement task.
- [ ] 2.2 Remove Nostr publication and identity actions from user surfaces, automation, agent tools, permission matrices, and command routing; verify former actions are absent rather than returning a compatibility error.
- [ ] 2.3 Delete Pod0 Rust publication intents, identifiers, receipt/status models, commands, reducers, workflows, projections, leases, and effect-outbox paths; verify no non-migration Rust production module references the removed concepts.
- [ ] 2.4 Remove publication-related application and facade methods and types at their Rust owners; verify facade/schema checks expose no Nostr/NMP or generic replacement publication API.
- [ ] 2.5 Delete obsolete publication-specific tests and rewrite affected non-Nostr tests around their remaining product behavior; verify test removal does not reduce coverage for unrelated agent commits, approvals, or artifact generation.

## 3. Retire Existing State Without Network Effects

- [ ] 3.1 Add one atomic, idempotent Rust schema migration that removes publication obligations, leases, receipts, observations, and projections; verify fixtures cover empty, completed, pending, leased, retryable, ambiguous, and interrupted migration states.
- [ ] 3.2 Add the narrow native retirement step that deletes the known Pod0 NMP engine store and Keychain item without importing or constructing NMP; verify clean, populated, missing, and interrupted cleanup cases.
- [ ] 3.3 Order retirement before ordinary service startup and persist only a generic completion marker; verify an upgraded installation performs no signing, publication, receipt reattachment, DNS lookup, or relay connection.
- [ ] 3.4 Integrate the retirement paths with user-data erasure; verify erasure and relaunch leave no Pod0-specific Nostr engine, keychain, or Rust publication state.

## 4. Remove Native NMP and Build Integration

- [ ] 4.1 Delete the Swift NMP client, receipt-fact translation, account lifecycle, relay configuration, publication resume/dispatch, and shared-library composition; verify application startup cannot initialize an NMP engine or signer.
- [ ] 4.2 Remove NMP from the Tuist project, local package inputs, dependency locks, release-input materialization, and prepared build products; verify a clean project generation and dependency resolution never fetch or link NMP.
- [ ] 4.3 Delete NMP preparation scripts, revision pins, package/XCFramework generation, CI hooks, and NMP-only caches declared by the repository; verify no supported build command invokes them.
- [ ] 4.4 Regenerate Swift and Kotlin bindings from the reduced facade and normalize generated project artifacts; verify binding drift and freshness checks pass with no removed symbols.
- [ ] 4.5 Remove Nostr/NMP settings, labels, fixtures, previews, icons, accessibility actions, and dormant feature flags; verify the built app and automation inventory advertise no removed capability.

## 5. Make Removal a Permanent Architecture Rule

- [ ] 5.1 Update current ownership, architecture, schema, release-readiness, roadmap, and state documents to describe the Nostr-free product; verify no current-state document claims Nostr/NMP support or future activation.
- [ ] 5.2 Mark retained historical ADRs and completed records as superseded without rewriting history, and remove obsolete active conformance entries; verify history is clearly non-normative and excluded from current surface counts.
- [ ] 5.3 Add a zero-tolerance conformance ratchet for active source, APIs, manifests, generated bindings, build scripts, and current documentation, with an explicit narrow allowlist for retirement and historical records; verify seeded forbidden references fail the check.
- [ ] 5.4 Update the active OpenSpec reconciliation artifacts, GitHub issues, milestones, and release evidence to match the removal and WIP dispositions; verify every completion claim links to exact-SHA evidence and deferred work remains open.

## 6. Qualify the Candidate

- [ ] 6.1 Run strict OpenSpec validation and all architecture, ownership, schema, dependency, facade, generated-artifact, and Nostr-absence checks; verify every check passes from the candidate checkout.
- [ ] 6.2 Run the complete locked Rust format, lint, test, audit, deny, and portability gates; verify Apple device/simulator and Android API 23 arm64/x86_64 core builds pass without an NMP dependency.
- [ ] 6.3 Compile and exercise the regenerated Kotlin bindings; verify all retained contract fixtures and runtime smoke tests pass with no publication surface.
- [ ] 6.4 Generate the Apple project with pinned tools and run the complete Apple release-input gate; verify project normalization, package resolution, binding freshness, and build inputs pass from a clean derived state.
- [ ] 6.5 Use XcodeBuildMCP on a disposable supported simulator to build, install, and launch a clean app; verify startup leaves “Opening your library” and reaches the expected usable accessibility tree without Nostr/NMP initialization or relay traffic.
- [ ] 6.6 Use XcodeBuildMCP to execute the complete iOS test suite on the candidate SHA; verify every retained test passes with zero hangs, failures, or skipped required coverage.
- [ ] 6.7 Exercise a migrated-install fixture containing old identity and every publication state; verify effect-free retirement, subsequent clean relaunch, intact non-Nostr data, and no Nostr-related runtime connection or loaded binary.

## 7. Publish and Merge

- [ ] 7.1 Commit the removal, migration, generated artifacts, documentation, and exact-SHA evidence in reviewable units; verify the working tree contains no unintended path and the reconciliation ledger matches the candidate commit graph.
- [ ] 7.2 Push the candidate branch and open or update one pull request targeting `master`; verify all retained work is remote-backed and the PR explains breaking identity/state deletion, migration behavior, WIP dispositions, and proof boundaries.
- [ ] 7.3 Run the full hosted workflow and non-publishing archive against the exact candidate SHA, fix any hosted-only failure with repeated local qualification, and verify every required check is green.
- [ ] 7.4 Merge the green pull request and fetch `master`; verify the tested candidate is an ancestor of the resulting default-branch SHA and required checks remain successful.

## 8. Return to a Clean Master Checkout

- [ ] 8.1 Check out synchronized `master` in the primary repository and verify zero local/remote divergence before destructive cleanup.
- [ ] 8.2 Apply every approved ledger disposition to superseded branches, stashes, auxiliary worktrees, rejected untracked artifacts, and generated debris only after confirming retained work is remote-backed; verify each removed unique item remains recoverable from its recorded archive or ref.
- [ ] 8.3 Sync and archive the completed OpenSpec changes and finalize planning/tracker bookkeeping through the repository's protected delivery path; verify no active change falsely reports unfinished Nostr or reconciliation work.
- [ ] 8.4 Run the final repository audit from `master`; verify clean status, synchronized refs, no stale worktrees or stashes, zero unapplied ledger rows, no active Nostr/NMP support, green local and hosted evidence, and explicit unchanged TestFlight/publishing status.
