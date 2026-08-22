---
phase: 01-headless-host-crates
plan: 03
subsystem: infra
tags: [pod0-cli, agent-approval, agent-capability, itunes-search, integration-test]

requires:
  - phase: 01-headless-host-crates
    provides: "Single shared multi-thread tokio runtime, one pooled LiveHosts HTTP client, and AgentModelCompleted.proposed_tool_call actually reachable in headless tests (01-02)"
provides:
  - "pod0-cli::HostExecutor answers every PresentAgentApproval proposal with AgentApprovalDecision::Approve, matching native AgentApprovalCoordinator's unconditional .approve"
  - "A new host/capability.rs module executes AgentToolAction::Search{tool: SearchPodcastDirectory, ..} for real via search::search, returning a bounded AgentCapabilityOutcome::Succeeded; every other AgentToolAction variant returns a specific AgentCapabilityOutcome::Failed{safe_detail} naming the unsupported action"
  - "A headless integration test (headless_turn_completes_after_approved_capability_execution) drives one real tool-call-producing turn end to end — model proposes search_podcast_directory, approval is granted, the capability executes a real HTTP search against a local fixture, and the turn reaches stage: completed"
affects: []

actuals:
  tokens: 3163
  tasks: 3
  commits: 2

tech-stack:
  added: []
  patterns:
    - "AgentCapabilityRequest dispatch lives in its own host/capability.rs module (mirrors host/search.rs, host/recall.rs, host/playback.rs's one-module-per-HostRequest-family shape); the catch-all Failed{safe_detail} arm uses '{:?}' on the whole AgentToolAction rather than enumerating ~20 variants by hand"
    - "Durable-core observation bounds (MAX_AGENT_MESSAGE_BYTES) are enforced by dropping whole trailing result entries in a loop until the JSON fits, never truncating the serialized string mid-object — kept independent of and tighter than search.rs's own HTTP-layer MAX_SEARCH_RESULTS/MAX_SEARCH_RESPONSE_BYTES bounds"

key-files:
  created:
    - rust/crates/pod0-cli/src/host/capability.rs
  modified:
    - rust/crates/pod0-cli/src/host.rs
    - rust/crates/pod0-cli/Cargo.toml
    - rust/crates/pod0-cli/tests/live_agent.rs

key-decisions:
  - "Moved pod0-application from pod0-cli's [dev-dependencies] to [dependencies] — production code in capability.rs now needs pod0_application::MAX_AGENT_MESSAGE_BYTES, which pod0_facade does not re-export. No Cargo.lock diff resulted (the crate was already resolved into the workspace graph)."
  - "search_outcome (the Ok/Err -> AgentCapabilityOutcome mapping) is factored out of the HTTP-calling execute_search so it's unit-testable with a synthetic Result, without needing a live HTTP fixture for a capability-execution unit test."
  - "The Task 1 unit test asserting 'every non-search action fails with a specific message' drives the real execute() function against a real HostExecutor (constructed via HostConfig::empty(), no network needed) rather than reimplementing the match arm inline, so the test exercises production code, not a parallel copy of it."

requirements-completed: [HOST-05]

coverage:
  - id: D1
    description: "pod0-cli::HostExecutor's PresentAgentApproval arm returns AgentApprovalDecision::Approve unconditionally, matching native AgentApprovalCoordinator"
    requirement: HOST-05
    verification:
      - kind: integration
        ref: "grep -n 'AgentApprovalDecision::Deny' crates/pod0-cli/src/host.rs (no matches); grep -n 'AgentApprovalDecision::Approve' crates/pod0-cli/src/host.rs (matches the PresentAgentApproval arm)"
        status: pass
    human_judgment: false
  - id: D2
    description: "AgentToolAction::Search{tool: SearchPodcastDirectory} executes real HTTP search via search::search and returns AgentCapabilityOutcome::Succeeded{bounded_result} clamped to MAX_AGENT_MESSAGE_BYTES by dropping whole trailing entries"
    requirement: HOST-05
    verification:
      - kind: unit
        ref: "crates/pod0-cli/src/host/capability.rs#tests::search_podcast_directory_succeeds_with_a_short_result_set"
        status: pass
      - kind: unit
        ref: "crates/pod0-cli/src/host/capability.rs#tests::two_hundred_result_response_is_clamped_below_the_bound"
        status: pass
    human_judgment: false
  - id: D3
    description: "Every non-search AgentToolAction variant returns AgentCapabilityOutcome::Failed{safe_detail: Some(_)} naming the specific unsupported action, not the old blanket unsupported_observation string"
    requirement: HOST-05
    verification:
      - kind: unit
        ref: "crates/pod0-cli/src/host/capability.rs#tests::every_non_search_capability_action_fails_with_a_specific_message"
        status: pass
    human_judgment: false
  - id: D4
    description: "A headless integration test drives a real tool-call turn through approval and capability execution to stage: completed, using two local HTTP fixtures (chat completions + iTunes-style search), proving the state machine runs without the iOS simulator"
    requirement: HOST-05
    verification:
      - kind: integration
        ref: "crates/pod0-cli/tests/live_agent.rs#headless_turn_completes_after_approved_capability_execution"
        status: pass
    human_judgment: false

duration: 45min
completed: 2026-08-22
status: complete
---

# Phase 1 Plan 3: Approval/Capability Parity (HOST-05) Summary

**pod0-cli::HostExecutor now approves every agent proposal and executes searchPodcastDirectory for real, with a headless integration test proving the full approval + capability state machine reaches stage: completed without the iOS simulator.**

## Performance

- **Duration:** ~45 min
- **Started:** 2026-08-22 (approx.)
- **Completed:** 2026-08-22
- **Tasks:** 3 completed (Task 2's test was authored alongside Task 1's implementation — see Deviations)
- **Files modified:** 4 (1 created, 3 modified)

## Accomplishments
- Flipped `HostExecutor`'s `PresentAgentApproval` arm from unconditional `Deny` to unconditional `Approve`, matching native `AgentApprovalCoordinator`'s documented "Pod0 does not interrupt the owner to authorize the owner's own agent" behavior — a one-line variant swap, no new conditional logic.
- Added `rust/crates/pod0-cli/src/host/capability.rs`: `ExecuteAgentCapability` now routes to `capability::execute`, which runs `AgentToolAction::Search{tool: SearchPodcastDirectory, ..}` through the existing `search::search` HTTP path (previously wired only to a CLI subcommand) and returns `AgentCapabilityOutcome::Succeeded{bounded_result}`, clamped to `pod0_application::MAX_AGENT_MESSAGE_BYTES` (64 KiB) by dropping whole trailing result entries — never truncating JSON mid-object. Every other `AgentToolAction` variant (~20, matched via a single `_ =>` catch-all) returns a specific `Failed{safe_detail: Some(format!("agent capability '{:?}' is unsupported in the headless host", ...))}` naming the exact action, replacing the old blanket `unsupported_observation` string.
- Added `headless_turn_completes_after_approved_capability_execution` to `tests/live_agent.rs`: a two-connection chat fixture plus a one-connection iTunes-style search fixture drive a real turn — model proposes `search_podcast_directory`, the durable core approves it, `capability::execute` performs a real HTTP search against the local fixture, the bounded result is fed back as a tool message, and the second model turn completes with `stage: "completed"`. Verified stable across repeated runs.

## Task Commits

1. **Task 1: Flip approval to Approve and wire searchPodcastDirectory capability execution** - `28813c5f` (feat) — includes Task 2's bound-clamping test (see Deviations)
2. **Task 3: Headless integration test proving the full approval + capability state machine** - `8edb3c1a` (test)

## Files Created/Modified
- `rust/crates/pod0-cli/src/host/capability.rs` (new) - `pub(crate) fn execute(host, capability) -> HostObservation`; `search_outcome`/`bounded_result_json` helpers; 3 unit tests
- `rust/crates/pod0-cli/src/host.rs` - `pub(crate) mod capability;` declaration; `PresentAgentApproval` arm returns `Approve`; `ExecuteAgentCapability` arm routes to `capability::execute`
- `rust/crates/pod0-cli/Cargo.toml` - `pod0-application` moved from `[dev-dependencies]` to `[dependencies]` (production code needs `MAX_AGENT_MESSAGE_BYTES`, not re-exported by `pod0_facade`)
- `rust/crates/pod0-cli/tests/live_agent.rs` - new test `headless_turn_completes_after_approved_capability_execution`

## Decisions Made
See `key-decisions` in frontmatter — the `pod0-application` dependency-tier move, factoring `search_outcome` out for unit-testability without a live HTTP fixture, and driving the "every non-search action fails" test through the real `execute()` function against a real `HostExecutor` rather than a parallel reimplementation of the match arm.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking issue] `pod0_application::MAX_AGENT_MESSAGE_BYTES` was unreachable from pod0-cli's production code**
- **Found during:** Task 1, first `cargo build -p pod0-cli`
- **Issue:** `pod0-cli`'s `Cargo.toml` only listed `pod0-application` under `[dev-dependencies]`; the plan's own must-haves reference `pod0_application::MAX_AGENT_MESSAGE_BYTES` directly from `capability.rs` (non-test code), and `pod0_facade`'s re-export list does not include this constant.
- **Fix:** Moved `pod0-application.workspace = true` from `[dev-dependencies]` to `[dependencies]` in `crates/pod0-cli/Cargo.toml`.
- **Files modified:** `rust/crates/pod0-cli/Cargo.toml`
- **Verification:** `cargo build -p pod0-cli --all-targets --locked` succeeds; `Cargo.lock` unchanged (the crate was already resolved into the workspace dependency graph as a dev-dependency).
- **Committed in:** `28813c5f` (Task 1 commit)

**2. [Rule 3 - Blocking issue] Task 3's fixture needed a second HTTP endpoint the plan's action text didn't call out**
- **Found during:** Task 3 planning, before writing the test
- **Issue:** The plan's Task 3 action describes serving "two sequential HTTP responses from the same fixture thread" for the model-turn chat endpoint, but a `search_podcast_directory` capability also issues a real HTTP request to the iTunes search endpoint (via `search::search`) between the two model turns. Without intercepting that second endpoint too, the test would either hang reaching the real internet or fail non-deterministically.
- **Fix:** Added a second local `TcpListener` fixture for the search request, redirected via the `POD0_PODCAST_SEARCH_URL` env var — the exact mechanism `tests/live_search.rs` already establishes for the same purpose (including its documented single-threaded-w.r.t.-this-env-var safety comment, reused verbatim in the new test).
- **Files modified:** `rust/crates/pod0-cli/tests/live_agent.rs`
- **Verification:** `cargo test -p pod0-cli --test live_agent --locked` passes, run 4 times consecutively with no flakiness.
- **Committed in:** `8edb3c1a` (Task 3 commit)

**3. [Task-boundary deviation, not a bug] Task 2's bound-clamping test was authored as part of Task 1's commit, not a separate one**
- **Found during:** Task 1, while implementing `bounded_result_json`
- **Issue:** The plan structures Task 1 (implement the clamp) and Task 2 (add a load-bearing test proving the clamp) as separate tasks with separate verification commands. Writing the clamp without a test that exercises it in the same sitting risked landing untested clamp logic; the natural single-sitting shape was to write both together.
- **Resolution:** Both `two_hundred_result_response_is_clamped_below_the_bound` (Task 2's required test) and Task 1's own tests landed in the Task 1 commit (`28813c5f`). Verified the test is load-bearing per Task 2's own acceptance criteria: temporarily reverted `bounded_result_json` to a naive `serde_json::to_string` with no clamp loop, re-ran `cargo test -p pod0-cli capability`, confirmed the test fails red (`assertion failed: bounded.len() <= pod0_application::MAX_AGENT_MESSAGE_BYTES`), then restored the real implementation and confirmed the file diff against the commit is empty (no drift).
- **Committed in:** `28813c5f` (folded into Task 1's commit; no separate Task 2 commit exists)

---

**Total deviations:** 3 (2 Rule 3 blocking-issue auto-fixes necessary for the plan to compile/pass as specified; 1 task-boundary consolidation, verified not to weaken Task 2's own acceptance bar)
**Impact on plan:** No scope creep — all three changes were necessary to make the plan's own literal must-haves buildable and testable. The task-boundary consolidation was independently verified against Task 2's exact acceptance criterion (red without the clamp) before being accepted as equivalent.

## Issues Encountered

**Full-workspace `cargo test --workspace --all-features --locked` surfaces 9 failures unrelated to this plan.** 4 are the pre-existing `FACADE_CONTRACT_VERSION` fixture-drift failures (55 vs 54) already documented as out-of-scope in the prerequisite state handed to this executor. The other 5 (`pod0-facade::runtime_chapter_workflow_tests::terminal_failure_can_be_retried_then_cancelled_through_typed_commands`, `pod0-facade::user_data_erasure_facade::...`, `pod0-live-hosts::download_tcp::failed_download_leaves_no_staged_or_temporary_file`, a `pod0-live-hosts::http_tcp` test binary failure, and `pod0-storage::lifecycle_wake_tests::lifecycle_cancellation_supersedes_unclaimed_wake_durably`) are new and were **not** present in the documented pre-existing-issue list. `git status` at both the start and end of this session shows the same set of uncommitted, unstaged modifications to `rust/crates/pod0-facade/src/{facade_exports.rs,runtime.rs,runtime_transcript_effect_leases.rs}` and `rust/crates/pod0-storage/src/{effect_outbox.rs,effect_outbox_tests.rs,exports.rs,lib.rs,library_store_activity.rs,lifecycle_wake_tests.rs,transition_commit_workflow_configuration.rs}` (substantial in-progress diffs, e.g. `runtime.rs` +120/-3, `effect_outbox.rs` +85), plus an untracked `rust/crates/pod0-storage/src/authoritative_bootstrap.rs` — none of which this plan's task list (`host.rs`, `host/capability.rs`, `tests/live_agent.rs`) touches or was scoped to touch. This is a different, concurrent, uncommitted workstream editing `pod0-facade`/`pod0-storage` in the same shared working tree (this session ran on the main checkout, not an isolated worktree, per the orchestrator's explicit instruction). Per the scope boundary rule ("only auto-fix issues DIRECTLY caused by the current task's changes"), these failures were left untouched and unfixed. **This plan's own scope is verified clean:** `cargo build -p pod0-cli --all-targets --locked`, `cargo test -p pod0-cli --all-targets --locked`, `cargo clippy -p pod0-cli --all-targets --all-features --locked -- -D warnings`, `cargo build --workspace --all-targets`, and `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` all pass with zero warnings/errors — the workspace **compiles and lints cleanly** with this plan's changes. Only the workspace-wide **test** run is affected, and only by unrelated crates' in-progress uncommitted state. **Flagging for the orchestrator's phase-level verification step:** if that step runs `cargo test --workspace` against this same shared checkout before the concurrent `pod0-facade`/`pod0-storage` work is committed or reconciled, it will likely surface these same 5 additional failures — they predate and are independent of this plan's commits (`28813c5f`, `8edb3c1a`), which touch only `pod0-cli`.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

HOST-05 is closed to the depth the requirement asks for (state-machine proof, not full capability coverage, per 01-CONTEXT.md's D-05/scope-boundary note): approval parity, one real headlessly-replicable capability wired end to end, and a passing integration test proving the whole chain. This was the last plan in Phase 1 (Headless Host Crates). No blockers for downstream phases from this plan's own changes; see Issues Encountered above regarding the shared checkout's concurrent uncommitted `pod0-facade`/`pod0-storage` work, which is outside this plan's scope but may affect the phase-level workspace-test verification step that follows.

---
*Phase: 01-headless-host-crates*
*Completed: 2026-08-22*

## Self-Check: PASSED

All claimed created/modified files (`host/capability.rs`, `host.rs`, `tests/live_agent.rs`, `Cargo.toml`, this SUMMARY) and both task commit hashes (`28813c5f`, `8edb3c1a`) verified present via file-existence checks and `git log --oneline --all | grep`.
