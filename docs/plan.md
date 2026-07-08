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
| Pod0 BDD scenario expansion and provider replay coverage. | Proposed | `docs/plan/pod0-bdd-scenario-expansion.md` |
| Optimistic subscribe + async HTTP capability (instant subscribe, off-thread feed hydration). | Landed | `docs/plan/optimistic-subscribe-async-http.md` |
| Epic A — migrate onto NMP master (UniFFI facade + ADR-0069). | P0 | `docs/plan/issue-597.md` |

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
- `docs/plan/pod0-bdd-scenario-expansion.md` - target BDD scenario expansion, replay/cassette matrix, chirp/NMP parity inputs, and gh-pages publication changes.
- `docs/plan/issue-605.md` - Nostr input routing and NMP open-search migration plan.
- `docs/plan/issue-597.md` - Epic A: NMP master / UniFFI facade migration target-state and pointers (tactical tracking is in GitHub Issues #597, #680-#688).

## Active State

| Area | Status |
|---|---|
| NMP dependency pin | Pinned to NMP v1.0.0-rc.1 snapshot rev `1fc3e6bea390224cef30e37d2ccaa90615197521` in `Cargo.toml` as ordinary git deps, with every listed `nmp-*` crate using the same rev. This is behind the Chirp-shipped NMP master pin `bc6b42592d7fd61bc6767cac246a24a6b23bf8e3`; #707 tracks the re-pin/publish lifecycle/relay-config persistence work, #708 tracks pollable NIP-05 lookup state, #709 tracks app action/projection codegen drift checks, and #734 tracks D8 sleep/polling paths found by the NMP scanner. The current validation expansion is recorded in `docs/testing/chirp-nmp-validation-pack-2026-07-06.md`. |
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
