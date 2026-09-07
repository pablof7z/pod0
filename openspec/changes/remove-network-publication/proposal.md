## Why

Pod0 carried an unrelated network-publication subsystem across Swift, Rust, storage, generated bindings, build tooling, agent permissions, and documentation. It added startup and ownership complexity without serving the product's podcast purpose.

## What Changes

- **BREAKING** Delete the entire network-publication capability and every product action, API, model, store, dependency, build input, test, fixture, and document dedicated to it.
- Delete all obsolete protocol identifiers from repository paths and file contents. There is no historical, migration, generated-artifact, or compatibility allowlist.
- Add one neutrally named, local Rust schema migration that deletes obsolete publication rows and tables without performing an external effect.
- Preserve unrelated podcast, playback, download, transcript, clip, search, memory, and agent behavior.
- Reconcile every branch, worktree, stash, modified path, and untracked artifact; land retained work, explicitly discard superseded work, merge to `master`, and finish on a clean synchronized checkout.

## Capabilities

### New Capabilities

- `protocol-free-product`: Pod0 has no network-publication subsystem, named protocol residue, dormant fallback, or build dependency.

### Modified Capabilities

None.

## Impact

- Rust: removes publication domain/application/facade/storage behavior and regenerates bindings.
- Swift: removes the client, composition, and build dependency.
- Persistence: advances the Rust schema with an effect-free deletion migration.
- Documentation and tooling: deletes dedicated records and installs a literal zero-reference ratchet.
- Delivery: integrates retained WIP through one qualified candidate and returns the primary checkout to `master`.
