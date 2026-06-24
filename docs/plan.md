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
- `docs/plan/issue-605.md` - Nostr input routing and open_search handler implementation plan.

## Active State

| Area | Status |
|---|---|
| NMP dependency pin | `0.7.2` crates are pinned to rev `9df43816da11b19b73ad98d9ff53bbaeff3b700d` (nmp-v0.7.2 + ADR-0055 Rung-1 publish_ver oracle fix, PR #510) in `Cargo.toml` as ordinary git deps. `nmp-blossom` is un-parked upstream — a first-class `[workspace].members` crate of the NMP repo — so the prior `vendor/nmp-blossom` workaround (and its `[patch]` redirect) was deleted and `nmp-blossom` resolves directly via git. `cargo tree` shows exactly one `nmp-core` v0.7.2. |
| Feature parity | Not achieved — many merged PRs are scaffolds or heuristics, not full original-app behavior. |
| Legacy app deletion | Blocked — `App/Sources/` remains the reference implementation until all parity exits pass. |
| Parked iOS shell | Deleted — there is no `ios/` tree on current `main`; remaining parity debt lives in `App/Sources/` Swift policy/fallback code and the cross-platform surfaces listed in `docs/BACKLOG.md`. |
| NIP-F4 Keychain flip | Cancelled — `podcast-keys.json` is the canonical and final store for per-podcast secrets. No Keychain migration. |
| Validation gate | Established for current merge gates — branch protection requires diff hygiene, migration PR-description lint, Rust workspace, Swift bridge codegen drift, Android Kotlin/unit tests, Android cross-compile, headless e2e, and the full iOS simulator `Build and Test` lane. PR #497 fixed the known playback UI blockers with canonical seeded download paths, UI-test lifecycle teardown, off-main-safe Now Playing artwork, and hardened playback reopen/launch-metric flows; PR #504 fixed the SwiftPM package-resolution stalls. The clean main-equivalent evidence is Test workflow run `27509095557` on commit `bde6e7695066ea7e3ae3f37ad01ad44cc1778d90`, where `Build and Test` completed successfully at `2026-06-14T19:43:14Z`; `Build and Test` is now a required branch-protection context. Feature parity is still not achieved; each code slice still needs focused local validation plus the required merge gate. The old `nmp-blossom` packaging blocker is resolved on `main`; the `publish_outbox` projection-rev issue is carried by the upstream-pinned NMP rev from PR #498, not by a local `nmp-core` fork. |

## Next Execution Order

1. Burn down feature-parity scaffolds — replace AI/platform scaffolds with real logic, one PR per backlog item.
2. Burn down remaining Swift policy/fallback code in `App/Sources/` — replace each business decision with Rust-backed snapshot/action behavior.
3. Keep the full required merge gate green while landing each remaining feature-parity slice.
4. Re-run full validation and only then mark feature-parity done.
