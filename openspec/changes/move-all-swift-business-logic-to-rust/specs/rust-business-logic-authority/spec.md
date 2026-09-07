## Purpose

Establishes one cross-platform authority for every Pod0 product decision so identical inputs produce durable, deterministic, and recoverable outcomes on every native shell.

## ADDED Requirements

### Requirement: Rust is the sole business-logic authority
The system SHALL make every product-state, admission, validation, authorization, sequencing, selection, retry, fallback, cancellation, retention, conflict, and semantic-outcome decision in Rust. Native shells SHALL NOT independently make or persist those decisions.

#### Scenario: Same intent from different platforms
- **WHEN** Swift and Kotlin submit the same intent against the same authoritative revision
- **THEN** the Rust owner returns the same typed disposition and resulting product state

#### Scenario: Native projection is stale
- **WHEN** a native shell submits an intent derived from a stale projection
- **THEN** Rust rejects or reconciles it according to the domain transition without allowing native state to overwrite authoritative state

### Requirement: Every product request has a typed disposition
Each product command SHALL have stable identity, expected-state context where required, idempotent replay behavior, and a typed disposition that distinguishes accepted, rejected, stale, duplicate, not allowed, already complete, no-op, cancelled, failed, and outcome-unknown states as applicable.

#### Scenario: Response is lost after commit
- **WHEN** a caller retries an identical command after the original commit response was lost
- **THEN** Rust returns the original durable disposition without applying the transition twice

#### Scenario: Invalid input is submitted
- **WHEN** an input violates a domain invariant
- **THEN** Rust returns a typed rejection and commits no product mutation or effect authorization

### Requirement: Authoritative mutations and activity are atomic
Every accepted product-state mutation SHALL atomically commit the new state, semantic activity facts, authorized external-effect or internal-command intents, and the idempotency receipt. A mutation SHALL NOT become visible without its required causal facts.

#### Scenario: Storage fails during commit
- **WHEN** storage fails between planning and completion of an accepted transition
- **THEN** readers observe either the complete atomic commit or the prior state, never a partial state/fact/outbox combination

### Requirement: Cross-domain consequences use internal commands
A transition that requires work in another product domain SHALL emit a durable typed internal command carrying correlation and causation identity. Neither native code nor one Rust domain SHALL directly mutate another domain's store.

#### Scenario: Completion triggers download deletion
- **WHEN** playback completion satisfies the configured auto-delete policy
- **THEN** the playback transition emits a durable command to the download owner instead of native code directly removing the download

#### Scenario: Import triggers a download
- **WHEN** a shared episode import commits and policy requires downloading it
- **THEN** the import owner emits a durable command to the download owner and recovery does not duplicate the request

### Requirement: Agent and voice behavior share one authority
Interactive text turns, voice turns, prompts, context selection, model selection, provider calls, proposals, approvals, denials, tools, scheduled runs, generation, cancellation, recovery, usage, and publication SHALL use the same Rust-owned transition and effect model. User approval SHALL be a real correlated observation and SHALL NOT be fabricated or automatically inferred by native code.

#### Scenario: Tool requires approval
- **WHEN** an agent proposes an action requiring one-shot approval
- **THEN** Rust records the exact proposal and native UI reports approve, deny, or dismiss for that proposal before Rust decides whether execution is authorized

#### Scenario: Product-mutating tool executes
- **WHEN** an authorized agent tool changes playback, library, settings, notes, clips, or another product domain
- **THEN** Rust submits a durable internal command to that domain rather than invoking a native product writer

#### Scenario: Voice turn is interrupted
- **WHEN** the user barges in or cancels during a voice turn
- **THEN** Rust records cancellation against the active turn and fences late model, speech, tool, or artifact observations

### Requirement: Transcript meaning is normalized in Rust
Rust SHALL own transcript format qualification, parsing, segment and word normalization, speaker identity, ordering, provenance, provider-phase truth, failure interpretation, artifact validation, and canonical selection. Native providers SHALL expose only bounded raw bytes or timed platform observations plus raw transport metadata.

#### Scenario: Publisher transcript is fetched
- **WHEN** a native capability returns transcript bytes, URL metadata, MIME evidence, and HTTP evidence
- **THEN** Rust detects the format, parses and validates the artifact, assigns stable identities, and decides whether it becomes canonical

#### Scenario: Provider request fails after acceptance
- **WHEN** a transcript provider reports a failure after an external operation was accepted
- **THEN** Rust preserves the accepted phase and derives retry, recovery, cancellation, or outcome-unknown state from recorded evidence

### Requirement: Durable settings, categories, and usage have Rust owners
Rust SHALL own durable preferences, defaults, validation, category membership and overrides, credential-connection metadata, workflow/model configuration, model/provider usage records, retention, and conflict resolution. Secrets SHALL remain in platform security storage and SHALL NOT be persisted by Rust.

#### Scenario: Setting changes on one device
- **WHEN** a supported settings sync transport reports a remote value with version evidence
- **THEN** Rust validates and merges it using the authoritative revision rules before publishing the resulting setting

#### Scenario: Provider usage completes
- **WHEN** a leased provider capability returns token, duration, latency, or cost evidence
- **THEN** Rust records at most one usage entry under the causal operation and applies Rust-owned retention policy

### Requirement: Library, search, playback, and workflow policy are projected by Rust
Rust SHALL own subscription/feed behavior, import identity and deduplication, product search ranking, home/library eligibility and ordering, queue and seek transitions, headphone-action fallback, auto-download/delete policy, scheduled occurrence calculation, workflow configuration, retry eligibility, and allowed actions. Native presentation SHALL render bounded projections and submit typed user intents.

#### Scenario: Continue-listening content is requested
- **WHEN** a native shell requests the continue-listening projection
- **THEN** Rust returns the eligible ordered items and bounds without native code reapplying age or progress thresholds

#### Scenario: Queue replacement and immediate playback are requested
- **WHEN** a caller requests an ordered queue and immediate playback
- **THEN** Rust applies one atomic playback transition rather than relying on a native sequence of select, enqueue, and play commands

#### Scenario: Scheduled task is created
- **WHEN** a user submits a schedule definition
- **THEN** Rust assigns identity, validates the interval, selects or validates the model, calculates the next occurrence, and returns only committed state

### Requirement: Export and publication plans are Rust-authored
Rust SHALL select and validate export/share content, redaction, metadata, provenance, conflict behavior, and publication meaning before authorizing a native file, media, share, signing, or transport capability. Native code SHALL NOT interpret transport facts as Pod0 semantic outcomes.

#### Scenario: User exports all data
- **WHEN** the user requests a portable data export
- **THEN** Rust produces a versioned redacted export plan covering all authoritative domains and authorizes the native file write

#### Scenario: Publication receives mixed relay facts
- **WHEN** publication transport reports mixed sent, acknowledged, rejected, retry, or terminal facts
- **THEN** Rust derives the durable Pod0 publication state without native semantic mapping

### Requirement: Migration preserves one writer
Each domain cutover SHALL use a versioned one-time import or verified replacement procedure that forbids durable dual writes. After authority commits, rollback SHALL NOT silently reactivate Swift product authority.

#### Scenario: Upgrade contains legacy native state
- **WHEN** a supported installation first opens after a domain cutover
- **THEN** Rust imports or rejects the legacy evidence deterministically, records the authority marker, and native persistence stops writing that domain

#### Scenario: Cutover has completed
- **WHEN** migration and rollback fixtures prove the Rust representation
- **THEN** the obsolete native writer, policy path, and compatibility state are removed
