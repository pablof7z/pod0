# NMP / Chirp Sync Audit - 2026-07-05

Audit scope: Pod0 NMP dependency pins, generated/runtime bridge integration,
local `../chirp` fixes and patterns, and local NMP checkouts after `git fetch`.
No cassette files or app implementation files were changed.

## Compared Commits

| Repository | Ref inspected | Commit | Notes |
|---|---:|---|---|
| Pod0 | `origin/main` after refresh | `d4fe09b7084db589a8122f403024196d9178548d` | Includes `Stabilize show follow option flow (#716)` on top of the UniFFI migration base. |
| Pod0 NMP pin | `Cargo.toml` | `1fc3e6bea390224cef30e37d2ccaa90615197521` | All `nmp-*` crates use this single rev (`Cargo.toml:47-72`). |
| NMP local checkout | `/Users/customer/Work/nostr-multi-platform` `origin/master` | `bc6b42592d7fd61bc6767cac246a24a6b23bf8e3` | 44 commits ahead of Pod0's pin after fetch. |
| NMP local checkout | `/Users/customer/Work/nostr-multi-platform` local `master` | `3d0e9e06cdad15b16634ac777499b96bfcd9b171` | Behind fetched `origin/master`; remote state is authoritative for this audit. |
| Chirp | `origin/master` | `b8474e71c5ce2d710861f92b7c0d80858d934304` | Shipped master re-pins all NMP crates to `bc6b42592` for relay-config persistence and Marmot freshness. |
| Chirp | local `codex/issue-144-joined-groups-chats` | `e87077f7119d94ac3c33970458a01bdf872456ec` | Local worktree is stale relative to shipped `origin/master`; only remote blobs/logs are used as shipped Chirp evidence. |

## Applicable Fixes / Patterns

- **Publish, relay, and action-lifecycle fixes are applicable.**
  Pod0 is missing NMP commits after `1fc3e6b`: `218ac7d7c` pending-publish
  retargeting, `53f2bbf9f` relay-lifecycle D8 rate limiting, `b3d446a30`
  action-stage D8 rate limiting, `f5664e963` permanently-failed publish
  surfacing, `1908fd895` observed terminal action-lifecycle retention, and
  `7be89305f` emitted action lifecycle for dispatched publishes. Current NMP
  `bc6b42592` also fixes runtime startup so persisted relay configuration is
  loaded in `start_runtime`, not only through the builder path. Chirp master
  re-pinned every NMP crate to that exact commit in `b8474e7`. Pod0 also
  currently discards `nmp.publish` dispatch errors and returns `"queued"`
  unconditionally from `apps/nmp-app-podcast/src/nmp_dispatch.rs:310`.
  Tracked as [#707](https://github.com/pablof7z/podcast-player/issues/707).

- **Chirp's NIP-05 lookup-state fix applies to Add Show / Add Friend.**
  NMP `2e44b64a4` added a pollable `Nip05LookupState`; Chirp adopted it in
  `c579723` to avoid endless or timeout-only NIP-05 lookup UI. Pod0 dispatches
  NIP-05 intent resolution, then waits for the first new resolved profile in a
  generic projection (`App/Sources/Features/Library/AddShowSheet.swift:274`,
  `App/Sources/Features/Settings/Agent/AddFriendSheet.swift:253`,
  `App/Sources/Bridge/AppStateStore+NostrIntent.swift:22`). That is not scoped
  to the requested identifier/session. Tracked as
  [#708](https://github.com/pablof7z/podcast-player/issues/708).

- **Chirp's generated-code backstop pattern applies, but as codegen drift rather
  than the exact same byte-vector bug.**
  Pod0's generated action-builder and read-projection registries declare
  `cargo run -p nmp-codegen ... --check`, but this workspace does not contain an
  `nmp-codegen` package and CI does not run those registry checks. The current
  workflow only runs UniFFI drift and local `swift-codegen` drift. Tracked as
  [#709](https://github.com/pablof7z/podcast-player/issues/709).

## Non-Applicable Or Deferred Fixes

- Chirp `ee6160a` / #162's exact fast FlatBuffers byte-vector fix is not a
  current Pod0 bug. Pod0's checked generated projection reader uses a string
  field (`podcast_json_projection_generated.swift:30-31`) and no
  `FlatbufferVector<UInt8>` accessor was found in `App/Sources/Bridge/Generated`.
  The guard pattern is still useful and is covered by #709's generated-code
  drift/backstop work.

- Chirp NIP-29 group-chat work (NMP `0f7ea7bac`, `488fa7a13`, and Marmot
  welcome/key-package fixes through `944f0997`) is not directly
  applicable to Pod0 today. Pod0 does not depend on `nmp-nip29` or `nmp-marmot`,
  and its user-visible social messaging is still public notes/comments/friends
  rather than private group chat.

- Chirp #156's "Add relay widen survives repeat onAppear" fix is Chirp group
  discovery UI state, not Pod0's app relay editor. Pod0 relay editing dispatches
  canonical role/url actions through the settings kernel path.

- NMP wallet/Cashu/nutzap/mint-discovery commits after `1fc3e6b` are not
  currently applicable; Pod0 does not depend on the wallet or mint-discovery
  crates.

## Concrete Pod0 Gaps

1. **NMP pin is behind current publish/relay/action-lifecycle and relay-config
   persistence fixes.**
   File: `Cargo.toml:47-72`. Issue: [#707](https://github.com/pablof7z/podcast-player/issues/707).

2. **NMP publish dispatch currently hides immediate rejection and has no generic
   terminal-state projection wired to user-visible publish flows.**
   Files: `apps/nmp-app-podcast/src/nmp_dispatch.rs:310`,
   `apps/nmp-app-podcast/src/social_publish_handler.rs:94`,
   `App/Sources/Bridge/ActionResultsRegistry.swift:5`. Issue: [#707](https://github.com/pablof7z/podcast-player/issues/707).

3. **NIP-05 Add Show/Add Friend resolution observes generic profile state instead
   of a kernel-owned per-identifier lookup verdict.**
   Files: `App/Sources/Features/Library/AddShowSheet.swift:274`,
   `App/Sources/Features/Settings/Agent/AddFriendSheet.swift:253`,
   `App/Sources/Bridge/AppStateStore+NostrIntent.swift:22`. Issue:
   [#708](https://github.com/pablof7z/podcast-player/issues/708).

4. **Generated action/projection registry drift checks are declared but not
   runnable or enforced in Pod0 CI.**
   Files: `apps/nmp-app-podcast/action-builders.json:459`,
   `apps/nmp-app-podcast/read-projections.json:235`,
   `.github/workflows/test.yml:199`. Issue:
   [#709](https://github.com/pablof7z/podcast-player/issues/709).

5. **Planning metadata is stale relative to current `Cargo.toml`.**
   `docs/plan.md:37` still describes an older `a543943...` NMP pin while the
   current manifest is at `1fc3e6b...`. This audit records the current compared
   commits; update the plan row when #707/#597 changes active focus or milestone
   state.

## Validation Performed

- Read `/Users/customer/Work/podcast-player/WIP.md` before edits.
- Created isolated worktree `/Users/customer/Work/podcast-player-nmp-chirp-audit`
  on branch `codex/nmp-chirp-sync-audit`.
- Ran `python3 /Users/customer/.codex/skills/nmp-app-architecture/scripts/nmp_architecture_scan.py /Users/customer/Work/podcast-player-nmp-chirp-audit`.
- Ran `git fetch --all --prune` in Pod0, `../chirp`, and local NMP checkouts.
- Refreshed the current remote comparison on 2026-07-05T22:37Z:
  Pod0 `origin/main` = `d4fe09b7`, NMP `origin/master` = `bc6b42592`,
  Chirp `origin/master` = `b8474e7`.
- Verified Chirp `origin/master:Cargo.toml` pins all NMP crates to
  `bc6b42592d7fd61bc6767cac246a24a6b23bf8e3`.
- Counted NMP `origin/master` as 44 commits ahead of Pod0's `1fc3e6b` pin.
- Inspected Pod0 manifests, generated bridge files, runtime dispatch/NIP-05
  seams, CI drift gates, Chirp recent commits, and NMP commits after Pod0's pin.
