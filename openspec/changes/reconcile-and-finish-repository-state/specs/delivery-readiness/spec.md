## Purpose

Defines the reproducible evidence required for a Pod0 change to be called landed, buildable, releasable, or qualified on simulator and physical hardware.

## ADDED Requirements

### Requirement: Pinned toolchain and generated-input consistency
The repository SHALL fail closed unless Xcode, Tuist, Rust, auxiliary quality tools, Swift package locks, generated Xcode inputs, UniFFI bindings, and linked core artifacts match the versions and fingerprints declared by the authoritative commit.

#### Scenario: A local tool or generated artifact drifts
- **WHEN** a required tool version, package lock, generated binding, or linked core fingerprint differs from the authoritative inputs
- **THEN** validation stops with a specific repair instruction and no build or runtime success is claimed

### Requirement: Mandatory local validation on one commit
A candidate SHALL pass formatting, strict linting, all non-manual Rust tests, BDD scenarios, architecture and ownership policies, file-length rules, binding checks, Kotlin smoke tests, Apple and Android core portability, iOS simulator build/tests, restart-recovery qualification, and the non-publishing archive against the same source commit.

#### Scenario: One mandatory check fails
- **WHEN** any mandatory check exits unsuccessfully, is skipped, crashes, or runs against a different source revision
- **THEN** the candidate is not described as locally green, landed, or release-ready

#### Scenario: A test is intentionally ignored
- **WHEN** a test requires credentials, physical hardware, or a manual sensory judgment
- **THEN** the ignore reason is specific and current, and any requirement covered only by that test remains explicitly unverified

### Requirement: Deterministic validation tooling
Repository validation SHALL safely classify relevant text and binary artifacts and SHALL produce a pass or a bounded actionable failure instead of crashing on an unrelated file type.

#### Scenario: A binary artifact exists in the checkout
- **WHEN** a source-boundary or secret-boundary checker encounters a legitimate binary file outside its declared text scope
- **THEN** it excludes or safely classifies that file without weakening checks over source and configuration files

### Requirement: Hosted confirmation of the authoritative commit
The exact commit intended for the default branch SHALL pass the full hosted workflow after publication, including the non-publishing archive, before delivery is called complete.

#### Scenario: Local checks pass but hosted CI has not run
- **WHEN** the commit has only local evidence or hosted CI covers another SHA
- **THEN** the change is reported as locally qualified but not fully landed

### Requirement: Honest runtime and release proof
Simulator, physical-device, archive, and TestFlight states SHALL be reported separately. TestFlight upload SHALL require separate explicit confirmation and SHALL not be performed as part of repository reconciliation.

#### Scenario: Simulator tests pass without device validation
- **WHEN** simulator build and tests succeed but the physical-device matrix has not been executed
- **THEN** simulator qualification is reported without claiming physical-device behavior

#### Scenario: Repository delivery completes
- **WHEN** source, local validation, hosted validation, and non-publishing archive all pass
- **THEN** the exact landed SHA and evidence locations are recorded, while TestFlight remains unchanged unless separately authorized
