# Recall-index ownership migration runbook

This runbook governs the one-way Swift-to-Rust recall execution-index cutover
implemented by #106. The execution index is disposable; canonical transcript
and evidence state is not.

## Artifacts and owners

- Canonical core store: `Persistence.sharedCoreStoreURL`, owned by Rust.
- Rust execution index: the canonical store path plus
  `.recall-index.sqlite`, owned by `pod0-recall-index`.
- Legacy Swift execution index: `Persistence.legacyRecallIndexURL`
  (`vectors.sqlite` plus optional `-wal` and `-shm`).
- Native provider executors: bounded embeddings and reranking only; no index
  writes, generation selection, retry, ranking, or citation ownership.

Never delete or rewrite the canonical core store while operating this runbook.

## Normal cutover

1. Open the authoritative Rust stores and the separate versioned recall index.
2. Attach the typed native embedding/reranking host.
3. Rebuild every selected active-library evidence generation. Reuse cached
   embeddings whose stable span/generation/text identity still matches; request
   missing vectors in bounded batches.
4. For every selected active-library generation, verify its generation ID,
   declared span count, and exact metadata/vector/lexical execution coverage.
5. Rust issues `RemoveLegacyRecallIndexArtifacts`. The native filesystem
   capability preflights and deletes only regular `vectors.sqlite`,
   `vectors.sqlite-wal`, and `vectors.sqlite-shm` files, then returns
   `LegacyRecallIndexArtifactsRemoved` with a bounded count.
6. Rust accepts the correlated observation and commits the ownership marker.

Steps 4–6 are ordered. If rebuild, cancellation, validation, or storage access
fails, leave the marker untouched and retry on a later launch. An interruption
during deletion can leave only disposable sidecars behind; retry is idempotent,
and an older app can rebuild a missing legacy index from canonical evidence.
The marker is never committed while a usable stale legacy database remains. No
Swift index writer exists in the new app, so this is not dual write.

## Restart and cancellation

- A terminated rebuild restarts from canonical evidence and persisted cached
  embeddings. No in-flight native task becomes authority.
- Cancellation is keyed by the facade cancellation ID. The facade signals the
  SQLite interrupt before waiting for its application-state lock.
- A cancelled replacement transaction commits neither a partial generation nor
  the ownership marker.
- Late or duplicate native observations are rejected by the host-request ledger.

## Corruption and incompatible schemas

- SQLite `CORRUPT` or `NOTADB` on the Rust execution index removes only that
  exact database and its WAL/SHM sidecars, then creates a clean index.
- Canonical evidence and the legacy rollback artifact are untouched.
- A newer/incompatible Rust index schema fails closed. Do not delete it during
  a downgrade; install a compatible build or add a versioned migration.
- A symlink, directory, or other non-regular legacy artifact blocks cutover and
  does not set the marker.
- Failure to resolve the Application Support legacy location blocks cutover;
  native must never substitute a temporary path and acknowledge deletion.

## Rollback

- Before the marker: an older app may use any remaining populated legacy index
  or rebuild missing disposable artifacts after an interrupted deletion.
  Canonical Rust evidence remains authoritative.
- After the marker: an older app sees its disposable index as missing and may
  rebuild it. It must never restore transcript/evidence authority from that file.
- A current app rollback discards/rebuilds only `.recall-index.sqlite`; it does
  not downgrade the canonical core store.

## Qualification commands

```sh
./scripts/check_rust.sh
./scripts/check_core_binding_drift.sh
./scripts/check_kotlin_core_bindings.sh
./scripts/check_core_portability.sh
python3 scripts/check_architecture.py --self-test
```

Also run the complete iOS Debug and Release suites with XcodeBuildMCP, then
build/install/launch the simulator app. Record benchmark and runtime evidence in
`docs/architecture/evidence/` before closing the migration issue.
