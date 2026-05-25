# WIP - Active Work In Flight

> Live tracker for work currently on a branch or agent-owned worktree.
> Add an entry when work starts; remove it after the PR merges.

Related files:

- `docs/plan.md` - overarching plan, active milestones, and current implementation focus.
- `docs/BACKLOG.md` - tactical queue, active violations, pending decisions, and follow-up work.

## Active

- 2026-05-25 - PR 1: subscribe action → kernel dispatch → library snapshot — branch `pr-1-subscribe-library`, opening PR now.
- 2026-05-25 - PR 2: player actions (Play/Pause/Seek/Speed/SleepTimer/Stop) + MiniPlayerView — branch `pr-2-player-actions`.

## Recent History

- 2026-05-25 - M13.A: Android shell — full snapshot types + action dispatch + audio stub — merged PR #28.
- 2026-05-25 - M13.C+D: Android Compose UI screens — Home, Library, Player, Identity, Settings + bottom-tab navigation — merged PR #27.
- 2026-05-25 - M13 iOS: wire Library + Identity tabs into RootShell; compat stubs for all Library views; shared cargo target dir library search path fix — committed d943f82.

- 2026-05-25 - M0.A: nmp-app-podcast Rust crate skeleton — merged PR #4.
- 2026-05-25 - M0.B: ios/Podcast xcodegen skeleton + bridge + model — merged PR #6.
- 2026-05-25 - M0.C: ios/Podcast capabilities scaffolding — merged PR #7.
- 2026-05-25 - M0.D: migration tooling and lint gates — merged PR #3.
- 2026-05-25 - M1.A: identity Rust foundation in nmp-app-podcast — merged PR #10.
- 2026-05-25 - M1.B: Keychain capability with podcast identity namespaces — merged PR #9.
- 2026-05-25 - M1.C: Identity + Onboarding views migrated to ios/Podcast/Features/ — merged PR #5.
- 2026-05-25 - M1.D: Settings Identity/Nostr views migrated to ios/Podcast/Features/ — merged PR #8.
- 2026-05-25 - M1.E: compat stubs, Design/ copies, iOS 26 deployment target — merged PR #11.
- 2026-05-25 - M2.A: podcast-core domain types (17 type modules + projections) — merged PR #12.
- 2026-05-25 - M2.B+C: podcast-feeds — RSS parser, OPML, refresh policy — merged PR #14.
- 2026-05-25 - M2.D: persistence migration — legacy_io capability + Rust from_state_json — merged PR #16.
- 2026-05-25 - M2.E: Library views migrated to ios/Podcast/Features/ — merged PR #13.
- 2026-05-25 - M2.F: Android Compose stub — second-platform proof-of-concept — merged PR #17.
- 2026-05-25 - M3.A: audio capability contract — AudioCommand/AudioReport + PlayerActor — merged PR #15.
- 2026-05-25 - M3.B: iOS audio capability — AVFoundation + MPNowPlayingInfoCenter — merged PR #19.
- 2026-05-25 - M4.A: download capability — DownloadCommand/DownloadReport + queue state machine — merged PR #18.
- 2026-05-25 - M4.B: iOS DownloadCapability — URLSession background downloads — merged PR #22.
- 2026-05-25 - M5: HTTP capability schema + FeedClient request/response bridge — merged PR #23.
- 2026-05-25 - M6.A: podcast-transcripts + podcast-knowledge crates — merged PR #20.
- 2026-05-25 - M7.A: podcast-agent-core — conversation, approval, and task types — merged PR #21.
- 2026-05-25 - M10.A: podcast-discovery NIP-74 parse+build crate (45 tests) — merged PR #24.
- 2026-05-25 - M8+M9: voice capability schema + podcast-briefings crate (148 tests) — merged PR #25.
- 2026-05-25 - M11+M12: platform stubs (WidgetSnapshot, HandoffState, Siri actions, PlatformCapability.swift) + M12 audit — merged PR #26.
- 2026-05-25 - Adopted the NMP-derived coordination protocol for future feature, fix, and refactor work.
