# Foundation Onboarding Wave 001 Evidence Run

Run ID: `pod0-local-20260707T230221Z-fnd-001`

Branch/worktree: `codex/fnd-wave001-evidence` at `/Users/customer/Work/podcast-player-fnd-wave001-evidence`

Source commit: `7098b7d9ec69`

## Skill Grounding

Required search:

```sh
npx skills search "iOS mobile UI UX liquid glass native polish validation"
```

Loaded skill prompts for this evidence pass:

- `vabole/apple-skills@ios-liquid-glass`
- `vabole/apple-skills@hig`
- `qodex-ai/ai-agent-skills@mobile-app-interface`

Applied review facts:

- Liquid Glass belongs on navigation/control chrome and should not obscure or compete with content.
- Onboarding should be brief, focused, and should not expose contradictory app state behind the first-run flow.
- iOS controls need clear semantics, safe-area respect, Dynamic Type support, and accessible touch targets.

## FND-001 Result

Status: `fail`

The visible first onboarding page renders correctly in light mode, with `Pod0`, explanatory copy, feature chips, page dots, and `Get Started`.

The semantic UI tree fails the first-run gate because hidden main-shell controls are exposed as tappable targets behind onboarding:

- `Open sidebar`
- `Settings`
- `Browse categories`
- `Conversations - 182 new`
- `Search`
- `Open Agent`
- `Add Show`

A controlled tap probe on hidden `Settings` returned `SUCCEEDED` but did not navigate. The failure is still actionable because FND-001 requires the UI tree to prove that no main-shell controls are exposed before onboarding completion.

Issue filed: https://github.com/pablof7z/podcast-player/issues/743

## Commands

```sh
npx skills search "iOS mobile UI UX liquid glass native polish validation"
npx skills use vabole/apple-skills@ios-liquid-glass
npx skills use qodex-ai/ai-agent-skills@mobile-app-interface
npx skills use vabole/apple-skills@hig
xcrun simctl create "Pod0 FND Wave001 Codex" com.apple.CoreSimulator.SimDeviceType.iPhone-17 com.apple.CoreSimulator.SimRuntime.iOS-26-2
tuist generate --no-open
cargo build --target aarch64-apple-ios-sim -p nmp-app-podcast
```

XcodeBuildMCP steps:

- `session_show_defaults`
- `discover_projs`
- `list_schemes`
- `list_sims`
- `session_set_defaults` with project `Podcastr.xcodeproj`, scheme `Podcastr`, bundle ID `io.f7z.podcast`, simulator `4C35CC31-67B0-411F-B4DA-A571CF307DFE`
- `erase_sims`
- `build_run_sim` with `extraArgs: ["-skipPackagePluginValidation"]`
- `wait_for_ui`
- `snapshot_ui`
- `screenshot`
- `tap` hidden Settings target `e22` as a post-capture probe

## Notes

The first `build_run_sim` failed because generated Tuist source files were absent in the fresh worktree. `tuist generate --no-open` fixed that prerequisite.

The second `build_run_sim` timed out at the MCP transport layer after 300 seconds, but the app had built, installed, and launched. Because the timeout prevented a reliable launch-to-first-frame timestamp, the required cold-launch metric remains blocked rather than estimated.

FND-002 through FND-008 were not executed in this slice. Continuing would build on a first-run gate that already has a filed defect and would not strengthen FND-001 evidence until #743 is fixed.
