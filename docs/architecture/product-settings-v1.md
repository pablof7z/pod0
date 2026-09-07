# Product settings v1

Task 4.1 establishes the dormant Rust settings owner. It does not commit the
legacy-import authority marker or redirect native callers; those are task 4.2
and task 4.4 responsibilities.

## Durable state

`pod0_product_settings` stores one validated, schema-versioned settings
aggregate with both the global core revision and sync-writer version. The v1
value has exact Rust defaults and contains portable product preferences only.
It excludes secret bytes, opaque credential handles, decode-only migration
fields, and Nostr/NMP-adjacent publication configuration.

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
