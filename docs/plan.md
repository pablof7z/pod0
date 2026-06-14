# Plan

Detailed implementation plans live under `docs/plan/` and are linked from this file.

## Active Work

| Work | State | Source |
|---|---|---|
| NMP feature parity hardening and scaffold burn-down. | Active | `docs/plan/nmp-feature-parity.md` |
| Rust-kernel migration v2 (PlaybackState pure renderer → Compat dir deletion). | Active | `docs/plan/migration-v2.md` |
| NIP-F4 publishing/signing completion for owned publishing/discovery. | P0 | `docs/plan/pod0-nostr-publishing.md` |
| Android feature wave 1 (subscribe/search/episode detail) + parity matrix. | Active | `docs/plan/android-parity.md` |
| TUI feature parity foundation and terminal surface expansion. | Active | `docs/plan/tui-parity.md` |
| TUI live agent validation with Ollama Cloud. | Active | `docs/plan/tui-agent-live-validation.md` |
| Shared LLM provider transport and typed task intents. | Active | `docs/plan/shared-llm-task-architecture.md` |
| Optimistic subscribe + async HTTP capability (instant subscribe, off-thread feed hydration). | Active | `docs/plan/optimistic-subscribe-async-http.md` |

## Planning Files

- `docs/BACKLOG.md` - tactical queue, active violations, and follow-up work.
- `WIP.md` - active branches/worktrees only.
- `docs/plan/nmp-feature-parity.md` - canonical feature-parity execution status and scaffold burn-down instructions.
- `docs/plan/migration-v2.md` - ordered milestone plan (M0–M8) for completing the Rust-kernel migration.
- `docs/plan/pod0-nostr-publishing.md` - NIP-F4 protocol correction and publishing/discovery plan.
- `docs/plan/android-parity.md` - Android feature-parity status matrix (Tier 1-4) and the subscribe/search/episode-detail wave.
- `docs/plan/tui-parity.md` - Terminal-client parity matrix and staged implementation plan.
- `docs/plan/tui-agent-live-validation.md` - live tmux scenario inventory and validation log for agentic TUI workflows.
- `docs/plan/shared-llm-task-architecture.md` - provider transport, model routing, and typed task-intent ownership contract across platforms.

## Active State

| Area | Status |
|---|---|
| NMP dependency pin | `0.6.2` crates pinned to rev `ac7e307e89b57a73b419ea9588275e599dcb228c` in `Cargo.toml`; PR #488 replaced the non-portable `/tmp/nmp-at-ac7e307e` path patch with reproducible `vendor/nmp-blossom`, and PR #492 vendors `nmp-core` with the local `publish_ver` fix needed by the headless proof. Local blocker `pablof7z/podcast-player#479` is closed. Upstream `pablof7z/nostr-multi-platform#1408` and `#1412` remain upstream cleanups, but current podcast-player `main` validates the Rust workspace and headless e2e gates. |
| Feature parity | Not achieved — many merged PRs are scaffolds or heuristics, not full original-app behavior. |
| Legacy app deletion | Blocked — `App/Sources/` remains the reference implementation until all parity exits pass. |
| Parked iOS shell | Deleted — there is no `ios/` tree on current `main`; remaining parity debt lives in `App/Sources/` Swift policy/fallback code and the cross-platform surfaces listed in `docs/BACKLOG.md`. |
| NIP-F4 Keychain flip | Cancelled — `podcast-keys.json` is the canonical and final store for per-podcast secrets. No Keychain migration. |
| Validation gate | Incomplete — code parity work must run focused Rust/Swift tests plus the merge gate, and the full simulator suite still needs to become the supervisor gate before declaring parity. The old `nmp-blossom` packaging blocker is resolved on `main`; the `publish_outbox` projection-rev issue is locally unblocked by the vendored `nmp-core` fix in PR #492, with upstream `pablof7z/nostr-multi-platform#1412` still open for canonical cleanup. |

## Next Execution Order

1. Burn down feature-parity scaffolds — replace AI/platform scaffolds with real logic, one PR per backlog item.
2. Burn down remaining Swift policy/fallback code in `App/Sources/` — replace each business decision with Rust-backed snapshot/action behavior.
3. Broaden the iOS validation gate.
4. Re-run full validation and only then mark feature-parity done.
