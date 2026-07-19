# App-core schema migrations

`pod0-storage` owns the app-core SQLite schema. SQL steps live in
`rust/schema/migrations/` and are applied in numeric order inside transactions.
`PRAGMA user_version` and the `kernel` component row must agree.

Migration files are immutable after release. Each file is covered by
`rust/schema/migrations.lock`; CI recomputes SHA-256 values and rejects drift.
A schema change adds the next numbered file, updates the current supported
version, extends structural validation, and adds forward, interruption,
backup, newer-schema, and cross-language fixture coverage as applicable.

Current versions:

- v1: kernel component and stable store identity.
- v2: migration journal and verified target-store backup evidence.
- v3: per-domain staged/authoritative cutover markers.
- v4: normalized listening import records, podcasts, subscriptions, episodes,
  playback policy, and queue entries.
- v5: authoritative library runtime commands and idempotency records.
- v6: authoritative playback runtime policy and recovery state.
- v7: versioned transcript evidence generations, spans, and selection state.
- v8: staged/authoritative notes, revisioned tombstones, and provenance.
- v9: staged/authoritative clips, exact millisecond bounds, frozen transcript
  text, revisioned tombstones, and immutable selected evidence.
- v10: full canonical transcript artifacts, raw segment overlays, speakers,
  words, optimistic selections, replay receipts, and staged two-source legacy
  import evidence. Existing evidence documents and normalized segment rows
  remain the sole semantic transcript representation.

Facade contract version 12 adds the canonical full-transcript application
command, typed receipt/failures, and bounded runtime projections. Schema v10
persists imported and runtime-observed artifacts without claiming iOS
authority: Swift remains the product writer through #96, and issue #97 performs
the single-writer cutover and deletes the temporary shadow path.

Legacy transcript import defines lossless preservation as exact retention of
the canonical semantic fields after the documented nearest-millisecond
conversion, plus a verified immutable backup of the original JSON bytes. Swift
transcript and segment UUIDs and the distinction between absent and empty word
arrays are legacy serialization details, not durable product identity.
Backups are verified before same-directory no-clobber atomic publication, so a
process death can leave an ignorable temporary file but never a partial final
backup. Inspection retains only bounded identities and digests; staging
rehydrates one artifact at a time and rechecks the selection database and files
immediately before commit. While the cutover remains staged, a newer Swift
selection may supersede only a prior import-owned selection. It cannot overwrite
a Rust runtime-owned selection or an authoritative transcript cutover.

Listening, note, and clip importers read the Swift source without mutation,
verify an online SQLite or copied JSON backup, stage rows in one target
transaction, reconstruct the typed projection, and compare it with the
inspected source before commit. They never dual-write. A different source,
import ID, store identity, newer schema, corrupt row, or mismatched verified
backup fails closed without resetting either store. Schema rollback backups
are version-qualified so a verified backup from an older upgrade cannot be
mistaken for the current migration's evidence. Clip source backups are also
generation/content-qualified. A staged clip cutover revalidates the current
source and staged digest while the Swift persistence writer is locked; changed
source state is discarded from staging and imported again before authority can
move. Legacy orphan targets and pre-kernel display-label speaker references are
preserved exactly, while newly authored clips still require live typed targets.

Rollback before authority is established means discarding the staged target
and retaining the verified Swift source/backup. Rollback after authority is a
separate, explicitly tested export operation; the old Swift writer must not be
silently re-enabled.
