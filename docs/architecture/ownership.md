# Cross-platform ownership inventory

[`ownership.json`](ownership.json) defines owners and selectors.
[`ownership-coverage.json`](ownership-coverage.json) defines the production
Swift, Kotlin, and Rust roots plus the exact behavioral manifests and derived
transition, internal-command, projection, and durable-store surfaces. Together
they classify product source and behavior rather than trusting directory labels.

Validate it from the repository root:

```bash
python3 scripts/check_architecture_ownership.py
```

The check fails when a production source or behavioral surface is uncovered or
multiply owned, a behavioral identity is duplicated, a fact resolves to more
than one domain owner, a classification is unsupported, or a migrating owner
lacks both a migration issue and deletion target.

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
- **Rust business authority:** domain-specific Rust state machines, transitions,
  facts, stores, and bounded projections own cross-platform product meaning.
- **Generated binding:** Swift/Kotlin transport output derives from one Rust
  contract and cannot own policy.
The inventory plus the exact exception manifest is a ratchet, not permission
to add temporary native policy. `current_owner`, `target_owner`,
`migration_issues`, and `deletion_target` document removal responsibility.

The native business-logic checker reads every production Swift and Kotlin
source, masks comments and string literals, and rejects native product-policy
declarations, direct durable product-store writes, semantic fact construction,
and direct external-effect dispatch outside the exact shrinking exception set.
Generated bindings and sources already assigned to deletion are inspected for
inventory coverage but cannot become new exception paths.

## Migration priority

1. Listening identity/state and playback policy: #78–#83.
2. Transcript knowledge, evidence provenance, notes, and clips: #59, #69,
   #92–#97. The version-12 transcript command/projection path and Rust store are
   authoritative after #97; remaining work migrates derived knowledge policy,
   not transcript selection back to Swift.
3. Download intent and recovery: #115–#119; scheduled-agent workflow and
   artifact ownership: #125–#130. Rust owns active agent conversations,
   memories, scheduled state, model usage, and generated audio provenance.
   Residual migration readers are development-only cleanup debt with no
   release-based retention.
4. Native UI and platform capabilities remain native and converge on typed
   host/projection boundaries as their domains migrate.

The live file counts printed by the checker are the authoritative inventory
metrics; do not copy them into static documentation that will drift.
