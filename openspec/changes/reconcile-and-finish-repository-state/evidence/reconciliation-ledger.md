# Repository Reconciliation Ledger

Snapshot time: `2026-09-07T08:02:15Z` after `git fetch --prune origin`.

Disposition meanings: **retain** enters the reconciliation candidate, **supersede** is already represented by the named destination, **archive** stays outside the candidate with a recovery artifact, and **discard** is generated, machine-local, or rejected work whose recovery artifact is still recorded.

## Recovery Set

- Safety namespace: `refs/backup/reconcile-20260907/` (21 verified refs: every local branch tip, local `master`, fixture branch, detached device-worktree commit, and `stash@{0}`).
- Complete bundle: `.git/reconciliation-backups/2026-09-07/all-local-lineages.bundle`, SHA-256 `020abfa94f2163399b482a7b0605252ee9324016770f90653f7db13f112fd82d`; `git bundle verify` reports a complete history with 21 refs.
- Root tracked patch: `.git/reconciliation-backups/2026-09-07/root-tracked.patch`, SHA-256 `bd0724614d1625dae1db3c695356f0f92f812c45d9074518785b1f955f6c0a40`; `git apply --numstat` parsed all 12 paths.
- Root index patch: `.git/reconciliation-backups/2026-09-07/root-index.patch`, empty-tree SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; the index had no changes.
- Root untracked archive: `.git/reconciliation-backups/2026-09-07/root-untracked.tar.gz`, SHA-256 `853b039f59f537460a327ffae7dcf349517697ada3928ae539fa31bb153d6c46`; `gzip -t` passed and its tar manifest contains all 51 untracked files.
- Device-worktree patch: `.git/reconciliation-backups/2026-09-07/pod0-84-tracked.patch`, SHA-256 `b72c77675499afdc36eea7785d220cc6422ae8c1dd4434c588097f4dae96e78d`; `git apply --numstat` parsed the project-file change.

## Authoritative Lineage and Local-only Commits

`origin/master` and the merge base are `49f64fbd3ddd1ab39f80a69623fef4e97abc3bcd`. Local `master` is `c0d0ba2798ae177980ff7d6d3e41516faade18b2`, with left/right count `0 38`. Disposition: **retain** all 38 commits in order, without rewriting:

```text
3310a427641b5016f05fe5d79b2e66cba3d5b066
80cf934b0721a12285694da1e1c3e66c29c3da9c
8564adcaabac5dee4337198fbfdab364361d2395
e9684ceeccdeb30ebb3f9a68a126c919fd5e07cb
44b904e2e96acfa5dc88d083b10e95ddbe3df98d
6785eec3753d0bba41723821130c4bd4e1d31c56
3c61925fbe87e4f710d47af366495ad128a4727b
bfe2d646a60881332457d4c398194b3b4b63e873
57ca1ccc74ac50ae962dcdaad6ee21b585741c4d
315c6c333f4aa21de142df6b6e0f66e74f187002
394764c8fc9d2525abfda651ad8e20ff032c6c3e
6c59e54c09bce2792122ff18c6bd251a68831a91
ae46f314266b00d61a1dcb5307f1cfae806ab50d
0e52f794a7a405f7a6eaacd23b4ddd09f62401cf
65fc096e953b5d7054689cde821dfeb016f64989
2ab7eefef3a2eb60464b79ed544b3684a50ab976
413d584c92c095eaf4b65ab0d03a7cd2a0848ab3
5fc0d500daa80f2d265094fc76f566a3b24fa1ca
cbd30320a7db0afdfcd9fb1f3a7f4c6dc0cc387f
ffc33c1d728384ff3851909afc12b9598f3150c1
28813c5f491a30d59945171c172818f871520fdc
8edb3c1aba82cfc89a06f959152ac4ce6317f926
a116e51f04127010a742c7a975986c2854435dce
b1ba7b3f18936088be8df7e042fff52da4b47234
52bc2678ac67f12775c0cdfed5174531ecfb907b
b7284b6814f793709cfe756acbbe1636c53722e0
7bc637f869a06fef1e81135c81145b7c12a73c0c
fdb41b9ecf3dc63f209fdd11786af8705d5aa6b3
4a6fefe6357261d7d2f54bfb061e8992253ce03e
1a36e821fae240c0f062bb8c65019967db753466
d20b556ed4c26f9c56448250e5aa4bc4d494f922
90841a35a424a002d161a6e15995d028f0c251d2
50dfa17dc949acbbe63291a9b3585b5972b8d3ba
e960039f2720f925140e98e4330ba5f5cb906a74
e8576390bc7c0dd56d88980b3f68c295a814f456
759bfb690ff9e87534a963cdfa133ead1adb9145
40b7071f5240317da3170cfcbfbec50dcb83cb0a
c0d0ba2798ae177980ff7d6d3e41516faade18b2
```

Other unpublished commits have one disposition each:

| Commit(s) | Disposition | Destination / rationale |
|---|---|---|
| `e285df9939ac8bc8cbac5c09615caad2f34dad5b` | retain | Integrate the four contract-v55 fixtures on the reconciliation branch. |
| `f25ca4c5d5e710d294759b65133262ea0308a4e1` | retain | Decompose into CLI/host behavior plus the shared bootstrap; synthesize with root WIP rather than blind cherry-pick. |
| `a53ec4761167b1d7b4242794595af4aba2aa2b62` | supersede | `git cherry origin/master add-planning-notes` reports patch-equivalence; default history is canonical. |
| `f90d6815e83798fa962eb1c96cf3d125e509a78c`, `b5f4b6d4a48f9eab6cb084e64332c959d509ccf0`, `cfe2841b4980afacc1dc345bd095d0cbb25335aa` | archive | Unique old `remove-whats-new-feature` lineage is outside this change; retained in the all-lineages bundle and its safety ref. |
| `fbee75313a230a918a3272d9aa392075f37d2fb2` | archive | Explicit pre-rebase clip-viewer WIP is outside this change; retained in the bundle/ref. |
| `696752273b205817964a405400f172f5aaa462ef`, `4a6cdc12b4c29c19a5d19e67836257a8949090c5` | supersede | Stashed generated project mutation on base `156607db`; canonical project regeneration in task 4.3 replaces it. Recovery is `refs/backup/reconcile-20260907/stash-0`. |

## Local Branches and Worktrees

| Local branch / worktree | Object | Disposition | Destination / recovery |
|---|---|---|---|
| `master` / root | `c0d0ba2798ae177980ff7d6d3e41516faade18b2` | retain | Parent of the reconciliation branch; safety ref `.../local/master`. |
| `worktree-fix-contract-fixtures` / clean worktree | `f25ca4c5d5e710d294759b65133262ea0308a4e1` | supersede | Retained behavior moves to reconciliation; safety ref and bundle preserve the branch. |
| detached `pod0-84-qual` | `7c83a1e250cf7afb909b82efc431e4712944dc74` | discard | Commit is already `origin/agent/ios-product-proof-cohort`; dirty project edit hard-codes this machine's absolute XCFramework path and is rejected. Patch is archived. |
| `add-planning-notes` | `a53ec4761167b1d7b4242794595af4aba2aa2b62` | supersede | Patch-equivalent merged work; safety ref `.../local/add-planning-notes`. |
| `agent/chapter-scope-admission` | `1d5b5e5285a2cab4d913c75747315761e6e464a3` | supersede | Ancestor of `origin/master`. |
| `agent/hard-cut-private-store` | `9031be17c6e620d57c098e7e8405e06539212f29` | supersede | Ancestor of `origin/master`. |
| `agent/pbxproj-absolute-path-guard` | `8cc3f9cd1ff27c62c38c926353a0c71715331def` | supersede | Ancestor of `origin/master`; exact remote also exists. |
| `agent/per-show-transcript-policy` | `16541241764ce3fdd8c37a9b34f1ed7a1c92e896` | supersede | Ancestor of `origin/master`; remote is 43 commits newer. |
| `agent/share-episode-import` | `4278bb31e9caa7654215f4bfdebe41b9524f38a6` | supersede | Ancestor of `origin/master`; remote is 32 commits newer. |
| `clip-transcript-view` | `bc65665f4e140b86285281f4f116d0c280be9cdf` | supersede | Ancestor of `origin/master`; exact remote exists. |
| `fix-download-host-test-timeout` | `50b9bcf1bc8a9a1ae29000a215812d19ca7475dd` | supersede | Ancestor of `origin/master`; exact remote exists. |
| `remove-whats-new-feature` | `cfe2841b4980afacc1dc345bd095d0cbb25335aa` | archive | Three unique commits preserved in bundle/ref; not selected for the current product candidate. |
| `surface-narrowing` | `8dd74dbbb6b93220d500e142977e5f5671008ecc` | archive | Exact remote branch is the retained recovery destination. |
| `wip-clip-viewer` | `fbee75313a230a918a3272d9aa392075f37d2fb2` | archive | Unique WIP preserved in bundle/ref; outside current change. |
| `worktree-agent-approval-crash-fix` | `94f34a2ccb2720f2545690a9e31723bc9166c13f` | supersede | Ancestor of `origin/master`. |
| `worktree-continue-listening-swipe` | `a609b51da58e820dbb9baa44ce3d7d2702c330fd` | supersede | Ancestor of `origin/master`; exact remote exists. |
| `worktree-fix-share-episode-import` | `183efc048523577d372cce8c66b840c9f46f08c5` | supersede | Ancestor of `origin/master`. |
| `worktree-home-podcast-row-size` | `8bbc58fd78603be9df4365f6612e10e101ec8227` | supersede | Ancestor of `origin/master`; exact remote exists. |

## Root Tracked and Untracked State

The root has 12 tracked paths (`338` insertions, `19` deletions) and 51 untracked files. Every `git status` entry has one row below.

| Status item | Identity | Disposition | Destination / rationale |
|---|---|---|---|
| `.planning/config.json` | in root tracked patch | discard | `_auto_chain_active` is ephemeral runner state; canonical config retains no session flag. |
| `rust/README.md` | in root tracked patch | retain | Headless CLI contract documentation, reconciled with the retained CLI. |
| `rust/crates/pod0-facade/src/facade_exports.rs` | in root tracked patch | retain | Facade surface for retained diagnostics/host behavior. |
| `rust/crates/pod0-facade/src/runtime.rs` | in root tracked patch | retain | Root diagnostics plus `f25ca4c5` create/open behavior are synthesized. |
| `rust/crates/pod0-facade/src/runtime_transcript_effect_leases.rs` | in root tracked patch | retain | Lease selection/cancellation behavior, validated in tasks 2.4-2.5. |
| `rust/crates/pod0-storage/src/effect_outbox.rs` | in root tracked patch | retain | Exact pending/due-time semantics, validated in task 2.4. |
| `rust/crates/pod0-storage/src/effect_outbox_tests.rs` | in root tracked patch | retain | Focused regression coverage. |
| `rust/crates/pod0-storage/src/exports.rs` | in root tracked patch | retain | Exports the retained bootstrap and diagnostics. |
| `rust/crates/pod0-storage/src/lib.rs` | in root tracked patch | retain | Declares the retained bootstrap module. |
| `rust/crates/pod0-storage/src/library_store_activity.rs` | in root tracked patch | retain | Delayed lifecycle wake behavior, validated in task 2.4. |
| `rust/crates/pod0-storage/src/lifecycle_wake_tests.rs` | in root tracked patch | retain | Cancellation regression coverage. |
| `rust/crates/pod0-storage/src/transition_commit_workflow_configuration.rs` | in root tracked patch | retain | Workflow configuration reconciliation. |
| `.agents/skills/` | 7 files, digest `1cad1f7eb50208f8721b2e115460b479f87740878c49d07c5fd56e10da079e79` | retain | OpenSpec project integration. |
| `.claude/commands/` | 6 files, digest `ce5e136b1ef7c1b523184f2883fe8fa89cb6bf036244e1d4e8790a6f9f8d3393` | retain | OpenSpec command integration. |
| `.claude/skills/` | 6 files, digest `c30b9a50325a7c9ba370f6bba157a61aa236d21b464398205974bdb8809ebeda` | retain | OpenSpec skill integration. |
| `.gsd/` | 1 file, digest `e3992d4716038dd3cc17b4a174dae00855821ac98363fb44b578223bc541fed2` | discard | Dispatch isolation sentinel is runtime debris; untracked archive is recovery. |
| `.pi/loops/` | 5 files, included in `.pi` digest `211cb71b60397c5c16b73e588e1cb745865245bb1c0a58667c7058a97fe055bd` | discard | Loop execution state is runtime debris; untracked archive is recovery. |
| `.pi/prompts/`, `.pi/skills/` | 12 files, included in the same `.pi` digest | retain | OpenSpec Pi integration. |
| `.planning/milestone.lock` | digest `c1653260faf238b3d7f05bbdf9019ce7ea1223925693d729021f54b4661070d0` | discard | Ephemeral milestone lock; untracked archive is recovery. |
| `.planning/phases/01-headless-host-crates/01-VERIFICATION.md` | digest `b82e4dcef8622392e4ecedc60dcab05e39039b7a78b0a46d91b3b0a53a695c66` | retain | Historical evidence, revised to distinguish old proof from the current candidate. |
| `audio_000.wav` | PCM mono 24 kHz, SHA-256 `ba9a515a823f7030d3f801a6737e5231cc904adb7297a75b2a750dc161924a27` | archive | Unreferenced sensory/runtime artifact; retained only in the untracked archive, not source control. |
| `openspec/` | 10 files at snapshot, digest `48db8694bbdcbb1731e85175f60806e3ee20c6c05563cf7a0a1cb4654310a98c` | retain | This validated change and OpenSpec configuration. |
| `rust/crates/pod0-storage/src/authoritative_bootstrap.rs` | digest `a5d08709e832c720bb443a4878c0efd3cabe1d5a5b0a9e1d9a7460061569ea5c` | retain | Byte-identical to `f25ca4c5`; one canonical copy is retained and tested. |

## Remote Branches

`origin/HEAD` resolves to `origin/master`. Remote branches already contained by `origin/master` are **superseded** by the default branch: `agent/bdd-harness` `75b99e82701b429c8cf51b9e9a678583e3a311a7`, `agent/binding-layout-identity` `b229bda48177609f2f8e070bf1ad798e2f13c8bc`, `agent/ci-concurrency-group` `3cd9c081921e32b86717087a2a5172c8a9102b98`, `agent/feed-fetch-durable-workflow` `f960019657524e36683fc9f848d513ffc9dea2f8`, `agent/ios-product-proof-cohort` `7c83a1e250cf7afb909b82efc431e4712944dc74`, `agent/ios-product-proof-evidence` `36be739232c55d4c217f4ebc2c816fe79b9731a8`, `agent/note-target-clip` `5db353eabc510b1b6249087443a49cbd94b52fb8`, `agent/pbxproj-absolute-path-guard` `8cc3f9cd1ff27c62c38c926353a0c71715331def`, `agent/per-show-transcript-policy` `9635c49f86bef4f456a909d22785e719ba47dd0c`, `agent/player-fullscreen-no-glass` `04bb644c0dc51f57bc64294c4a6fa330e7c1d7ad`, `agent/player-menu-chapter-proportions` `37bd53297150042d5ef26522b9b46c722204cc54`, `agent/player-view-progress-polish` `d2acb5f4a959146c6bd3fbedebef9be4eb132884`, `agent/podcast-categories` `0df5188d678c5b9f8c9e0db89dc048aa9cc7906f`, `agent/share-episode-import` `3d760867c5dda81f4b5575a0873b39521db33e49`, `agent/speaker-identity` `0594a0ac46ca1a37a6df0be84d9f7b9dd517d4d9`, `clip-transcript-view` `bc65665f4e140b86285281f4f116d0c280be9cdf`, `durable-workflow-epic-19` `6669ac19fa2baddf36c9415a54739a9e5ee69e98`, `feature/sync-up` `49f64fbd3ddd1ab39f80a69623fef4e97abc3bcd`, `fix-download-host-test-timeout` `50b9bcf1bc8a9a1ae29000a215812d19ca7475dd`, `worktree-continue-listening-swipe` `a609b51da58e820dbb9baa44ce3d7d2702c330fd`, `worktree-home-podcast-row-size` `8bbc58fd78603be9df4365f6612e10e101ec8227`, and `worktree-remove-see-all-podcasts` `f702972a9f32c1286753640dc0f54f6c20120986`.

`origin/master` `49f64fbd3ddd1ab39f80a69623fef4e97abc3bcd` is **retain** as the published base. Remote-only/stale tips not contained by default are **archive** in place, with no integration implied: `agent/cache-github-hosted-ci` `62f5b07482811fc09ca67108725d551ae3b267ff`, `agent/clip-notes-exploration-note` `2ca183bbeabfb34cb831c08d48ac46aba1338fed`, `agent/document-github-hosted-ci` `a63c04ff330bd71f443eb3978add87fb49ce4f4e`, `chore/track-nmp-master` `817dd322d85f4a5b2c50e5fed5626404c7e46a6c`, `copilot/upgrade-pod0-nmp-master` `84dbcf04ac5926988325621589679a075deac140`, and `surface-narrowing` `8dd74dbbb6b93220d500e142977e5f5671008ecc`.

## GitHub and Validation State

| Item | Identity / current state | Disposition |
|---|---|---|
| Open pull requests | Empty set after refresh | retain as evidence; a new reconciliation PR is task 8.2. |
| Issue #84 | Open, `M1 - iOS product-proof foundation`, updated `2026-07-26T14:15:58Z` | retain open until the physical audio matrix passes. |
| Issue #142 | Open, `M4 - Durable workflows, agent artifacts, and Nostr coordination`, updated `2026-07-24T00:43:29Z` | retain open until shared Voice-to-Rust authority passes. |
| Latest default-branch CI | Run `31967090316`, failure on `49f64fbd3ddd1ab39f80a69623fef4e97abc3bcd`, created `2026-08-16T19:16:31Z` | retain as blocker evidence; no later default-branch run exists. |
| Latest successful default CI | Run `31648057249`, success on `fb476674d5c7637d153c25ff10f1d99a0ac125f6` | archive as historical evidence only; it does not qualify current work. |

Current baseline evidence is deliberately unresolved: strict Clippy passed in the initial audit, while formatting, five Rust test targets, architecture/version inventory, binding fingerprint, package resolution, simulator, archive, physical-device, and hosted candidate checks remain pending or failing. No TestFlight upload is authorized.

## Reconciliation Branch

Created `agent/reconcile-and-finish-repository-state` at `c0d0ba2798ae177980ff7d6d3e41516faade18b2`. Its merge base with `origin/master` is `49f64fbd3ddd1ab39f80a69623fef4e97abc3bcd`; `origin/master...HEAD` reports `0 38`, and all 38 direct-extension commits remain reachable.

## Reconciliation Corrections

| Item | Disposition | Evidence / recovery |
|---|---|---|
| `rust/crates/pod0-nostr-host` | discard | The isolated crate had no consumer and implemented private-key parsing and BIP-340 signing outside NMP, violating the enforced single-owner boundary. Its complete committed lineage remains recoverable from safety refs `.../local/master` and the verified all-lineages bundle. The workspace member and now-unused shared `nostr` dependency were removed. |

## Nostr-removal Refresh

Snapshot time: `2026-09-07T10:34:35Z` after `git fetch --all --prune`.

- Active checkout: `agent/reconcile-and-finish-repository-state` at `69c7817abe55d2d79690530aebd54270b9cb02f9`.
- Local `master`: `c0d0ba2798ae177980ff7d6d3e41516faade18b2`; active checkout is its direct one-commit extension (`0 1`).
- Published `origin/master`: `49f64fbd3ddd1ab39f80a69623fef4e97abc3bcd`; active checkout is a direct 39-commit extension (`0 39`).
- Refreshed safety namespace: `refs/backup/remove-nostr-20260907/head` plus the 21 earlier `refs/backup/reconcile-20260907/*` refs.
- Refreshed complete bundle: `.git/reconciliation-backups/2026-09-07-nostr-removal/local-lineages.bundle`, SHA-256 `0c2b10ad4535f8a115058a87c35b24d04ffc20e3bbf249bf402f6b5df89aeb90`; `git bundle verify` reports complete history with all local, remote, stash, worktree, and safety refs.
- Refreshed tracked patch: `.git/reconciliation-backups/2026-09-07-nostr-removal/pre-removal-tracked.patch`, SHA-256 `c51c0c533893d067ce4faa616055310f8e107c9ae06357202518e904646818ba`; `git apply --numstat` succeeds.
- Refreshed index patch: `.git/reconciliation-backups/2026-09-07-nostr-removal/pre-removal-index.patch`, empty-tree SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`; the index is clean.
- Refreshed untracked archive: `.git/reconciliation-backups/2026-09-07-nostr-removal/pre-removal-untracked.tar.gz`, SHA-256 `27163c13ad38a2f425ba75d6ffa8fe29f50069b6d34147692c60ba6d9cffecef`; `gzip -t` succeeds and the archive was built from all 69 untracked files.
- Current checkout inventory has 121 status entries. No checkout file was changed by the remote refresh or safety capture except this ledger and the new OpenSpec planning artifacts, all of which are included in the refreshed untracked archive or tracked patch.
