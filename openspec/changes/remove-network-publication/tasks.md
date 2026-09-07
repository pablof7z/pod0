## 1. Freeze and Reconcile Existing Work

- [x] 1.1 Refresh refs and verify the safety bundle, patch, untracked archive, and backup ref.
- [x] 1.2 Inventory every branch, worktree, stash, dirty path, and untracked artifact.
- [x] 1.3 Assign each unique item a retain, supersede, defer, or discard disposition.
- [x] 1.4 Reconcile the predecessor repository-state change and preserve completed unrelated fixes.
- [x] 1.5 Commit the retained pre-removal baseline in reviewable units.

## 2. Delete the Product Capability

- [x] 2.1 Inventory product, Rust, Swift, persistence, facade, binding, build, test, and documentation surfaces.
- [x] 2.2 Delete user actions, agent tools, permissions, commands, and routing for the capability.
- [x] 2.3 Delete Rust domain, application, transition, projection, facade, and storage behavior.
- [x] 2.4 Delete obsolete tests while preserving unrelated agent, approval, and artifact coverage.
- [x] 2.5 Remove generic aliases or replacement publication APIs that would preserve dormant behavior.

## 3. Delete Obsolete Persisted State

- [x] 3.1 Add one neutrally named atomic Rust migration that deletes obsolete tables and related outbox/journal rows.
- [x] 3.2 Cover empty, completed, pending, leased, retryable, ambiguous, interrupted, and repeated migration cases.
- [x] 3.3 Verify the migration preserves unrelated data and performs no external operation.
- [x] 3.4 Keep native production code free of compatibility or legacy-cleanup adapters.

## 4. Remove Swift and Build Integration

- [x] 4.1 Delete the Swift client, translation, composition, and startup paths.
- [x] 4.2 Remove the local package, dependency locks, release inputs, preparation scripts, and CI hooks.
- [x] 4.3 Regenerate Swift and Kotlin bindings from the reduced facade and advance fixtures/contracts.
- [x] 4.4 Regenerate the Apple project with pinned tools and verify release inputs.
- [x] 4.5 Build and launch on an erased simulator; verify the usable empty-library accessibility tree replaces the stuck opening state.

## 5. Establish Literal Absence

- [x] 5.1 Delete dedicated architecture, planning, product, and wiki records; remove subsystem-specific content from mixed records.
- [x] 5.2 Remove obsolete conformance, ownership, roadmap, and release-readiness entries.
- [x] 5.3 Add a zero-reference path/content checker with no historical, migration, fixture, generated, or planning allowlist.
- [x] 5.4 Seed forbidden content and path fixtures and verify the checker rejects both.
- [x] 5.5 Integrate the retained ownership-inventory commits and re-run the literal absence scrub.

## 6. Qualify the Candidate

- [x] 6.1 Pass strict OpenSpec, architecture, ownership, schema, dependency, facade, generated-artifact, and literal-absence checks.
- [x] 6.2 Pass complete locked Rust format, lint, tests, audit, deny, and portability checks.
- [x] 6.3 Compile and exercise regenerated Kotlin bindings.
- [x] 6.4 Generate Apple inputs and pass the Apple release-input gate.
- [x] 6.5 Build, install, launch, and inspect an erased simulator with XcodeBuildMCP.
- [x] 6.6 Pass the complete iOS test suite with zero required skips.

## 7. Publish and Merge

- [ ] 7.1 Commit removal, migration, generated artifacts, documentation, and evidence in reviewable units.
- [ ] 7.2 Push one candidate branch and open or update one pull request targeting `master`.
- [ ] 7.3 Pass all hosted required checks on the exact candidate SHA.
- [ ] 7.4 Merge the pull request and verify candidate ancestry in `master`.

## 8. Return to Clean Master

- [ ] 8.1 Check out synchronized `master` with zero divergence.
- [ ] 8.2 Apply recorded dispositions to auxiliary branches, worktrees, stash, untracked artifacts, and generated debris after retained work is remote-backed.
- [ ] 8.3 Sync/archive completed OpenSpec changes and finalize reconciliation bookkeeping.
- [ ] 8.4 Run the final audit: clean status, no stale worktrees/stashes, zero forbidden path/content matches, green evidence, and unchanged publishing/TestFlight state.
