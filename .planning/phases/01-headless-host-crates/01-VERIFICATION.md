---
phase: 01-headless-host-crates
verified: 2026-08-23T00:00:00Z
status: passed
score: 5/5 must-haves verified
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 4/5
  gaps_closed:
    - "SC5: pod0-cli::HostExecutor reaches approval and capability-execution parity with CoreAgentHost — headless_turn_completes_after_approved_capability_execution now runs as a normal (non-ignored) test and passes against committed HEAD"
  gaps_remaining: []
  regressions: []
human_verification: []
---

# Phase 1: Headless Host Crates Verification Report

**Phase Goal:** The six new Rust host crates are committed, hardened, and prove the same `pod0-application` state machine iOS runs — headlessly, in CI, without the simulator.
**Verified:** 2026-08-23
**Status:** passed
**Re-verification:** Yes — third pass, after gap-closure plan 01-06 restored SC5's evidence (regressed by 01-04's `create_store` disablement, per pass-2 findings)

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SC1: `cargo build/test/clippy --workspace --all-targets` passes in one CI job covering all six crates | ✓ VERIFIED | Independently reproduced this pass: stashed the 11 protected `pod0-facade`/`pod0-storage` WIP files (`git stash push --include-untracked`), then against committed HEAD alone: `cargo build --workspace --all-targets --all-features --locked` (0 errors), `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` (0 warnings). `scripts/check_rust.sh` (the exact script CI's single "Build and Test" job runs) chains `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` then `cargo test --workspace --all-features --locked` in one job. Stash popped cleanly afterward — tree identical to before. |
| 2 | SC2: Each of the six crates also compiles standalone outside the workspace | ✓ VERIFIED | With the same 11 files stashed: `cargo build -p {pod0-cli,pod0-live-hosts,pod0-nostr-host,pod0-portable-media,pod0-system-hosts,pod0-tts-host} --all-targets --all-features --locked` — all six finish 0 errors individually |
| 3 | SC3: HTTP provider clients share one pooled `reqwest::Client` per process with explicit timeouts | ✓ VERIFIED | `grep -c reqwest::blocking crates/pod0-cli/src/host.rs` = 0; `pod0-live-hosts/src/client.rs` sets `connect_timeout: Duration::from_secs(15)` and applies it via `.connect_timeout(config.connect_timeout)` on client construction; unchanged since pass 2 |
| 4 | SC4 (reworded 2026-08-22): crates actually colocated (`pod0-cli`+`pod0-live-hosts`+`pod0-portable-media`) share one runtime; `pod0-nostr-host`/`pod0-system-hosts`/`pod0-tts-host` are Handle-ready but not wired in (deferred) | ✓ VERIFIED | `pod0-cli::HostExecutor` constructs exactly one `tokio::runtime::Builder::new_multi_thread()` Runtime (`host.rs:51`), the only such construction site in `pod0-cli`; `pod0-system-hosts` and `pod0-tts-host` contain zero `Runtime::new`/`Builder::new_*`/`#[tokio::main]` call sites (grepped both crates' `src/*.rs` — no matches); unchanged since pass 2 |
| 5 | SC5: `pod0-cli::HostExecutor` reaches approval and capability-execution parity with `CoreAgentHost` — headless tests exercise real approvals instead of auto-denying | ✓ VERIFIED (gap closed) | Ran directly, not trusted from SUMMARY: `cargo test -p pod0-cli --test live_agent --all-features --locked -- --exact headless_turn_completes_after_approved_capability_execution` → `test result: ok. 1 passed; 0 failed`. Confirmed no `#[ignore]` on this test (`grep -n ignore crates/pod0-cli/tests/live_agent.rs` = no matches). Read the test body: it stands up two fake HTTP chat-completion turns plus a fake search endpoint, drives `Shell::handle(ask_request(...))` through `HostExecutor`'s unconditional `AgentApprovalDecision::Approve` arm (`host.rs:103-108`) and real `search_podcast_directory` capability execution, and asserts the turn reaches `stage: "completed"` with the tool result genuinely round-tripped back to the model. This is a full presented-approval → approved → capability-executed → completed proof, not a stub. |

**Score:** 5/5 truths verified (0 present-but-behavior-unverified)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `rust/crates/pod0-cli/` | Committed, compiles standalone + in workspace, passes tests | ✓ VERIFIED | Standalone + workspace builds clean against committed HEAD; `cargo test -p pod0-cli --all-features --locked` → 0 failed, 3 ignored (each with a dated, linked, out-of-scope reason) |
| `rust/crates/pod0-live-hosts/` | Committed, compiles standalone + in workspace | ✓ VERIFIED | Unchanged since pass 2 |
| `rust/crates/pod0-nostr-host/` | Committed, compiles standalone + in workspace; Handle-ready | ✓ VERIFIED | `cargo test -p pod0-nostr-host --all-features --locked` → 11 passed, 0 failed, 1 ignored (requires live Nostr relay secrets, unrelated to Phase 1 scope) |
| `rust/crates/pod0-portable-media/` | Committed, compiles standalone + in workspace | ✓ VERIFIED | Unchanged since pass 2 |
| `rust/crates/pod0-system-hosts/` | Committed, compiles standalone + in workspace; Handle-ready | ✓ VERIFIED | Compiles clean; owns no runtime |
| `rust/crates/pod0-tts-host/` | Committed, compiles standalone + in workspace; Handle-ready | ✓ VERIFIED | Compiles clean; owns no runtime |
| `rust/crates/pod0-cli/tests/support/mod.rs` | Store-bootstrap fixture restoring SC5's evidence (01-06) | ✓ VERIFIED | Exists, used by `live_agent.rs`, `host_pump.rs`, `live_feed.rs`, `live_search.rs`, `store_bootstrap_smoke.rs`; smoke test passes |

### Protected WIP Boundary Check (pod0-facade / pod0-storage)

Instruction: confirm the 11 pre-existing dirty/untracked `pod0-facade`/`pod0-storage` files remain untouched by any Phase 1 commit.

- Current dirty/untracked set (`git status --short rust/crates/pod0-facade rust/crates/pod0-storage`): 10 modified + 1 untracked (`authoritative_bootstrap.rs`) — 11 files total, matching the team-lead's briefing.
- Checked `git show --stat --format="" <commit>` for every one of the 20 Phase 1 commits (`5fc0d500` through `40b7071f`, spanning 01-01 through 01-06): **none** touch any of the 11 currently-dirty files. Grep for `pod0-facade|pod0-storage` in each commit's stat output returned zero matches.
- One earlier Phase 1 commit, `0e52f794` ("feat(01-01): join six host crates into the Cargo workspace"), does touch `pod0-facade/src/runtime_playback_host.rs` and six *different* `pod0-storage` files (`chapter_workflow_model.rs`, `feed_fetch_store_model.rs`, `internal_command_store.rs`, `recall_workflow_store.rs`, `speaker_store_write.rs`, `user_data_erasure_tests.rs`) — none overlapping the 11 protected files. Its commit message discloses these as mechanical clippy fixes needed to get the workspace-wide `cargo clippy` gate green for crates that had never been linted together before. Not a boundary violation: different files, disclosed, and required for SC1/HOST-02.
- Stash-then-restore executed cleanly: `git stash push --include-untracked -- rust/crates/pod0-facade rust/crates/pod0-storage`, ran all builds/tests against committed HEAD, `git stash pop` restored the exact same 11 files with no conflicts.

**Conclusion: the protected WIP boundary holds.**

### Two New Out-of-Scope Findings (per 01-06-SUMMARY.md) — Load-Bearing Check

01-06-SUMMARY.md self-reports two new, unfixed findings and re-`#[ignore]`s the two tests that hit them. Checked whether either is load-bearing for any of Phase 1's 5 ROADMAP success criteria or HOST-01..05:

1. **`Pod0Facade::next_leased_host_requests` non-idempotent** (`host_drain.rs::pending_host_diagnostics_do_not_claim_or_mutate_work`, re-ignored) — concerns pending-host-request draining/diagnostics, a `pod0-facade` behavior. None of SC1-SC5 or HOST-01..05 mention diagnostics draining or lease idempotency. Confirmed out of scope: this is a bug in the protected `pod0-facade` WIP crate, not in any of the six Phase 1 host crates' committed code.
2. **`settings.rs::workflow_settings_are_initialized_by_the_user_command_and_reopen`** depends on the concurrent uncommitted `transition_commit_workflow_configuration.rs` fix (re-ignored) — concerns workflow-configuration settings persistence across reopen, a `pod0-storage` behavior. None of SC1-SC5 or HOST-01..05 mention settings/workflow-configuration persistence. Confirmed out of scope: same category as the original SC1/SC2 gap (a test depending on uncommitted `pod0-facade`/`pod0-storage` WIP), correctly re-`#[ignore]`d rather than worked around.

Both re-ignored tests carry a single, dated (2026-08-23), linked reason string. Neither blocks any Phase 1 requirement.

### Anti-Patterns Found

`grep -rn -E "TBD|FIXME|XXX"` across `pod0-cli/tests/support/`, `live_agent.rs`, `host_drain.rs`, `settings.rs` — zero matches. All `#[ignore]` annotations carry dated, linked, non-generic reason strings (verified by reading each one directly in the `cargo test` output above), consistent with pass 2's finding that this repo's `#[ignore]` discipline is honest, not a hidden-failure pattern.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Workspace build against committed HEAD (11 facade/storage files stashed) | `cargo build --workspace --all-targets --all-features --locked` | 0 errors | ✓ PASS |
| Workspace clippy against committed HEAD | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | 0 warnings | ✓ PASS |
| Each of 6 crates standalone | `cargo build -p <crate> --all-targets --all-features --locked` ×6 | 0 errors each | ✓ PASS |
| SC5 proof test, run directly (not trusted from SUMMARY) | `cargo test -p pod0-cli --test live_agent --all-features --locked -- --exact headless_turn_completes_after_approved_capability_execution` | 1 passed, 0 failed | ✓ PASS |
| Full `pod0-cli` test suite | `cargo test -p pod0-cli --all-features --locked` | 0 failed, 3 ignored (all dated/linked, out-of-scope) | ✓ PASS |
| `pod0-nostr-host` test suite | `cargo test -p pod0-nostr-host --all-features --locked` | 11 passed, 0 failed, 1 ignored (requires live relay secrets) | ✓ PASS |
| Stash restore integrity | `git stash pop` after all checks | Identical 11 files restored, no conflicts | ✓ PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|---|---|---|---|---|
| HOST-01 | 01-01, 01-04 | Six crates committed, compile cleanly standalone + in workspace | ✓ SATISFIED | Reproduced against committed HEAD (stash-based), this pass |
| HOST-02 | 01-01, 01-04 | Build/test/lint together in one CI job | ✓ SATISFIED | `scripts/check_rust.sh` (CI's single job) chains clippy + test workspace-wide; reproduced clean |
| HOST-03 | 01-02 | HTTP clients pooled with explicit timeouts | ✓ SATISFIED | Unchanged, re-verified |
| HOST-04 | 01-02, 01-05 | Six crates share exactly one tokio runtime (as reworded) | ✓ SATISFIED | Unchanged, re-verified |
| HOST-05 | 01-03, 01-06 | `HostExecutor` reaches approval/capability parity, exercised by headless tests | ✓ SATISFIED | Gap closed this pass — `headless_turn_completes_after_approved_capability_execution` runs and passes as a normal test |

### Gaps Summary

None. All five ROADMAP success criteria for Phase 1 now hold against committed HEAD, independently reproduced via stash-then-check-then-restore rather than trusted from any SUMMARY.md claim. The regression pass 2 found — SC1/SC2's fix (disabling `create_store`) silently disabling the only test proving SC5 — is closed by 01-06's store-bootstrap fixture, verified directly by running the named test rather than trusting 01-06-SUMMARY's self-report. The protected `pod0-facade`/`pod0-storage` WIP (11 files) remains completely untouched by any of the 20 Phase 1 commits. The two new out-of-scope findings 01-06-SUMMARY self-reported (a `next_leased_host_requests` idempotency bug, a `settings.rs` test dependent on uncommitted WIP) are confirmed genuinely unrelated to any of Phase 1's 5 success criteria or HOST-01..05 — they live in the protected WIP crates and are honestly disclosed via dated/linked `#[ignore]` reasons, not silently dropped or worked around.

**Phase 1 goal achieved. Ready to proceed to Phase 2.**

---

*Verified: 2026-08-23*
*Verifier: Claude (gsd-verifier)*
