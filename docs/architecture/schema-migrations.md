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

The v4 listening importer is deliberately pre-cutover. It reads the Swift
source without mutation, verifies an online SQLite or copied JSON backup,
stages all rows in one target transaction, reconstructs the typed projection,
and compares it with the inspected source before commit. It never marks the
domain authoritative and never dual-writes. A different source, import ID,
store identity, newer schema, corrupt row, or mismatched verified backup fails
closed without resetting either store.

Rollback before authority is established means discarding the staged target
and retaining the verified Swift source/backup. Rollback after authority is a
separate, explicitly tested export operation; the old Swift writer must not be
silently re-enabled.
