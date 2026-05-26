# Plan

This is the canonical project plan. Detailed implementation plans live under
`docs/plan/` and are linked from this file.

## Current Focus

| Work | State | Source |
|---|---|---|
| NMP feature parity hardening and scaffold burn-down. | Active | `docs/plan/nmp-feature-parity.md` |
| NIP-F4 protocol correction for owned publishing/discovery. | P0 | `docs/plan/pod0-nostr-publishing.md` |
| Pod0 app rename. | Done | `docs/BACKLOG.md` |
| Planning/WIP reconciliation after merged PR stack. | Done for current pass | `WIP.md`, `docs/BACKLOG.md` |

## Planning Files

- `docs/plan.md` - overarching plan, milestone status, and active focus.
- `docs/BACKLOG.md` - tactical queue, active violations, and follow-up work.
- `WIP.md` - active branches/worktrees only; it must not duplicate backlog state.
- `docs/plan/` - detailed implementation plans linked from this file.
  - `docs/plan/nmp-feature-parity.md` - canonical feature-parity execution status and scaffold burn-down instructions.
  - `docs/plan/pod0-nostr-publishing.md` - NIP-F4 protocol correction and publishing/discovery plan.

## Migration State

| Area | Status | Meaning |
|---|---|---|
| PR stack | Merged | GitHub reported zero open PRs on 2026-05-26; prior WIP open-PR entries were stale. |
| Core NMP shell | Mostly implemented | Subscribe, library, player, refresh, OPML/search, persistence, downloads, settings, queue, and several platform surfaces exist in the NMP target. |
| Feature parity | Not achieved | Many merged PRs are scaffolds or heuristics, not full original-app behavior. |
| Legacy app deletion | Blocked | `App/Sources/` remains the reference implementation until all parity exits pass. |
| Compat layer | Active debt | `ios/Podcast/Podcast/Compat/` still contains service/domain/identity/utility shims. |
| NIP-F4 | Incorrect/partial | Some current tag builders still emit NIP-74-era `d`, `a`, `summary`, `published_at`, and `imeta` shapes. |
| Validation | Incomplete gate | Docs-only changes require `git diff --check`; code parity work must also run focused Rust/Swift tests plus the merge gate. |

## Pod0 / NIP-F4 Milestones

| Milestone | Exit Criteria | Status |
|---|---|---|
| Pod0 protocol setup | `AGENTS.md`, `WIP.md`, `docs/plan.md`, and `docs/BACKLOG.md` define the NMP-style workflow. | Done |
| Pod0 app rename | User-facing app name reflects Pod0; stable identifiers unchanged. | Done via PR #52 |
| NIP-F4 discovery | Discovery reads kind `10154` shows and kind `54` episodes without NIP-74 addressing assumptions. | Partial |
| NIP-F4 publishing | Publishes signed kind `10154`/`54`/`10064` events with real per-podcast keys. | Scaffolded, not done |
| Feature-parity truth pass | Every feature has `done`, `partial`, `scaffold`, `wrong`, or `blocked` status. | Done in `docs/plan/nmp-feature-parity.md` |

## Next Execution Order

1. Fix NIP-F4 wire correctness before adding more Nostr features.
2. Replace per-podcast key placeholder logic with real persistent secp256k1 keys.
3. Wire signed relay publish and relay-backed episode discovery.
4. Burn down `ios/Podcast/Podcast/Compat/` by replacing each shim with Rust-backed snapshot/action behavior.
5. Replace AI/platform scaffolds with real logic feature by feature, keeping each PR tied to a backlog item.
6. Re-run full validation and only then update the feature-parity status to done.
