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

- **Shared Rust now:** product/business behavior is Rust-owned. Retained Swift
  in this class is presentation, a literal typed capability, or decode-only
  migration input; it is not an alternate policy owner.
- **Native by design:** presentation or Apple platform capability execution
  that remains Swift permanently.
- **Temporary Swift:** a frozen legacy exception recorded exactly in
  `rust-business-logic-exceptions.json`. New files and declarations are
  forbidden; #213–#218 delete or reduce the existing rows and #219 requires
  zero.
- **Undecided pending investigation:** forbidden for production business
  logic. Investigation can own a decision artifact, not a shipping policy.

The inventory plus the exact exception manifest is a ratchet, not permission
to add temporary native policy. `current_owner`, `target_owner`,
`migration_issues`, and `deletion_target` document removal responsibility.

## Migration priority

1. Listening identity/state and playback policy: #78–#83.
2. Transcript knowledge, evidence provenance, notes, and clips: #59, #69,
   #92–#97. The version-12 transcript command/projection path and Rust store are
   authoritative after #97; remaining work migrates derived knowledge policy,
   not transcript selection back to Swift.
3. Download intent and recovery: #115–#119; scheduled-agent workflow and
   artifact ownership: #125–#130. Rust now owns active agent conversations,
   memories, scheduled state, model usage, generated audio provenance, and
   tracked NMP publication after #135/#136. The #131 cohort completed agent,
   Nostr, signer, and migration ownership through #138. Any residual one-shot
   readers are development-only cleanup debt with no release-based retention.
4. Native UI and platform capabilities remain native and converge on typed
   host/projection boundaries as their domains migrate.

The live file counts printed by the checker are the authoritative inventory
metrics; do not copy them into static documentation that will drift.
