---
phase: 01-headless-host-crates
plan: 05
subsystem: infra
tags: [tokio, pod0-nostr-host, runtime]

requires:
  - phase: 01-headless-host-crates
    provides: "pod0-portable-media's MediaLoader::new_with_handle owned_runtime/handle dual-field precedent (01-02); pod0-cli's single shared multi-thread tokio runtime (01-02)"
provides:
  - "NostrPublisher::new_with_handle — an additive constructor that accepts a caller-owned tokio::runtime::Handle instead of building an owned Runtime"
  - "A test proving Handle::block_on driven from a second OS thread through NostrPublisher's real publish_to_relay I/O path completes rather than hangs"
affects: []

actuals:
  tokens: 2242
  tasks: 1
  commits: 1

tech-stack:
  added: []
  patterns:
    - "NostrPublisher mirrors pod0-portable-media's HttpTransport dual-field shape exactly: owned_runtime: Option<Runtime> + always-present handle: Handle, with publish's single block_on call site dispatching through owned_runtime when present, the shared Handle otherwise — same pattern, no new abstraction invented"

key-files:
  created: []
  modified:
    - rust/crates/pod0-nostr-host/src/publisher.rs
    - rust/crates/pod0-nostr-host/src/publisher_tests.rs
    - rust/crates/pod0-nostr-host/Cargo.toml

key-decisions:
  - "NostrPublisher::new_with_handle relay-target validation is identical to new — the only difference is the runtime source, matching the plan's explicit instruction not to change validation/target-resolution logic"
  - "Test proves the cross-thread block_on path via an unreachable relay target (ws://127.0.0.1:9, no listener) with a bounded 5s operation_timeout and a 10s wall-clock assertion, rather than the pre-cancelled-token shortcut, because cancellation-before-signing returns before any block_on call and would not exercise the real pitfall"
  - "Added tokio's 'macros' feature to pod0-nostr-host's production dependency — relay.rs's pre-existing tokio::select! (unrelated to this plan's edits) only ever compiled before via feature unification when built alongside sibling workspace crates (01-01/01-02's verification commands always combined multiple -p flags), never truly standalone -p pod0-nostr-host as this plan's own acceptance criteria require; this was a real, previously-undetected gap in every prior standalone-build claim for this crate"
  - "Added tokio's 'rt-multi-thread' feature as a dev-dependency-only addition (not production) so the new test's Builder::new_multi_thread() compiles without expanding pod0-nostr-host's production feature surface"

requirements-completed: [HOST-04]

coverage:
  - id: D1
    description: "NostrPublisher::new_with_handle exists as an additive constructor; NostrPublisher::new is unchanged; a test proves cross-thread Handle::block_on completes through the real publish_to_relay path"
    requirement: HOST-04
    verification:
      - kind: unit
        ref: "rust/crates/pod0-nostr-host/src/publisher_tests.rs#new_with_handle_publish_completes_when_driven_from_a_second_thread"
        status: pass
      - kind: integration
        ref: "cd rust && cargo test -p pod0-nostr-host --all-features --locked (11 passed, 0 failed)"
        status: pass
      - kind: integration
        ref: "cd rust && cargo clippy -p pod0-nostr-host --all-targets --all-features --locked -- -D warnings (clean)"
        status: pass
    human_judgment: false
  - id: D2
    description: "cargo build --workspace --all-targets --all-features --locked passes against committed HEAD with the 11 dirty/untracked pod0-facade/pod0-storage files stashed out, and pod0-facade/pod0-storage were not modified by this plan"
    verification:
      - kind: integration
        ref: "git stash push --include-untracked -- <11 files>; cd rust && cargo build --workspace --all-targets --all-features --locked; git stash pop (build finished, 0 errors; stash popped cleanly, all 11 files restored)"
        status: pass
    human_judgment: false

duration: 20min
completed: 2026-08-22
status: complete
---

# Phase 1 Plan 5: Handle-Based NostrPublisher Constructor Summary

**Added `NostrPublisher::new_with_handle`, an additive constructor mirroring `pod0-portable-media`'s established `owned_runtime`/`handle` dual-field shape, plus a test that genuinely drives `publish`'s `block_on` call site from a second OS thread against an unreachable relay to prove no cross-thread hang.**

## Performance

- **Duration:** ~20 min
- **Started:** 2026-08-22 (approx.)
- **Completed:** 2026-08-22T19:53:21Z
- **Tasks:** 1 completed
- **Files modified:** 3

## Accomplishments
- `NostrPublisher` now holds `owned_runtime: Option<tokio::runtime::Runtime>` + `handle: tokio::runtime::Handle` instead of an always-owned `Runtime` field — `NostrPublisher::new` (unchanged behavior) sets `owned_runtime: Some(...)`; the new `NostrPublisher::new_with_handle` sets `owned_runtime: None` and stores the caller's `Handle`.
- `publish`'s single `block_on` call site dispatches through `owned_runtime.block_on` when present, `handle.block_on` otherwise — no duplicated dispatch logic, no change to `publish`'s public signature.
- New test `new_with_handle_publish_completes_when_driven_from_a_second_thread`: builds a `Builder::new_multi_thread().worker_threads(1)` `Runtime` on the test thread, clones its `Handle`, then from a spawned second thread constructs `NostrPublisher::new_with_handle` with that `Handle` and calls `publish` against an unreachable local relay (`ws://127.0.0.1:9`), asserting the call returns within a bounded 10s wall-clock deadline. This genuinely reaches `publish_to_relay`'s `block_on` (not a pre-cancelled early return), proving the exact pitfall 01-02-SUMMARY.md documented (a current-thread runtime's `Handle::block_on` hangs forever when called cross-thread) does not recur here.
- Discovered and fixed a real, previously-undetected gap: `pod0-nostr-host` never actually compiled standalone via `cargo test -p pod0-nostr-host --all-features --locked` — every prior verification command (01-01, 01-04) combined it with sibling `-p` flags, relying on feature unification to supply `tokio`'s `macros` feature that `relay.rs`'s pre-existing `tokio::select!` needs. Added `macros` to `pod0-nostr-host`'s production `tokio` dependency (fixing the crate's own standalone buildability, not a change caused by this plan's edits) and `rt-multi-thread` as a dev-dependency-only addition for the new test's multi-thread `Runtime`.

## Task Commits

1. **Task 1: Add an additive Handle-based constructor to NostrPublisher and prove cross-thread block_on correctness** - `1a36e821` (feat)

## Files Created/Modified
- `rust/crates/pod0-nostr-host/src/publisher.rs` - `owned_runtime`/`handle` dual-field struct shape; `new_with_handle` (new); `publish`'s `block_on` call site dispatches on `owned_runtime`
- `rust/crates/pod0-nostr-host/src/publisher_tests.rs` - new cross-thread `Handle::block_on` test
- `rust/crates/pod0-nostr-host/Cargo.toml` - `tokio` gains `macros` (production, fixes pre-existing standalone-build gap) and `rt-multi-thread` (dev-dependency only, for the new test)

## Decisions Made
See `key-decisions` in frontmatter — identical validation logic between `new`/`new_with_handle`, the unreachable-relay-target test design (not the pre-cancelled shortcut), and the production-vs-dev-dependency split for the two added tokio features.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking issue] `pod0-nostr-host` did not compile standalone via the plan's own literal verify command**
- **Found during:** Task 1, first run of `cargo test -p pod0-nostr-host --all-features --locked`
- **Issue:** `E0433: could not find select in tokio` at `relay.rs:89` (pre-existing code, untouched by this plan). `pod0-nostr-host`'s `Cargo.toml` requests `tokio` features `["io-util", "net", "rt", "time"]` — missing `macros`, which `tokio::select!` requires. This compiled successfully in every prior verification (01-01, 01-04) only because those commands always built `pod0-nostr-host` alongside sibling crates (e.g. `cargo test -p pod0-cli -p pod0-live-hosts -p pod0-nostr-host ...`), letting Cargo's feature unification supply `macros` from another crate in the same build graph. This plan's own acceptance criteria require a literal `-p pod0-nostr-host` standalone command, which surfaced the gap for the first time. Verified pre-existing (not caused by this plan's edits) by reproducing the identical error with `publisher.rs`/`publisher_tests.rs` stashed back to HEAD.
- **Fix:** Added `macros` to `pod0-nostr-host`'s production `tokio` dependency features.
- **Files modified:** `rust/crates/pod0-nostr-host/Cargo.toml`
- **Verification:** `cargo test -p pod0-nostr-host --all-features --locked` passes (11 tests, 0 failed); `cargo clippy -p pod0-nostr-host --all-targets --all-features --locked -- -D warnings` clean; `rust/Cargo.lock` unchanged (tokio-macros already resolved elsewhere in the lockfile).
- **Committed in:** `1a36e821` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 3, a pre-existing standalone-build gap surfaced by this plan's own literal verify command, not caused by this plan's own edits)
**Impact on plan:** Necessary to actually run this plan's own acceptance-criteria commands. No scope creep — the fix is a one-line feature addition to `pod0-nostr-host`'s own `Cargo.toml`, not a change to any other crate.

## Issues Encountered

The plan's literal acceptance-criteria grep `grep -n 'fn new(' rust/crates/pod0-nostr-host/src/publisher.rs` does not match either the pre-plan or post-plan signature — `NostrPublisher::new` has always been generic (`pub fn new<I, S>(`), so the literal `fn new(` substring never appears with or without this plan's changes. Confirmed via `git show HEAD:rust/crates/pod0-nostr-host/src/publisher.rs | grep -n 'fn new'` that the committed pre-plan signature was already `pub fn new<I, S>(`. The actual invariant the criterion is checking — "the original constructor's signature is unchanged" — holds: `new<I, S>`'s parameter list, generics, and body are byte-for-byte the same as before this plan except for the two new struct-field assignments (`owned_runtime: Some(runtime)`, `handle` instead of the old single `runtime` field), which are the minimum change required to add the second construction path per the plan's own `<behavior>` spec.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

The concretely-fixable half of the HOST-04/SC4 gap is closed: `pod0-nostr-host`'s `NostrPublisher` no longer unconditionally owns a second runtime — it is now Handle-ready, matching `pod0-portable-media`'s established pattern, so it will not violate the single-shared-runtime invariant the moment it is ever linked into a shared process. The other half of SC4 — no process currently links all six host crates together, so "all six share one runtime" remains unexercised by any test — is still genuinely open, exactly as this plan's objective flagged. That wiring requires a new `HostRequest`/`HostObservation` contract surface in `pod0-application`/`pod0-facade`, which is out of this plan's scope (those files remain untouched) and out of Phase 1's scope as currently defined. Recommend either a follow-up phase to wire `pod0-nostr-host`/`pod0-system-hosts`/`pod0-tts-host` into `pod0-cli` once facade/storage contract work is available, or accept ROADMAP.md's 2026-08-22 reworded SC4 as the final Phase 1 bar (already reworded to reflect exactly this state). This was the last plan in Phase 1 — Phase 1 is ready for re-verification against the reworded SC4.

---
*Phase: 01-headless-host-crates*
*Completed: 2026-08-22*

## Self-Check: PASSED

All 3 claimed modified files verified present via file-existence checks; commit hash `1a36e821` verified present via `git log --oneline --all | grep 1a36e821`.
