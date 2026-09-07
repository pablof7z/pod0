> **Superseded for delivery on 2026-09-07 by `remove-nostr-support`.** Checked
> repository/bootstrap/quality work remains retained input. Task 4.2's NMP
> package/binding result is historical and will be deleted. Remaining 2.1,
> 3.1, and 4.5-4.7 proof obligations move into the removal change. Unstarted
> Voice tasks 5.1-5.7 remain deferred to issue #142; audio/device tasks 6.1-6.5
> remain deferred to issue #84; Siri tasks 7.1-7.5 remain deferred behind both
> issues. Delivery tasks 8.1-8.5 are superseded by removal tasks 5.4 and 7.1-8.4.
> Unchecked boxes below are intentionally not completion claims; this change is
> archived as superseded only after the replacement lands.

## 1. Freeze and Classify Repository State

- [x] 1.1 Refresh remote references and write a reconciliation ledger covering the default branch, all local branches and commits, root tracked/untracked changes, every stash, every linked worktree, pull requests, issues, and CI runs; verify every `git status`, `git branch`, `git stash`, `git worktree`, and GitHub item has exactly one ledger entry with an object ID or digest.
- [x] 1.2 Create durable safety refs and patch/bundle archives for every unique authored WIP lineage before modifying it; verify every recovery ref resolves and every archived patch or bundle passes an integrity check.
- [x] 1.3 Compare the dirty root facade/storage work, `e285df99`, `f25ca4c5`, the detached `pod0-84-qual` project change, `stash@{0}`, `audio_000.wav`, and `.gsd`/`.pi`/planning metadata; assign retain, supersede, archive, or discard in the ledger and verify no inventoried item lacks a disposition or retained destination.
- [x] 1.4 Create the dedicated reconciliation branch from the local direct-extension tip and record its merge base with `origin/master`; verify the 38 local commits remain reachable without a history rewrite.

## 2. Reconcile Contract, Bootstrap, and Host WIP

- [ ] 2.1 Integrate the facade-contract v55 fixture correction and update every current architecture/version claim; verify all cross-language contract fixture tests and `scripts/check_architecture_docs.py` pass with one version value.
- [x] 2.2 Compare the two `authoritative_bootstrap` implementations and retain one transactional, restart-safe store-creation path with all required authorities initialized; verify focused storage and facade bootstrap/reopen tests pass from a fresh database.
- [x] 2.3 Reconcile the auxiliary CLI store, host-loop, agent payload, and capability-dispatch changes with the retained facade API; verify headless create/open, tool-schema, approval, capability, and provider round-trip tests pass without test-only production behavior.
- [x] 2.4 Reconcile pending-effect diagnostics, headless lease selection, delayed lifecycle wakes, and workflow-configuration changes; verify pending reads do not claim work, due-time reporting is exact, cancellation prevents later claims, lease fencing survives restart, and the targeted storage tests pass.
- [x] 2.5 Fix the reproducible facade chapter-cancellation and user-data-erasure regressions at their owning boundaries; verify both previously failing exact tests pass and late work or residual product projection files cannot survive the terminal action.
- [x] 2.6 Resolve all seven failing BDD scenarios against the authoritative feed, cancellation, notification, library-title, and durability contracts; verify all eight BDD scenarios and the current 62-step catalog pass without weakening expected user behavior.
- [x] 2.7 Remove obsolete bootstrap-related ignores, temporary compatibility paths, duplicate exports, and superseded WIP only after their replacement tests pass; verify remaining ignored tests are limited to named credential, hardware, or manual-sensory requirements with current reasons.

## 3. Restore Repository Policy and Rust Quality Gates

- [ ] 3.1 Add the retained bootstrap and other new mutation modules to architecture/activity ownership inventories and reconcile `.planning/STATE.md`, the Phase 1 verification, and roadmap status; verify the complete architecture script passes and all planning files agree on the current phase and SHA.
- [x] 3.2 Split every newly introduced Rust source file at or above the 300-line soft limit along ownership-aligned seams without creating generic helpers; verify `scripts/check_file_lengths.py` passes and no source file exceeds the 500-line hard limit.
- [x] 3.3 Apply pinned Rust formatting to the 22 drifted files and review the result for semantic changes; verify `cargo fmt --all --check` and `git diff --check` pass.
- [x] 3.4 Make the secret/source-boundary checker safely scope or classify binary artifacts such as WAV input while retaining source/config coverage; verify positive checks pass with the WAV present and negative secret fixtures still fail.
- [x] 3.5 Diagnose and remove the full-suite-only TCP download flake without increasing timeouts blindly; verify the isolated suite and at least two consecutive complete no-fail-fast workspace test runs pass.
- [x] 3.6 Run the pinned full Rust quality gate including strict Clippy, all workspace tests, dependency policy, facade/schema policy, `cargo-deny`, and `cargo-audit`; verify every command succeeds with the repository-declared tool versions and no unaccounted ignores.

## 4. Restore Generated Artifacts and Apple Baseline

- [x] 4.1 Align the local/bootstrap toolchain with the declared Xcode, Tuist, Rust, cargo-deny, cargo-audit, cargo-ndk, NDK, and API versions; verify the toolchain-only release-input gate reports the exact pinned versions.
- [x] 4.2 Prepare the pinned NMP Swift package and rebuild/regenerate Pod0Core bindings and XCFramework from the reconciled Rust sources; verify binding fingerprints, generated Swift/Kotlin sources, and the core binding drift/freshness checks all agree.
- [x] 4.3 Regenerate the Xcode project and both Swift package locks from canonical inputs, reviewing generated diffs separately; verify package resolution succeeds and project/lock drift checks are clean.
- [x] 4.4 Compile and exercise Kotlin bindings and build the complete Rust core for Apple device/simulator and Android arm64/x86_64; verify the binding smoke and portability scripts pass without opening the Android product phase.
- [ ] 4.5 Use XcodeBuildMCP to build and run the app on a disposable supported simulator and execute the full iOS test suite; verify the app launches, the expected accessibility tree appears, and every automated test passes on the candidate SHA.
- [ ] 4.6 Run process-reconstruction/shared-workflow recovery and create a non-publishing release archive containing app, widget, and share extension; verify recovery passes and archive evidence records hashes and the exact candidate SHA.
- [ ] 4.7 Re-run the complete local mandatory gate from a clean checkout of the candidate commit; verify source status stays clean and every result is attributable to that one commit.

## 5. Complete Shared Voice Agent Authority

- [ ] 5.1 Implement the single native Voice-to-shared-session adapter and production composition, removing the stub/local fallback path; verify production source contains one shared conversation authority and Voice Mode fails closed when it is unavailable.
- [ ] 5.2 Route voice and text availability through the same authoritative active-turn state; verify tests cover voice blocking text, text blocking voice, and independent conversations remaining usable.
- [ ] 5.3 Map streaming content plus completed, blocked, ambiguous, provider-failed, and cancelled stages into truthful voice events; verify each terminal state ends exactly once without a fabricated reply.
- [ ] 5.4 Route barge-in and explicit stop through expected-revision Rust cancellation while stopping local TTS promptly; verify before-dispatch, mid-stream, late-output, and post-side-effect cases, including the defined cancellation latency budget and no duplicate effect.
- [ ] 5.5 Reopen voice on the same durable conversation after process death; verify relaunch tests preserve identity/history and neither repeat model submission nor lose a committed outcome.
- [ ] 5.6 Implement the spoken and visible voice approval presenter with explicit confirm/deny and no ambient auto-approval; verify approve, deny, unavailable-presenter, stale-revision, and process-recovery tests.
- [ ] 5.7 Exercise push-to-talk, ambient mode, text/voice handoff, cancellation, provider failure, and approval end to end on the simulator; verify the full voice integration suite passes with no production `StubVoiceTurnDelegate` reference.

## 6. Complete Native Audio Events and Hardware Qualification

- [ ] 6.1 Define the typed native event surface for interruption begin/end, route loss/change, media-services reset, and foreground/background transitions; verify deterministic unit tests cover event decoding, ordering, deduplication, and cancellation.
- [ ] 6.2 Implement one concrete native audio-session coordinator shared by podcast playback and Voice Mode using an AirPlay-compatible record/playback configuration; verify source and runtime tests reject competing coordinators and `voiceChat` mode.
- [ ] 6.3 Apply audio events to playback and voice state transitions with exactly-once resume/pause behavior and durable progress preservation; verify phone/Siri interruption, route loss, route restoration, backgrounding, and media reset tests.
- [ ] 6.4 Run simulator regression tests for playback, Voice Mode, media controls, and lifecycle transitions; verify no existing listening, queue, download, transcript, or agent behavior regresses.
- [ ] 6.5 Execute wired disconnect, Bluetooth disconnect/reconnect, phone or Siri interruption/resume, background/foreground, AirPlay availability, and lock-screen-control scenarios on a named supported iPhone; verify each result includes device, OS, candidate SHA, timestamp, logs, and screenshot or recording evidence.

## 7. Re-enable Siri and Shortcuts Behind Proven Gates

- [ ] 7.1 Encode the Voice/Siri prerequisite gate so production App Intents remain unadvertised until the shared-authority, simulator, hosted, and physical-device evidence is complete; verify a missing prerequisite keeps routing disabled.
- [ ] 7.2 Implement cold Siri invocation through the durable shared conversation; verify a terminated app launches and creates exactly one correctly identified turn.
- [ ] 7.3 Implement warm Shortcut invocation through the existing process and conversation; verify no second session, duplicated turn, or conflicting active-turn state is created.
- [ ] 7.4 Return bounded truthful failures for unavailable microphone, audio route, provider, conversation, or approval presenter; verify no failed invocation leaves an orphaned durable turn.
- [ ] 7.5 Flip the reachability guard only after the prerequisite evidence is attached and run cold/warm tests on simulator and physical device; verify production Voice/Siri surfaces are advertised and both paths use the same authority.

## 8. Publish, Land, and Finish Cleanup

- [ ] 8.1 Update the OpenSpec change, roadmap, state, Phase 1 evidence, GitHub issues #142 and #84, and milestone counts to match the candidate commit; verify each completion claim links to current exact-SHA evidence and unresolved work remains open.
- [ ] 8.2 Publish the reconciliation branch and open or update one reviewable pull request; verify hosted CI runs the full workflow and non-publishing archive against the exact candidate SHA.
- [ ] 8.3 Resolve hosted-only failures, repeat local qualification for any source change, and merge the green PR; verify the resulting default-branch SHA is reachable from the tested candidate and its required GitHub checks succeed.
- [ ] 8.4 Apply every approved disposition: remove superseded local branches, stashes, auxiliary worktrees, generated debris, and rejected untracked artifacts after confirming retained work is remote-backed; verify the ledger has no unapplied row and each removed unique item remains recoverable from its recorded archive/ref.
- [ ] 8.5 Perform the final repository audit from the default branch; verify clean status, zero local/remote divergence, no stale worktrees or stashes, coherent planning/tracker state, green local and hosted evidence, and explicit separation of simulator, physical-device, archive, and unchanged TestFlight status.
