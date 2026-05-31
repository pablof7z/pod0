# Plan

This is the canonical project plan. Detailed implementation plans live under
`docs/plan/` and are linked from this file.

## Current Focus

| Work | State | Source |
|---|---|---|
| NMP feature parity hardening and scaffold burn-down. | Active | `docs/plan/nmp-feature-parity.md` |
| Rust-kernel migration v2 (PlaybackState pure renderer → Compat dir deletion). | Active | `docs/plan/migration-v2.md` |
| NIP-F4 publishing/signing completion for owned publishing/discovery. | P0 | `docs/plan/pod0-nostr-publishing.md` |
| Android feature wave 1 (subscribe/search/episode detail) + parity matrix. | Active | `docs/plan/android-parity.md` |
| Pod0 app rename. | Done | `docs/BACKLOG.md` |
| Planning/WIP reconciliation after merged PR stack. | Done for current pass | `WIP.md`, `docs/BACKLOG.md` |

## Planning Files

- `docs/plan.md` - overarching plan, milestone status, and active focus.
- `docs/BACKLOG.md` - tactical queue, active violations, and follow-up work.
- `WIP.md` - active branches/worktrees only; it must not duplicate backlog state.
- `docs/plan/` - detailed implementation plans linked from this file.
  - `docs/plan/nmp-feature-parity.md` - canonical feature-parity execution status and scaffold burn-down instructions.
  - `docs/plan/migration-v2.md` - ordered milestone plan (M0–M8) for completing the Rust-kernel migration.
  - `docs/plan/pod0-nostr-publishing.md` - NIP-F4 protocol correction and publishing/discovery plan.
  - `docs/plan/nmp-v0.1.0-upgrade.md` - NMP dependency upgrade from old pin to `nmp-v0.1.0` tag: changelog, breakage analysis, and follow-up items.
  - `docs/plan/nmp-v0.2.0-upgrade.md` - NMP dependency upgrade to `nmp-v0.2.0` (rev `ae7b004`): non-breaking pin bump (C-ABI unchanged), changelog, and not-yet-adopted items.
  - `docs/plan/android-parity.md` - Android feature-parity status matrix (Tier 1-4) and the subscribe/search/episode-detail wave.

## Migration State

| Area | Status | Meaning |
|---|---|---|
| PR stack | Merged | GitHub reported zero open PRs on 2026-05-26; prior WIP open-PR entries were stale. |
| NMP dependency pin | v0.2.0 | The four git-pinned NMP workspace deps (`nmp-app-template`, `nmp-core`, `nmp-ffi`, `nmp-signer-broker`) are pinned to `nmp-v0.2.0` (rev `ae7b004`, workspace version `0.2.0`). Non-breaking bump from v0.1.0 — C-ABI byte-for-byte identical, FlatBuffers pin unchanged. See `docs/plan/nmp-v0.2.0-upgrade.md`. |
| Core NMP shell | Implemented | Subscribe, library, player, refresh, OPML/search, persistence, downloads, settings, queue, chapters, ad segments, and all platform surfaces wired to Rust. iOS is a thin rendering shell. Phases 1–6 landed 2026-05-27. |
| Feature parity | Not achieved | Many merged PRs are scaffolds or heuristics, not full original-app behavior. |
| Legacy app deletion | Blocked | `App/Sources/` remains the reference implementation until all parity exits pass. |
| Compat layer | Active debt | `ios/Podcast/Podcast/Compat/` still contains service/domain/identity/utility shims. |
| NIP-F4 | Publishing + discovery wired end-to-end | PR #89 corrected the builders/parsers to canonical NIP-F4 wire shape; PR #93 added real secp256k1 key derivation. Signed relay publish (kind `10154`/`54`/`10064` via the `nostr_relay` capability), relay-backed discovery (WebSocket primary + `api.nostr.band` HTTP fallback), author claims, Blossom audio upload, and deletion cleanup are all in place. Per-podcast secrets now persist to `<data_dir>/podcast-keys.json` and reload on launch. Remaining: M7 flip of the Rust read path to the Keychain (blocked on PD-019). |
| Validation | Incomplete gate | Docs-only changes require `git diff --check`; code parity work must also run focused Rust/Swift tests plus the merge gate. |

## Pod0 / NIP-F4 Milestones

| Milestone | Exit Criteria | Status |
|---|---|---|
| Pod0 protocol setup | `AGENTS.md`, `WIP.md`, `docs/plan.md`, and `docs/BACKLOG.md` define the NMP-style workflow. | Done |
| Pod0 app rename | User-facing app name reflects Pod0; stable identifiers unchanged. | Done via PR #52 |
| NIP-F4 discovery | Discovery reads kind `10154` shows and kind `54` episodes using canonical NIP-F4 addressing. | Done (relay subscription + HTTP-gateway fallback) |
| NIP-F4 publishing | Publishes signed kind `10154`/`54`/`10064` events with real per-podcast keys. | Done (signed relay publish; per-podcast secrets persisted to `podcast-keys.json`) |
| Feature-parity truth pass | Every feature has `done`, `partial`, `scaffold`, `wrong`, or `blocked` status. | Done in `docs/plan/nmp-feature-parity.md` |

## Next Execution Order

1. Finish NIP-F4 key persistence, signed relay publish, relay-backed discovery, author claims, and deletion cleanup.
2. Broaden the iOS validation gate now that the known focused-test compile blockers have been cleared.
3. Burn down `ios/Podcast/Podcast/Compat/` by replacing each shim with Rust-backed snapshot/action behavior.
4. Replace AI/platform scaffolds with real logic feature by feature, keeping each PR tied to a backlog item.
5. Re-run full validation and only then update the feature-parity status to done.
