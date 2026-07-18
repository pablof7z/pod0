# ADR-0004: Persistence, schemas, and single-writer migration

- Status: Accepted
- Date: 2026-07-18
- Decision owners: Pod0 application architecture
- Related issues: #58, #63, #75, #79, #81, #82, #83

## Context

The current Swift store is SQLite-authoritative. It contains a JSON-encoded
`AppState` metadata blob without episodes, per-episode JSON payload rows,
persistence generation, workflow jobs, schema metadata, and artifact metadata.
Legacy JSON is migration input only. Rust must not share write ownership of
these Codable payloads with Swift.

## Implementation status

The app-core schema mechanism now exists in `pod0-storage`: sequential locked
SQL versions, transactional forward migration, a restart journal, verified
SQLite backups, staged/authoritative domain markers, and read-only blocked
states. Schema v4 adds normalized listening tables and a one-shot importer for
the current Swift SQLite store plus pre-SQLite legacy JSON. Swift, Kotlin, and
Rust tests inspect, stage, and read back the same typed projection. The importer
can only write a `staged` marker; Swift remains the sole writer until the first
vertical-slice cutover.

## Decision

Each migrated domain is stored in a Rust-owned SQLite schema in an app-core
database. Rust is the sole writer after that domain's cutover marker commits.
The platform supplies an application-support location and filesystem
capabilities; it does not define schemas or persistence policy.

Schemas have explicit component versions. Migrations are transactional,
restartable, forward-only within a supported compatibility window, and
non-destructive on unknown/newer/corrupt input. Unsupported state opens in an
honest blocked/read-only recovery mode rather than resetting data.

Time and identifiers used in durable decisions are injected into the Rust
actor. Deterministic command/state/time input must produce semantically
identical events and projections.

## Import and cutover protocol

1. Inspect the current store without mutation and build a typed import plan.
2. Verify and record a backup plus source generation/content evidence.
3. Import into a staged Rust schema using stable existing IDs.
4. Verify counts, IDs, key fields, and projection parity.
5. Optionally shadow-read; never dual-write.
6. Stop the Swift writer for the domain.
7. Atomically commit the Rust cutover marker, then permit Rust writes.
8. Delete obsolete Swift ownership in the vertical-slice cleanup issue.

Before the first Rust-authoritative write, rollback discards the staged target
and keeps/restores the Swift backup. Afterward, rollback requires a separately
tested export/restore path; silently re-enabling the old writer is forbidden.

## Failure behavior

- Import and migration interruption before commit rolls back atomically; retry
  is idempotent and restartable.
- Ambiguous identity, corrupt rows, unsupported schema, and partial staging
  produce typed diagnostic state.
- Migration failure never deletes, truncates, or silently replaces user data.
- A late/stale writer cannot commit over a newer revision.
- Backups are verified before cutover, not assumed from file existence.

## Domain boundaries

The first store migration includes podcast, subscription, episode library,
queue/resume/completion/rate/sleep policy, and relevant preferences. Transcript,
knowledge, workflow, agent, and Nostr state remain in their current owners until
their own complete vertical slices.

## Consequences

- The app temporarily has multiple physical stores but exactly one writer per
  fact.
- Existing Swift data needs versioned import fixtures and a supported upgrade
  window.
- The first vertical slice must delete old mutation paths after cutover.
- Rust storage design cannot depend on Swift Codable implementation details
  beyond the importer.

## Rejected alternatives

- **Rust and Swift write the same mixed schema:** makes ordering and migration
  authority ambiguous.
- **Long-lived dual writes:** cannot prove convergence under termination.
- **Destructive reset on migration failure:** violates user-data safety.
- **Migrate every domain at once:** creates an unbounded rewrite and blocks iOS.
- **Keep durable storage native forever:** forces Android to reproduce schemas
  and workflow behavior.
