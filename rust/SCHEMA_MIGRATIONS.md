# Rust core schema and recovery contract

`pod0-storage` owns the app-core SQLite schema mechanism and the durable domains
that have completed their Swift-to-Rust cutovers. A domain remains Swift-owned
until its importer, parity check, cutover marker, and obsolete-writer deletion
land together. Full transcript artifacts may be staged and verified in the core,
but Swift transcript JSON remains authoritative until the transcript cutover.

## Version contract

SQLite `application_id` identifies a Pod0 core store and `user_version` records
the global kernel schema. `pod0_schema_versions` records the matching component
version. A typed, caller-supplied ID permanently identifies each store. All
values must agree before a write is allowed.

The first three forward-only versions establish only migration infrastructure:

1. explicit component schema metadata;
2. migration journal and verified-backup evidence;
3. per-domain staged/authoritative cutover markers.

Later versions add complete domain slices rather than generic storage:

4. bounded listening-data import and parity evidence;
5. Rust-authoritative library and subscription state;
6. playback, queue, and resume state;
7. versioned transcript documents and staged/verified evidence generations;
8. durable notes, tombstones, provenance, and staged import state;
9. durable clips with immutable transcript/evidence references; and
10. canonical full transcript artifacts, speakers, words, selections, command
    receipts, and staged two-source legacy import evidence.

Evidence generation writes are transactional. A complete artifact is staged,
reread, and integrity-checked before commit; verification is a separate durable
transition; and only a verified generation can become an episode's selected
generation. Selection moves one pointer atomically while retaining the previous
generation for rollback. Corrupt, incomplete, foreign, or newer-schema evidence
fails closed, and pruning cannot remove the selected generation.

SQL steps are sequential files under `rust/schema/migrations`. Their SHA-256
lock and `CURRENT_SCHEMA_VERSION` are checked in CI. Never edit a shipped step;
add the next version and update the lock in the same reviewed change.

The workspace pins `rusqlite` 0.39.0 with bundled SQLite. Version 0.40.1's
`libsqlite3-sys` release requires the unstable Rust `cfg_select` feature under
the repository's stable Rust 1.93 toolchain. A dependency upgrade therefore
requires stable-toolchain plus Apple and Android target validation first.

## Migration lifecycle

`CoreStoreMigrator` requires an injected clock and caller-supplied stable
migration ID. For an existing store it:

1. opens and validates SQLite without changing the schema;
2. rejects corrupt, foreign, newer, or structurally unexpected stores;
3. creates or reuses a distinct SQLite online backup;
4. reopens each pending step as an immediate transaction;
5. persists a running journal entry when the journal schema exists;
6. commits the SQL step, matching component/global version, verified backup
   evidence, and journal completion atomically.

A crash before commit leaves the old schema and a running journal. Once the
schema version advances, the completed journal and backup evidence are already
durable in that same commit. Failed journal entries block automatic retries
until a newer app supplies an explicit repair; they never trigger a reset.

## Backup and rollback boundary

Backups use SQLite's online backup API rather than copying a live WAL file.
The backup is reopened read-only, must carry the Pod0 application ID and source
schema version and store identity, and must pass `PRAGMA quick_check`. Existing
backup paths are never overwritten; only a matching verified backup from the
same store may be reused after interruption.

Legacy transcript import coordinates the selected artifact row in the Swift
SQLite store with its external JSON payload. Database backups use SQLite's
online backup API. JSON and database backups are written to a same-directory
temporary file, verified against the inspected identities and digests, and
published atomically without clobbering an existing content/generation-qualified
path. Inspection retains bounded identities and digests rather than every full
artifact; staging rehydrates and verifies one artifact at a time. The importer
rechecks both sources immediately before stage and selection commits. A newer
Swift selection can supersede only an earlier import-owned selection while the
cutover remains staged; runtime-owned or authoritative selections fail closed.

Restore is intentionally limited to a new destination path. Tests verify the
restored store before it can replace anything. Before a domain's first
Rust-authoritative write, rollback discards the staged target and keeps or
restores the verified Swift source. After that write, rollback requires the
domain-specific tested export/restore path; re-enabling dual writers is not a
rollback strategy.

## Read-only recovery

Inspection returns `ReadOnlyRecovery` with a stable blocked reason for corrupt,
foreign, newer, or failed-migration state. Migration refuses to mutate that
store. A later native recovery UI may copy/export the original file through a
typed capability, but file paths and SQLite details do not enter normal product
projections.
