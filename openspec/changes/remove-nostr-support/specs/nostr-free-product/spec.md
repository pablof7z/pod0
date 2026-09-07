## Purpose

Define and preserve Pod0 as a product with no Nostr or NMP capability, dependency, persisted authority, network behavior, or user-facing workflow.

## ADDED Requirements

### Requirement: Product exposes no Nostr capability
Pod0 SHALL NOT expose Nostr identity, account, relay, event, publication, private-recipient, remote-agent, or receipt behavior through its UI, automation, agent tools, native APIs, shared facade, or generated bindings.

#### Scenario: User and agent surfaces are inspected
- **WHEN** the shipped application, registered automation, and available agent tools are enumerated
- **THEN** no surface offers or advertises a Nostr- or NMP-backed action

#### Scenario: Unsupported historical action is presented
- **WHEN** a caller attempts to invoke a previously supported Nostr publication action
- **THEN** the action is absent rather than routed through a compatibility shim or alternate implementation

### Requirement: Runtime performs no Nostr work
Pod0 SHALL NOT initialize a Nostr engine, create or select a Nostr account, access Nostr key material, choose or contact Nostr relays, compose or sign Nostr events, submit publications, or observe publication receipts during startup or any product workflow.

#### Scenario: Clean application launch
- **WHEN** Pod0 launches with no prior application data
- **THEN** startup completes without initializing Nostr/NMP state or opening a Nostr relay connection

#### Scenario: Ordinary product use
- **WHEN** a user listens, downloads, transcribes, clips, searches, or uses an agent workflow
- **THEN** no Nostr/NMP component is loaded or contacted

### Requirement: Distribution contains no Nostr dependency
Pod0 SHALL NOT build, link, package, download, generate, or execute Nostr/NMP libraries, bindings, binaries, scripts, configuration, or dependency lock entries.

#### Scenario: Reproducible clean build
- **WHEN** the product is generated and built from a clean checkout using pinned toolchains
- **THEN** the build succeeds without fetching, preparing, compiling, linking, or embedding Nostr/NMP code

#### Scenario: Repository conformance scan
- **WHEN** source, manifests, generated bindings, scripts, build products declared by the repository, and current architecture inventories are scanned
- **THEN** no active Nostr/NMP implementation or dependency surface remains

### Requirement: Existing Nostr state is retired without effects
The first compatible upgrade SHALL deterministically remove Pod0-specific Nostr account checkpoints, keychain material, engine stores, publication obligations, receipt links, retry state, and projections without publishing, retrying, signing, or contacting a relay.

#### Scenario: Upgrade with an unfinished publication
- **WHEN** an existing installation contains a pending, leased, ambiguous, or retryable Nostr publication
- **THEN** migration retires the obligation locally and performs no external effect

#### Scenario: Upgrade with Pod0 Nostr identity material
- **WHEN** an existing installation contains Pod0-specific Nostr engine or keychain state
- **THEN** migration removes that state and subsequent launches cannot recover or use the former identity

#### Scenario: Migration is interrupted
- **WHEN** the process terminates during Nostr-state retirement
- **THEN** the next launch safely resumes or confirms retirement without external effects or restoring Nostr support

### Requirement: Removal has no dormant fallback
Pod0 SHALL NOT retain feature flags, aliases, deprecated APIs, hidden commands, optional packages, dead production branches, test-only production hooks, or compatibility adapters capable of restoring Nostr/NMP behavior.

#### Scenario: Removed names and paths are audited
- **WHEN** current production source, build manifests, facade schemas, migrations, and generated artifacts are inspected
- **THEN** obsolete Nostr/NMP names and executable paths are absent except for explicit historical or migration descriptions that cannot activate behavior

### Requirement: Non-Nostr product behavior remains intact
Removing Nostr/NMP SHALL preserve local podcast, playback, queue, download, transcript, clip, search, memory, and non-publication agent behavior.

#### Scenario: Product regression suite runs
- **WHEN** the complete supported local test and simulator qualification suite runs after removal
- **THEN** all non-Nostr product workflows pass and application startup reaches the usable library interface
