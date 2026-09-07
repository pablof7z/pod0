## Purpose

Defines how Pod0 converges local commits, dirty changes, stashes, worktrees, planning records, and remote state into one authoritative and recoverable repository lineage.

## ADDED Requirements

### Requirement: Complete repository-state inventory
Before any WIP is integrated or discarded, the reconciliation SHALL record every local-only commit, tracked modification, untracked artifact, stash, linked worktree, local branch, remote branch, open pull request, and relevant CI state, including exact object identifiers or content digests where available.

#### Scenario: Reconciliation begins with multiple lineages
- **WHEN** the canonical checkout, auxiliary worktrees, stashes, or remotes contain different work
- **THEN** the system produces an inventory that identifies each item and its relationship to the remote default branch before mutating any of them

### Requirement: Evidence-backed WIP disposition
Every inventoried item SHALL receive exactly one disposition: retain, supersede, archive, or discard. The disposition SHALL identify the retained implementation or recovery artifact and SHALL explain how behavior, user data, and authorship are preserved.

#### Scenario: Two worktrees implement overlapping behavior
- **WHEN** two candidate changes modify the same capability
- **THEN** their behavior and tests are compared, one canonical implementation is selected or synthesized, and the losing variant is marked superseded only after its unique behavior is accounted for

#### Scenario: A WIP artifact is discarded
- **WHEN** an item has no retained implementation
- **THEN** it is discarded only after the disposition record proves it is generated, obsolete, duplicate, or intentionally rejected and identifies a recoverable snapshot when the item contains unique authored work

### Requirement: One authoritative repository state
After reconciliation, the default branch SHALL contain every retained change exactly once, SHALL have no unexplained tracked modifications, untracked artifacts, stashes, auxiliary worktrees, or contradictory planning records, and SHALL identify the exact remote commit that represents the authoritative state.

#### Scenario: Reconciliation completes
- **WHEN** every disposition has been applied and validated
- **THEN** repository status, worktree inventory, stash inventory, branch topology, planning state, and remote state all agree on one authoritative commit

### Requirement: Planning and tracker truthfulness
Planning documents, OpenSpec artifacts, GitHub issues, and milestones SHALL distinguish completed, pending, blocked, and unverified work and SHALL not claim completion based only on a historical test run against a different commit.

#### Scenario: Historical verification conflicts with current evidence
- **WHEN** a phase report says complete but the current authoritative commit fails a covered requirement
- **THEN** current planning and tracker state report the failure until the same authoritative commit satisfies the requirement again
