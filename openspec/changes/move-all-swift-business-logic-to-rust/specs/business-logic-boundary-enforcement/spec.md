## Purpose

Makes the Rust-only business-logic boundary mechanically complete, regression-resistant, migration-aware, and provable across source inventories, bindings, platforms, and crash windows.

## ADDED Requirements

### Requirement: Every production surface has one behavioral classification
The architecture inventory SHALL classify every production source file and every command, transition, fact, effect, internal command, observation, recovery path, projection, and durable store under exactly one owner. Directory labels alone SHALL NOT be treated as proof that a file contains no business logic.

#### Scenario: New production file is added
- **WHEN** a production Swift, Kotlin, or Rust file is introduced
- **THEN** validation fails until its behavior and owner are classified without overlapping ownership

#### Scenario: Policy is added to a native-classified file
- **WHEN** a native adapter gains provider selection, fallback, retry, product validation, semantic failure mapping, durable mutation, or another prohibited decision
- **THEN** the architecture gate fails even though the file remains labelled native

### Requirement: Native business-logic exceptions reach zero
The final production configuration SHALL contain no Swift or Kotlin business-policy or direct-writer exception. New temporary native policy SHALL be forbidden, and migration-only code SHALL have an exact bounded input role and deletion condition.

#### Scenario: Existing exception is removed
- **WHEN** its Rust owner, migration, bindings, and proof scenarios are complete
- **THEN** the exception row and obsolete native path are deleted in the same ownership cutover

#### Scenario: New exception is proposed
- **WHEN** a change adds a native business-policy exception
- **THEN** CI rejects it rather than increasing or replacing the allowlist

### Requirement: Negative fixtures prove enforcement
The repository SHALL include failing fixtures for unregistered inputs, direct product-store mutation, arbitrary mutation closures, native policy, direct effect dispatch, fabricated semantic activity, wildcard routing, in-memory-only authorization, stale observations, and restored retired writers.

#### Scenario: Direct native provider call is introduced
- **WHEN** a feature or view starts a provider request outside the leased dispatcher
- **THEN** a negative architecture fixture detects the bypass and CI fails

#### Scenario: Semantic activity is fabricated natively
- **WHEN** native code attempts to append or construct canonical product activity
- **THEN** validation fails before the change can merge

### Requirement: Retired policy is deleted rather than shadowed
Once Rust owns a capability, obsolete native parsers, coordinators, stores, defaults, caches, compatibility branches, tests, and aliases SHALL be deleted unless they are explicitly required for a supported one-time migration. Dormant production code SHALL NOT remain as an alternate implementation.

#### Scenario: No production caller remains
- **WHEN** a native policy component has no production caller and Rust already owns its capability
- **THEN** the component and its policy-specific tests are removed instead of being ported or left compiled

#### Scenario: Migration reader is still required
- **WHEN** supported upgrade fixtures still require a native legacy reader
- **THEN** it remains read-only, cannot become authoritative, and is removed as soon as the supported migration window closes

### Requirement: Cross-platform contracts remain in lockstep
Generated Swift and Kotlin bindings SHALL derive from the same Rust contract metadata. Contract fixtures SHALL prove stable identifiers, exact integer units, exhaustive variants, bounded collections, typed failures, and rejection of unsupported values on every supported platform.

#### Scenario: Rust contract changes
- **WHEN** a command, projection, effect, observation, or enum changes
- **THEN** generated Swift and Kotlin bindings and their compile/runtime fixtures are updated and validated together

### Requirement: Crash, replay, and concurrency proof is comprehensive
Every migrated domain SHALL test process termination and fault injection across request admission, transition commit, effect claim, external execution, observation commit, internal-command delivery, projection publication, migration, backup, restore, and erasure. Tests SHALL cover duplicates, stale revisions, stale fences, cancellation, supersession, lease expiry, and ambiguous outcomes.

#### Scenario: Crash occurs at an effect boundary
- **WHEN** the process terminates at any claim, execution, observation, or recovery seam
- **THEN** restart produces one committed outcome or an explicit outcome-unknown state and never fabricates success or duplicates an unsafe effect

### Requirement: Required domain scenarios are covered
Validation SHALL exercise manual, automatic, playback-triggered, agent-triggered, ingestion, scheduled, retry, recovery, cancellation, denial, rejection, and failure paths across AI/voice, library/feed, playback/download, transcripts, settings/categories, user artifacts, usage, export/share, and retained publication behavior.

#### Scenario: Full final gate runs
- **WHEN** the zero-exception cutover is proposed complete
- **THEN** every conformance row has implementation, migration, deletion, Rust tests, binding tests, iOS tests, Android-compatible tests, and reproducible proof evidence

### Requirement: Performance and boundedness remain explicit
The final gate SHALL measure and enforce documented bounds for activity write amplification, projection pagination, startup and recovery time, playback observation volume, provider response sizes, native buffers, and large-history behavior.

#### Scenario: Large history is projected
- **WHEN** a user has data at the supported large-history limits
- **THEN** projections remain bounded and stable without reintroducing native caches as product authority
