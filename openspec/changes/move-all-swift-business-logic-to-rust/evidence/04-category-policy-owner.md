# Category policy owner evidence

Task 4.3 extends the existing Rust category and membership store with durable
category settings and deterministic auto-download override resolution.

## Proven scenarios

- Membership projects from the Rust store with exact category defaults.
- An explicit category auto-download override wins over the subscription
  policy.
- An absent override preserves the subscription default.
- Conflicting overrides choose the newest membership, use stable identity as a
  tie-breaker, and return the losing category identities as conflict evidence.
- A stale category revision is rejected without changing the accepted policy.
- Schema 47 backfills existing category rows and the older-schema recovery
  fixtures migrate successfully.

## Verification commands

- `cargo test -p pod0-domain categories -- --nocapture` (3 passed)
- `cargo test -p pod0-storage category -- --nocapture` (14 passed)
- Four schema 13-16 recovery fixtures rerun exactly after adding schema 47
  teardown coverage (4 passed)
- `cargo test --workspace --no-run`
- `python3 scripts/check_rust_schema_policy.py`
- `scripts/check_swift_core_bindings.sh`
- `scripts/check_kotlin_core_bindings.sh`
- `scripts/check_core_binding_drift.sh`
