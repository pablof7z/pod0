---
phase: 01-headless-host-crates
plan: 06
subsystem: testing
tags: [rust, pod0-cli, pod0-storage, sqlite, rusqlite, gap-closure]

requires:
  - phase: 01-headless-host-crates
    provides: "pod0-cli reworked to depend only on committed pod0-facade/pod0-storage symbols; create_store disabled and 11 tests #[ignore]d pending a store-bootstrap primitive (01-04)"
provides:
  - "rust/crates/pod0-cli/tests/support::bootstrap_authoritative_store — a raw-SQL fixture that pre-seeds a fresh SQLite file with every authority marker Pod0Facade::open requires, without calling the disabled Pod0Facade::create"
  - "headless_turn_completes_after_approved_capability_execution restored as a passing, non-ignored test — the only automated proof of HOST-05's approval + capability-execution parity"
  - "9 of the 11 previously-#[ignore]d pod0-cli tests now run un-ignored and pass; the remaining 2 carry dated, linked reasons naming genuinely new, out-of-scope blockers"
affects: []

actuals:
  tokens: 5017
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "A test-only store fixture can reach a Pod0Facade::open-able store via CoreStoreMigrator (schema only) plus raw SQL that mirrors the exact literal shapes already committed and passing in pod0-storage's own note/chapter import code — no legacy-JSON import flow required"
    - "Pod0Facade::open requires FOUR domains authoritative (listening, notes, transcripts, clips), not just the three named in this plan's own read_first guidance — clip_snapshot() is called unconditionally inside FacadeState::open and fails closed if the clips domain cutover is missing"
    - "The listening/notes/clips singleton state tables (pod0_playback_state, pod0_note_state, pod0_clip_state) enforce FK+CHECK constraints requiring a real, minimally-valid import row (64-char hex source_hash, backup_byte_count>0, sleep_mode_code/sleep_wire_code paired) — these can be reached by direct INSERT with placeholder values without going through the real Importer/legacy-JSON path"

key-files:
  created:
    - rust/crates/pod0-cli/tests/support/mod.rs
    - rust/crates/pod0-cli/tests/store_bootstrap_smoke.rs
  modified:
    - rust/crates/pod0-cli/tests/live_agent.rs
    - rust/crates/pod0-cli/tests/host_drain.rs
    - rust/crates/pod0-cli/tests/host_pump.rs
    - rust/crates/pod0-cli/tests/live_feed.rs
    - rust/crates/pod0-cli/tests/live_search.rs
    - rust/crates/pod0-cli/tests/settings.rs

key-decisions:
  - "Added 'clips' as a fourth domain to the fixture's authoritative-cutover set, beyond the plan's read_first-specified listening/notes/transcripts — discovered empirically that FacadeState::open unconditionally calls clip_snapshot(), which fails CutoverNotAuthoritative otherwise"
  - "Populated pod0_listening_imports/pod0_note_imports/pod0_clip_imports with minimal placeholder rows (not through the real Importer/legacy-JSON flow) so the FK-constrained pod0_playback_state/pod0_note_state/pod0_clip_state singleton rows can be inserted directly — discovered via CHECK/FK constraint trial-and-error, not documented in any read_first reference"
  - "host_drain.rs's pending_host_diagnostics_do_not_claim_or_mutate_work re-ignored: next_leased_host_requests (pod0-facade, out of scope) is not idempotent on repeated calls against the same pending work — reproduces identically against committed HEAD with the concurrent WIP stashed out, so it is a real pod0-facade bug, not a store-bootstrap gap"
  - "settings.rs's workflow_settings_are_initialized_by_the_user_command_and_reopen re-ignored: it only passes with the concurrent uncommitted fix in pod0-storage's transition_commit_workflow_configuration.rs (a first-time settings_set is incorrectly rejected as a revision conflict at committed HEAD); confirmed by running the identical stash-based check with and without that one file stashed"

requirements-completed: [HOST-05]

coverage:
  - id: D1
    description: "headless_turn_completes_after_approved_capability_execution runs (not --ignored) and passes against committed HEAD with the concurrent pod0-facade/pod0-storage WIP stashed out, restoring HOST-05/SC5's evidence"
    requirement: HOST-05
    verification:
      - kind: integration
        ref: "cd rust && git stash push --include-untracked -- <11 files> && cargo test -p pod0-cli --test live_agent headless_turn_completes_after_approved_capability_execution -- --exact; git stash pop"
        status: pass
    human_judgment: false
  - id: D2
    description: "rust/crates/pod0-facade/ and rust/crates/pod0-storage/ are unmodified by this plan"
    verification:
      - kind: other
        ref: "git status --short rust/crates/pod0-facade rust/crates/pod0-storage before and after each task — identical 10 modified + 1 untracked file, none touched"
        status: pass
    human_judgment: false
  - id: D3
    description: "As many of the other 11 previously-#[ignore]d pod0-cli tests as the fixture pattern cleanly restores are un-ignored and passing; any left ignored carry an updated, dated, linked reason distinct from the closed store-bootstrap gap"
    requirement: HOST-05
    verification:
      - kind: integration
        ref: "cd rust && git stash push --include-untracked -- <11 files> && cargo test -p pod0-cli --all-features --locked; git stash pop — 0 failed, 3 ignored (settings.rs's untouched totals-limitation test plus the 2 newly-discovered out-of-scope blockers)"
        status: pass
    human_judgment: false

duration: 70min
completed: 2026-08-23
status: complete
---

# Phase 1 Plan 6: Gap Closure — Restore SC5/HOST-05's Headless Approval Test Summary

**Added a raw-SQL store-bootstrap test fixture that reaches a `Pod0Facade::open`-able store without the disabled `Pod0Facade::create`, restoring `headless_turn_completes_after_approved_capability_execution` (HOST-05's only end-to-end approval + capability-execution proof) and 8 of the other 10 previously-`#[ignore]`d `pod0-cli` tests.**

## Performance

- **Duration:** ~70 min
- **Started:** 2026-08-23 (approx.)
- **Completed:** 2026-08-23
- **Tasks:** 3 completed
- **Files modified:** 8 across 3 task commits

## Accomplishments
- `rust/crates/pod0-cli/tests/support::bootstrap_authoritative_store` pre-seeds a fresh SQLite file with the `listening`/`notes`/`transcripts`/`clips` authority markers and the minimal import + state singleton rows `Pod0Facade::open` requires, proven by a standalone smoke test (`store_bootstrap_smoke.rs`).
- `live_agent.rs`'s 4 tests — including `headless_turn_completes_after_approved_capability_execution`, the sole proof of HOST-05's approval-parity criterion — are un-ignored and pass, verified against committed HEAD with the concurrent `pod0-facade`/`pod0-storage` WIP stashed out.
- 5 more previously-`#[ignore]`d test files (`host_drain.rs`, `host_pump.rs`, `live_feed.rs`, `live_search.rs`, `settings.rs`) got the same fixture swap applied; 4 of their 6 in-scope tests now pass un-ignored. The `#[ignore]` count across `pod0-cli` dropped from 11 to 3 (2 newly-discovered, genuinely different blockers plus the 1 pre-existing, explicitly out-of-scope totals-limitation test).

## Task Commits

1. **Task 1: Store-bootstrap test helper** - `e960039f` (feat)
2. **Task 2: Restore live_agent.rs's 4 tests** - `e8576390` (fix)
3. **Task 3: Best-effort fixture swap on remaining test files** - `759bfb69` (fix)

## Files Created/Modified
- `rust/crates/pod0-cli/tests/support/mod.rs` - `bootstrap_authoritative_store`: `CoreStoreMigrator`-created schema plus raw-SQL authority seeding for 4 domains and the chapter singleton
- `rust/crates/pod0-cli/tests/store_bootstrap_smoke.rs` - standalone smoke test proving the fixture's output opens via `Pod0Facade::open`
- `rust/crates/pod0-cli/tests/live_agent.rs` - all 4 tests un-ignored; `create_request`/`create_store` swapped for `support::bootstrap_authoritative_store` + `open_request`/`open_store`
- `rust/crates/pod0-cli/tests/host_drain.rs` - fixture swap applied; test re-ignored for a new, unrelated `pod0-facade` bug
- `rust/crates/pod0-cli/tests/host_pump.rs` - fixture swap applied to all 3 tests; all pass un-ignored
- `rust/crates/pod0-cli/tests/live_feed.rs` - fixture swap applied; test passes un-ignored
- `rust/crates/pod0-cli/tests/live_search.rs` - fixture swap applied; test passes un-ignored
- `rust/crates/pod0-cli/tests/settings.rs` - fixture swap applied to the first test only, which is then re-ignored for a new, unrelated `pod0-storage` bug; the totals-limitation second test is untouched

## Decisions Made
See `key-decisions` in frontmatter — the fourth `clips` domain requirement discovered empirically, the minimal-placeholder import rows needed to satisfy FK/CHECK constraints on the singleton state tables, and the two tests re-ignored for genuinely new (not store-bootstrap) blockers confirmed to reproduce against committed HEAD.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug, plan under-scoped] `Pod0Facade::open` requires a fourth authoritative domain (`clips`) the plan's `read_first` did not name**
- **Found during:** Task 1, running the smoke test for the first time
- **Issue:** The plan's `read_first` for Task 1 lists only `listening`/`notes`/`transcripts` (via `LibraryStore::open_authoritative`, `chapter_store_is_authoritative`, `require_notes_authoritative`, `TranscriptStore::open_authoritative`) as the authority markers `open_with_clock_value` enforces. Empirically, `FacadeState::open` (called internally by `open_with_clock_value`) also unconditionally calls `store.clip_snapshot()`, which fails `CutoverNotAuthoritative` unless the `clips` domain cutover is also `'authoritative'`.
- **Fix:** Added a `clips` domain-cutover row plus a minimal `pod0_clip_imports`/`pod0_clip_state` pair to the fixture, using the same raw-SQL pattern as the other three domains.
- **Files modified:** `rust/crates/pod0-cli/tests/support/mod.rs`
- **Verification:** `store_bootstrap_smoke.rs` passes; `Pod0Facade::open` returns `Ok` on the pre-seeded fixture.
- **Committed in:** `e960039f` (Task 1 commit)

**2. [Rule 1 - Bug, plan under-scoped] Singleton state tables require real FK-referenced import rows, not just a domain-cutover flag**
- **Found during:** Task 1, iterating on the smoke test failure
- **Issue:** `pod0_playback_state`, `pod0_note_state`, and `pod0_clip_state` each have a `source_import_id` FK into their respective `*_imports` table, plus CHECK constraints (`length(source_hash)=64`, `backup_byte_count>0`, `sleep_mode_code` paired with a non-null `sleep_wire_code` when `=255`, `collection_revision>=1`) discovered only by trial against the live schema — none of this is described in the plan's cited reference patterns (`agent_store_tests.rs`, `model_chapter_workflow/success_tests.rs`), which only exercise schema-only or chapter-only stores.
- **Fix:** Inserted minimal, valid placeholder rows into `pod0_listening_imports`/`pod0_note_imports`/`pod0_clip_imports` (64-char hex `source_hash`, `backup_byte_count=1`) before each singleton state row, satisfying every constraint without using the real Importer/legacy-JSON flow.
- **Files modified:** `rust/crates/pod0-cli/tests/support/mod.rs`
- **Verification:** `store_bootstrap_smoke.rs` and all of `live_agent.rs`'s 4 tests pass; `cargo clippy -p pod0-cli --all-targets --all-features --locked -- -D warnings` clean.
- **Committed in:** `e960039f` (Task 1 commit)

**3. [Rule 4 boundary — documented, not auto-fixed] Two tests re-ignored for genuinely new, out-of-scope reasons after the fixture swap**
- **Found during:** Task 3, running the full `pod0-cli` suite against committed HEAD (WIP stashed) per the team-lead's critical-boundary instruction
- **Issue:** `host_drain.rs`'s `pending_host_diagnostics_do_not_claim_or_mutate_work` fails because `Pod0Facade::next_leased_host_requests` is not idempotent on repeated calls against the same pending work (reproduces identically at committed HEAD, unrelated to store bootstrap). `settings.rs`'s `workflow_settings_are_initialized_by_the_user_command_and_reopen` only passes with the concurrent uncommitted fix in `pod0-storage`'s `transition_commit_workflow_configuration.rs` (confirmed by running the stash-based check with and without that single file stashed).
- **Fix:** Per Rule 4 (architectural/behavioral change outside this plan's `pod0-cli`-only scope, in files this plan is explicitly forbidden from touching), re-added `#[ignore]` to both, each with a single dated (2026-08-23), linked reason string distinct from the resolved store-bootstrap gap, naming the specific remaining blocker. No `pod0-facade`/`pod0-storage` file was modified.
- **Files modified:** `rust/crates/pod0-cli/tests/host_drain.rs`, `rust/crates/pod0-cli/tests/settings.rs`
- **Verification:** `cd rust && git stash push --include-untracked -- <11 files> && cargo test -p pod0-cli --all-features --locked; git stash pop` — 0 failed, 3 ignored, stash restored cleanly.
- **Committed in:** `759bfb69` (Task 3 commit)

---

**Total deviations:** 3 auto-fixed/documented (2 Rule 1 bugs in the plan's own scoping, 1 Rule 4 boundary correctly deferred rather than worked around)
**Impact on plan:** No scope creep into `pod0-facade`/`pod0-storage` — both newly-discovered blockers live entirely in those protected, concurrently-WIP crates and are documented, not patched around. The `clips`-domain and import-row discoveries were necessary corrections to the plan's own fixture design, not expansions of the plan's goal.

## Issues Encountered

The plan's `read_first` guidance for Task 1 undercounted what `Pod0Facade::open` actually requires (see deviations 1 and 2 above); reaching a working fixture required iterative empirical discovery via temporary debug tests (removed before the final commit) rather than a single read-and-implement pass. No other issues.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

HOST-05/SC5 is closed: `headless_turn_completes_after_approved_capability_execution` runs and passes as a normal (non-`--ignored`) test against committed HEAD, independently reproduced with the concurrent `pod0-facade`/`pod0-storage` WIP stashed out. This was the last plan needed to close Phase 1 (Headless Host Crates) — all 5 of Phase 1's roadmap success criteria (SC1–SC5) now have passing, reproducible automated evidence against committed HEAD.

3 tests remain `#[ignore]`d in `pod0-cli`, down from 11: `settings.rs`'s pre-existing, explicitly out-of-scope totals-limitation test (unrelated to this plan), plus 2 newly-discovered `pod0-facade`/`pod0-storage` bugs (a non-idempotent `next_leased_host_requests` and an uncommitted workflow-configuration revision-conflict fix) that are out of this plan's `pod0-cli`-only scope and belong to whoever lands that concurrent WIP as its own reviewed commit.

---
*Phase: 01-headless-host-crates*
*Completed: 2026-08-23*

## Self-Check: PASSED

All 8 claimed created/modified files and all three task commit hashes (`e960039f`, `e8576390`, `759bfb69`) verified present via file-existence checks and `git log --oneline --all | grep`.
