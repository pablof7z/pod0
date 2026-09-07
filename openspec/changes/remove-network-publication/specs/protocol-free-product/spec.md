## Purpose

Define Pod0 as a product and repository with no network-publication subsystem or residue from the retired integration.

## ADDED Requirements

### Requirement: The capability is absent

Pod0 SHALL expose no identity, relay, event publication, private-recipient, remote-agent, or receipt behavior from the retired integration through UI, automation, agent tools, native APIs, the shared facade, or generated bindings.

#### Scenario: Product surfaces are enumerated

- **WHEN** shipped UI, automation, commands, and agent tools are enumerated
- **THEN** none offers or advertises the removed capability

#### Scenario: A former action is requested

- **WHEN** a caller seeks a previously supported action
- **THEN** the action is absent rather than routed through a shim or alternate implementation

### Requirement: Runtime and distribution contain no implementation

Pod0 SHALL NOT initialize, build, link, package, download, generate, or execute the retired integration or a generic replacement for it.

#### Scenario: Clean application launch

- **WHEN** Pod0 launches on an erased supported simulator
- **THEN** startup reaches the usable library without initializing the removed subsystem

#### Scenario: Clean project generation

- **WHEN** the product is generated and built with pinned tools
- **THEN** no removed dependency, package, binary, or preparation step is fetched or linked

### Requirement: Repository names contain no residue

The resulting checkout SHALL contain no retired protocol identifier in a file name, directory name, or file content. No historical, migration, fixture, generated, or planning exception is permitted.

#### Scenario: Literal repository scan

- **WHEN** the zero-reference checker scans paths and content
- **THEN** it reports no match

#### Scenario: The checker is self-tested

- **WHEN** seeded forbidden content and path fixtures are checked
- **THEN** each fixture fails for the intended reason

### Requirement: Obsolete Rust state is deleted without effects

The supported Rust database SHALL use one atomic, idempotent migration to delete obsolete publication rows and tables without issuing external effects.

#### Scenario: Old database variants migrate

- **WHEN** empty, completed, pending, leased, retryable, ambiguous, or interrupted fixtures migrate
- **THEN** obsolete state is absent, unrelated state is preserved, and no external operation occurs

### Requirement: No dormant fallback remains

Pod0 SHALL NOT retain feature flags, aliases, deprecated APIs, hidden commands, optional packages, dead production branches, or compatibility adapters capable of restoring the removed behavior.

#### Scenario: Architecture and generated APIs are inspected

- **WHEN** source, manifests, facade schemas, migrations, generated artifacts, and conformance inventories are checked
- **THEN** no removed or generic replacement surface remains

### Requirement: Unrelated behavior remains intact

Removal SHALL preserve local podcast, playback, queue, download, transcript, clip, search, memory, and non-publication agent behavior.

#### Scenario: Qualification runs

- **WHEN** complete Rust, binding, architecture, Apple, simulator, and iOS suites run
- **THEN** retained behavior passes and startup reaches the usable library
