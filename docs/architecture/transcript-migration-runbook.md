# Transcript authority migration runbook

This runbook governs the one-way cutover from legacy Swift transcript storage
to the Pod0 Rust core. It is an operational companion to
[ADR-0004](adr/0004-persistence-and-single-writer-migration.md) and the
[schema migration policy](schema-migrations.md).

## Non-negotiable invariants

- Rust is the only durable transcript writer after the cutover marker becomes
  `authoritative`.
- Legacy Swift SQLite and transcript JSON files remain read-only migration
  inputs. They are never updated to mirror Rust state.
- A transcript import commits only after both the source database and every
  available transcript file have verified immutable backups.
- Import inspection, staging, verification, and commit compare the same typed
  plan: source generation, database digest, selection digest, artifact count,
  and selected count.
- Every available historical artifact is retained. Selected rows retain their
  exact selection; orphan artifacts receive hidden retained parents.
- Failure leaves the application unavailable for transcript-dependent work. It
  must never silently resume legacy Swift reads or writes.
- Rollback after authority is an explicit export. It is not permission to
  re-enable the deleted Swift transcript store.

## Startup sequence

`SharedLibraryBootstrap` performs these steps while the Swift persistence and
workflow database migration fence is held:

1. Prepare or migrate the shared store and verify its schema backup.
2. Complete prerequisite listening, note, and clip cutovers.
3. Check the `transcripts` domain cutover marker.
4. If already `authoritative`, open the facade without reading legacy
   transcript state.
5. Otherwise inspect the legacy transcript database and files into a bounded
   `LegacyTranscriptImportPlan`.
6. Reuse a matching active import, or discard a mismatched/corrupt staged
   import before creating a new one.
7. Create or verify immutable database and file backups.
8. Stage normalized artifacts, history, exact selections, and provenance in
   the shared store.
9. Verify staged artifacts and backup digests.
10. Commit selections, episode readiness, collection/listening revisions, and
    the authoritative cutover marker in one transaction.
11. Open `Pod0Facade` and expose only bounded typed projections to iOS.

Bootstrap logs only a payload-free stage and failure code. Transcript text,
provider bodies, source URLs, file paths other than the already-public store
location, and raw decoder/database errors must not be logged.

## Process termination recovery

| State at termination | Next launch behavior |
| --- | --- |
| No import row | Inspect and stage from the current legacy source. |
| `staged` | Reinspect the source, reuse matching backup evidence, verify, and commit. |
| `verified` | Reinspect, reverify immutable backup evidence, and commit exactly once. |
| `corrupt` | Discard staged target rows, preserve legacy source/backups, and restage only from a newly matching plan. |
| `discarded` | Treat as no active import and create a new import identity for the current plan. |
| `committed` / cutover `authoritative` | Open Rust authority; never inspect or import Swift transcript state again. |

Import commands and commit receipts are replay-safe. A repeated commit for the
same import returns the committed report without advancing revisions or
duplicating artifacts.

## Source changes during migration

A changed generation, database digest, selection digest, row set, transcript
file digest, or byte count is `SourceChanged` and fails closed.

- Before verification: discard the staged target rows, inspect the new source,
  and stage against a new plan.
- After verification but before commit: discard the verified staging record;
  do not reuse it for the changed source.
- During the final commit fence: roll back the entire transaction and retain
  the verified source backup for diagnosis.
- After authority: reject the legacy source entirely. New transcript versions
  must arrive through the typed Rust application command.

Never edit or delete legacy inputs to force their digest to match a staged
plan.

## Backup evidence

The migration keeps two distinct kinds of backup:

- Version-qualified shared-core schema backups beside the core store.
- Content-addressed legacy transcript database and artifact backups below the
  persistence-owned transcript backup root.

Final backup paths publish with no-clobber atomic moves. Existing files are
reused only after digest and byte-count verification. Temporary staging files
left by termination are not authority and may be replaced by a later verified
attempt.

Do not remove legacy sources or backups as part of the cutover deployment.
Retention policy is a separate reviewed decision after production validation.

## Rollback procedure

### Before authority

1. Confirm the transcript cutover is not `authoritative`.
2. Preserve the legacy source and all verified backups.
3. Discard the active staged/verified import by its typed import ID.
4. Correct the source or implementation fault.
5. Restart bootstrap and require a full inspect, stage, verify, and commit.

The iOS app remains fail-closed during this procedure.

### After authority

1. Choose an empty, persistence-controlled export root.
2. Call the typed `exportLegacyTranscriptRollback` facade operation.
3. Verify the returned schema version, transcript revision, total artifact
   count, selected count, and bundle path.
4. Reinspect the exported SQLite selection database and transcript directory
   with `inspectLegacyTranscriptSource`.
5. Require the round-trip plan to report the same artifact and selected counts.
6. Repeating export for the same core revision must return
   `reusedExisting=true`; any changed bytes or existing conflicting bundle fail
   closed.

The export contains every canonical transcript version at a unique path and an
exactly reconstructed legacy selection database. It is evidence for recovery,
diagnosis, or a separately reviewed importer. It does not reactivate Swift
authority.

## Release verification

Before shipping a cutover build, require:

- Rust migration, history/orphan, large-fixture, interruption, source-change,
  rollback-export, and schema-upgrade tests.
- iOS restart recovery, damaged-core fail-closed, rollback round-trip, and
  concurrent-writer fencing tests.
- `scripts/check_transcript_single_writer.py` and the complete architecture
  checker.
- Generated Swift/Kotlin binding drift checks and Apple/Android Rust builds.
- Full iOS tests plus simulator launch and transcript UI validation.

If any gate fails, keep Rust authority unavailable and fix the failure. Never
restore the removed Swift writer as a release workaround.
