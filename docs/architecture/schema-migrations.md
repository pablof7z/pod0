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

Facade contract version 11 adds the canonical full-transcript artifact and
bounded projections without changing the SQLite schema. It deliberately does
not claim transcript authority or write a second artifact. Issue #95 adds the
next locked schema and staged importer; issue #97 performs the single-writer
cutover and deletes Swift transcript persistence authority.

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
