# Product settings v1

Tasks 4.1 and 4.2 establish and activate the Rust settings owner. Task 4.4
replaces the transitional whole-record UI command with narrower typed intents
and bounded settings projections.

## Durable state

`pod0_product_settings` stores one validated, schema-versioned settings
aggregate with both the global core revision and sync-writer version. The v1
value has exact Rust defaults and contains portable product preferences only.
It excludes secret bytes, opaque credential handles, decode-only migration
fields, and retired publication configuration.

Every attempted settings command records a redaction-safe validation result in
`pod0_settings_validation_evidence`. Equal-counter updates from different
writers additionally record both writer versions, both value digests, and the
deterministic winner in `pod0_settings_sync_conflicts`. Raw settings values are
not duplicated in either evidence table.

## Transition rules

- A local command must name the current settings revision. Rust assigns its
  next writer counter and rejects stale optimistic revisions.
- A remote snapshot must name its schema version, monotonically increasing
  writer counter, and stable non-secret writer identity.
- Larger writer counters win. Equal counters from different writers are
  concurrent; the lexicographically larger writer identity wins on every
  device and the conflict is recorded.
- Reusing one exact writer version for different values is invalid.
- Unsupported schemas and invalid values never mutate the current settings,
  but their validation evidence commits atomically with the request receipt.
- An accepted settings change, its `SettingChanged` activity fact, validation
  evidence, optional conflict evidence, and idempotency receipt share one
  transaction.

The sync transport remains a native capability. It may carry versioned
snapshots, but it neither validates nor chooses a winner.

## One-time authority cutover

Bootstrap converts the supported portable fields from legacy `Settings` into
`ProductSettingsValues`. The values, their validation evidence, and the
`product_settings` authority marker commit in one SQLite transaction. A retry
after interruption is safe; a settings row without its marker fails closed;
and an already-authoritative store ignores later legacy input.

After the marker is verified, AppState persistence redacts portable product
values while retaining device-local credential handles and connection
metadata. UI updates commit through the Rust facade before changing the Swift
read model. iCloud carries a schema version, writer counter, stable writer
identity, and portable values; Rust validates and merges every observation
before the canonical projection is mirrored back to iCloud.
