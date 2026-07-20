# Chapter and ad-span authority migration runbook

This runbook governs the one-way cutover from Swift episode adjuncts and
workflow artifacts to the Pod0 Rust core. It complements
[ADR-0004](adr/0004-persistence-and-single-writer-migration.md), the
[schema migration policy](schema-migrations.md), and the
[iOS playback qualification](../validation/ios-playback-qualification.md).

## Non-negotiable invariants

- Rust is the only durable chapter/ad-span writer after authority activates.
- Swift `Episode.chapters` and `Episode.adSegments` are decode-only migration
  inputs and replaceable bounded presentation projections. They are never
  encoded or written back as authority.
- A verified selected workflow row with a matching immutable file wins. Inline
  episode adjuncts are fallback only when no selected workflow artifact exists.
- Missing, changed, malformed, hash-mismatched, future-schema, conflicting, or
  multiply-selected evidence blocks the cutover. No best-effort winner is used.
- Explicit empty ad spans remain a completed evaluation and are distinct from
  an unevaluated ad-span state.
- Every canonical historical artifact and its provenance is retained. Orphan
  and unsubscribed episodes remain valid evidence even when hidden from the
  visible library.
- Selection import and authority activation commit in the same immediate SQLite
  transaction while the native persistence/workflow migration lock is held.
- Failure renders the app unavailable and suppresses native persistence. It
  never resumes a Swift chapter reader or writer.
- Rollback after authority is a typed, immutable export; it does not reactivate
  the deleted Swift implementation.

## Startup and cutover sequence

`SharedLibraryBootstrap` runs under `withSharedArtifactMigrationLock`:

1. Prepare or migrate the Rust store and verify its version-qualified backup.
2. Complete prerequisite listening, note, clip, and transcript authority.
3. If chapter authority is already active, skip every legacy chapter source.
4. Inspect the episode SQLite database, selected workflow artifact rows, and
   referenced immutable files into a bounded `LegacyChapterImportPlan`.
5. Reject any blocked evidence. Reuse only an active staged/verified import
   whose complete plan still matches; otherwise discard it before restaging.
6. Publish digest-verified, no-clobber backups of the source database and each
   referenced evidence file.
7. Stage canonical history, normalized chapters/ad spans, exact selection,
   source links, summaries, transcript provenance, and raw legacy evidence.
8. Reinspect the source and verify all staged counts and backup digests.
9. Begin an immediate transaction, recheck the source, write selections and
   import state, activate authority, and commit atomically.
10. Open `Pod0Facade`; iOS reads bounded projections. Rust issues recoverable
    publisher HTTP requests and qualifies their raw native observations;
    temporary model/agent adapters submit typed observations only.

The logged diagnostic surface is limited to the bootstrap stage and a stable
failure code. Chapter titles, summaries, provider payloads, URLs, file contents,
credentials, and raw SQLite/decoder errors must not be logged.

## Process-termination recovery

| State at termination | Next launch behavior |
| --- | --- |
| No import | Reinspect the current legacy source and stage it. |
| `staged` | Require the same plan, reuse verified backups, then verify and commit. |
| `verified` | Reinspect and reverify evidence, then commit exactly once. |
| `corrupt` or blocked | Preserve source/backups, discard target staging only when a newly inspected plan is safe, then restage. |
| `discarded` | Treat as no active import and derive a stable identity from the current plan. |
| `imported` with authority inactive | Replay commit for the same import and activate atomically. |
| Authority active | Open Rust authority without consulting legacy chapter data. |
| Publisher GET requested or retry scheduled | Reissue the same persisted request identity, absolute due time, and deadline. |
| Publisher bytes delivered before core observation | Reissue the idempotent GET; accept one fenced observation. |
| Publisher observation accepted but storage commit fails | Retain the bounded observation in process and replay it; after termination reissue the still-durable request. |
| Publisher source removed and later restored | Preserve the source-absent tombstone and advance generation so the old request identity can never be reused. |
| Publisher artifact committed | Reopen the succeeded workflow and selected artifact without another GET. |

Repeated stage, verify, commit, command, and rollback-export calls are
idempotent for the same typed identity and fingerprint. A different payload
under the same identity fails closed.

## Source changes and unavailable core

A changed database identity, generation, row selection, artifact file digest,
byte count, raw evidence digest, or plan count is `SourceChanged` or a typed
blocked projection. Before authority, discard only staged target rows and retry
from a fresh inspection. During final commit, the transaction rolls back and
the import becomes diagnostic evidence. After authority, legacy source changes
are ignored; new chapters arrive only through the Rust application command.

If the Rust store cannot open, `AppStateStore` attaches no services and
suppresses all persistence, workflow jobs, widget reloads, and iCloud pushes.
`RootView` presents the recovery surface instead of legacy-backed product UI.
Repair or replace the app build and relaunch; never edit legacy evidence to
force a plan match.

## Rollback export

Before authority, retain the source and backups, discard the active staged
import by typed import ID, correct the fault, and rerun the full cutover.

After authority:

1. Choose an empty persistence-controlled export root.
2. Call `exportLegacyChapterRollback` with the Rust store and verified backup
   root.
3. Require format version, core schema version, source generation, evidence
   count, artifact count, bundle digest, and bundle path.
4. Reinspect `source.sqlite` using the bundle directory as its artifact root.
5. Require the replay plan to reproduce artifact, selection, and blocked counts.
6. Repeat the export; it must return the same digest/path with
   `reusedExisting=true`. Any tampered or conflicting existing bundle blocks.

The bundle includes the original source database, a replayable selection
database, content-addressed evidence, a typed manifest, and a bundle digest.
It is recovery evidence for a separately reviewed rollback, not a dual-write
mechanism.

## Release verification

Before shipping the cutover, require:

- all Rust chapter import, evidence, recovery, rollback, lifecycle, storage,
  command-replay, projection, navigation, and ad-skip tests;
- `SharedChapterRecoveryTests`, Rust publisher workflow tests, native bounded
  HTTP host tests, projection/action tests, playback host tests, and the
  complete iOS suite;
- `scripts/check_chapter_single_writer.py`,
  `scripts/check_chapter_playback_staging.py`, and the architecture/schema/file
  length/privacy checks;
- generated Swift/Kotlin binding drift checks and Apple/Android Rust builds;
- Debug and Release simulator build/launch/render validation plus the physical
  playback checklist for hardware-only route behavior.

If any gate fails, do not reactivate Swift authority. Fix the shared cutover or
typed native adapter and rerun the complete matrix.
