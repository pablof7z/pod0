# Swift ownership inventory

The machine-readable inventory is
[`ownership.json`](ownership.json). It classifies every production Swift file
by behavior and target ownership rather than by assuming a directory is native
or shared.

Validate it from the repository root:

```bash
python3 scripts/check_architecture_ownership.py
```

The check fails when a production Swift file is uncovered, covered by multiple
owners, attached to an unsupported classification, or assigned to a migrating
owner without both a GitHub migration issue and a deletion target.

## Baseline interpretation

- **Shared Rust now:** stable durable/cross-platform behavior whose current
  Swift implementation is migration input, not the long-term owner.
- **Native by design:** presentation or Apple platform capability execution
  that remains Swift permanently.
- **Temporary Swift:** unsettled product-proof behavior isolated behind a
  boundary and linked to mandatory migration/deletion work.
- **Undecided pending investigation:** the owner direction is known, but an
  implementation choice needs named evidence before selection.

The inventory is a ratchet, not a claim that existing shared domains have
already migrated. `current_owner`, `target_owner`, `migration_issues`, and
`deletion_target` make that distinction explicit.

## Migration priority

1. Listening identity/state and playback policy: #78–#83.
2. Transcript knowledge, evidence provenance, notes, and clips: #59, #69,
   #92–#97. The version-12 transcript command/projection path and Rust store are
   authoritative after #97; remaining work migrates derived knowledge policy,
   not transcript selection back to Swift.
3. Download intent and recovery: #115–#119; scheduled-agent workflow and
   artifact ownership: #125–#130; agent conversation and memory authority,
   remaining permissions/artifacts, and Nostr publication: #131–#138 under
   #60. Rust is authoritative for agent memories after the schema-v28 cutover;
   Swift retains only bounded presentation projections and temporary rollback
   compatibility.
4. Native UI and platform capabilities remain native and converge on typed
   host/projection boundaries as their domains migrate.

The live file counts printed by the checker are the authoritative inventory
metrics; do not copy them into static documentation that will drift.
