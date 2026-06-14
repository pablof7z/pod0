---
type: research-record
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
source_lines: 10364-10421
agent_attribution: Opus review agent (a1887fe064c548b24)
has_preregistered_criteria: true
has_method: true
has_structured_report: true
characterization: "Structured code review investigation of PR #444's e2e test assertions, finding that headline Assertion C (last_published_at stamped) is unsound as a signing proxy — it passes even if signing is deleted — with per-item PASS/PARTIAL PASS/FAIL verdicts and a FIX-FIRST conclusion"
captured_at: 2026-06-13T23:59:33Z
---

Structured code review investigation of PR #444's e2e test assertions, finding that headline Assertion C (last_published_at stamped) is unsound as a signing proxy — it passes even if signing is deleted — with per-item PASS/PARTIAL PASS/FAIL verdicts and a FIX-FIRST conclusion

---

User: [Agent task result: Agent "Opus review PR #444 e2e scenario" completed]
(Voice endpoint returns 404 — server not running; proceeding with the review.)

---

📋 SUMMARY: Read-only review of PR #444 — the rewritten `nipf4_publish.rs` headless scenario guarding the per-podcast NIP-F4 register→sign→publish seam.

🔍 ANALYSIS: I verified the scenario against the real production handlers in `host_op_publish.rs`, the dispatch layer in `nmp_dispatch.rs`, the harness, and the CI workflows. One finding materially undermines the test's central claim (Assertion C). The seam is real, but the chosen observable does not prove signing.

⚡ ACTIONS: Examined `gh pr diff/view 444`, `host_op_publish.rs:106-270`, `nmp_dispatch.rs:44-160`, `harness.rs`, `fixtures.rs`, `mod.rs`, `relay_smoke.rs`, and `.github/workflows/test.yml` — all via `git show`/`git grep` on `origin/test/perpodcast-publish-e2e`. No files modified.

✅ RESULTS — per-item verdicts (all line refs are `apps/nmp-app-podcast/src/bin/headless/scenarios/nipf4_publish.rs` unless noted):

**1. Drives the REAL kernel signing path — PARTIAL PASS (with a serious caveat).**
The scenario boots the real kernel (harness `app_new`/`dispatch` go through `nmp_app_dispatch_action`, not a null app) and dispatches `create_owned_podcast`, `publish_show`, `publish_episode` through the same `podcast.publish` namespace the app uses (lines 158, 224, 320, 367). Those handlers (`host_op_publish.rs:106`, `:186`) genuinely call `register_podcast_signer_in_kernel` (`host_op_publish.rs:152`, `:234`) then `publish_raw_with_signer_via_nmp` (`:165`, `:263`). So the *dispatch* path is exercised end-to-end.

**BUT** — it does NOT observe that the kernel actually *signed* with the per-podcast key. `publish_raw_with_signer_via_nmp` returns `"queued"` (`nmp_dispatch.rs:66-86` → `dispatch_nmp_publish`); signing happens later, asynchronously, inside the kernel actor's `sign_with_account_nonblocking`. The scenario never inspects a signed event's `pubkey`/`sig`. This is the core gap.

**2. Assertions — mostly sound, except C:**
- **A (pubkey ≠ active account):** PASS — line 207. Reads `podcast_pubkey_hex` from the snapshot, validates 64-char hex (line 200), and asserts `!=` active (line 207). Would genuinely FAIL if `create_owned_podcast` reused the identity key. Catches a real regression.
- **B (active unchanged):** PASS — lines 215-228 (pre), 300-318 (post-show), 343-360 (post-episode), 396-410 (post-re-register). Would FAIL if a regression passed `make_active=1`. Sound, and checked at every stage. Good.
- **C (`last_published_at` stamped):** **FAIL as a signing proxy — this is the BLOCKER.** Read `host_op_publish.rs:152-166`: the handler calls `register_podcast_signer_in_kernel` (line 152, a `void` no-error-return FFI call), then *unconditionally* stamps `last_published_at` and `rev.fetch_add` (lines 157-162), then calls `publish_raw_with_signer_via_nmp` (line 165). So `last_published_at` is stamped **before and independent of** whether signing or the relay publish succeeds. Worse: `register_podcast_signer_in_kernel` (`nmp_dispatch.rs:44-55`) returns `()` — it cannot fail-fast; even if the signer registration were silently broken, the stamp still lands. **Therefore C can be true with zero successful per-podcast signing.** The PR description's claim ("FAILS/times-out if register_podcast_signer_in_kernel is dropped") is incorrect: if you deleted line 152 entirely, the stamp at line 159 still fires and C still PASSES. C only catches an *early return before line 159* (store-poisoned, podcast-not-found, key-not-owned), not a signing regression. This is false confidence — exactly the risk flagged in the prompt.
- **D (episode publish accepted):** WEAK PASS — lines 320-360. Asserts only `correlation_id` present (line 333) + active unchanged after a 600ms sleep (line 339). Acceptance just means the action enqueued; it proves nothing about kind:54 signing. The `make_active` half is sound; the "publish accepted" half is trivially near-true (any non-erroring dispatch returns a correlation_id).
- **E (idempotent re-registration):** PASS-ish — lines 367-410. Second `publish_show` returns correlation_id + active unchanged. Genuinely catches "duplicate AddSigner errors" or "re-register flips active." Reasonable. (The `wait_for` at 388-394 is best-effort and its result is discarded, which is fine since it's only a settling delay.)

**Net:** A, B, E are real regression guards. C is the headline assertion and it does NOT guard signing — it guards "the handler didn't early-return before the stamp." D is half-real.

**3. No false pass via Skip — PASS (clean).** The scenario contains zero `Skip` returns (grep confirmed only doc-comment mentions; all failure paths return `Fail`). Unlike `relay_smoke.rs:44-51` which legitimately skips on `probe_tcp` failure / missing `nak`, this scenario will FAIL (not skip) on any timeout. No masking. Good. The one caveat: because there's no network gate, see item 4.

**4. Network dependency — PASS for runnability (and this is *why* C was chosen).** The publish path does NOT contact `relay.primal.net` synchronously. `publish_raw_with_signer_via_nmp` → `dispatch_nmp_publish` only enqueues (`target: "Auto"`, returns `"queued"`); actual relay transport is fire-and-forget inside the kernel. The scenario's observables (`last_published_at`, `active_account`, `owned_podcasts`) are all in-process snapshot state. The only real socket is the local mock RSS feed (`mock_feed::start()`, 127.0.0.1). **So this scenario IS runnable in a sandboxed/no-network env and will not hang on a relay** — it is genuinely network-independent. That's the redeeming design point. The flip side is precisely the C problem: by deliberately avoiding any relay/sig observation to stay network-free, it lost the ability to assert signing actually happened.

**5. Scope — PASS.** PR touches exactly one file: `nipf4_publish.rs` (+323/-85). `mod.rs` already registered `nipf4_publish` (it's a rewrite of an existing registered scenario, not a new registration — the PR description's "mod.rs registration" is misleading but harmless; no mod.rs change was needed or made). No production code changed. Golden/codegen unaffected (no projection-type edits). Confirmed.

**Is this scenario RUN anywhere that gates regressions? — NO.** Confirmed: `.github/workflows/` (migration-lints.yml, test.yml, testflight.yml) contains no `headless` invocation; `test.yml` runs `cargo run --bin swift-codegen` only. `run_all` (mod.rs:48) includes `nipf4_publish` but is only invoked by the manual `--bin headless --features headless` run. **As of this PR the test is dead weight in CI** — it protects nothing automatically; it relies on a human remembering to run it.

📊 STATUS: Compiles/registers correctly and a sibling agent is confirming it passes at runtime. But its headline guarantee (C = "signing succeeded") is unsound, and it is not wired into any regression gate.

📁 CAPTURE: `last_published_at` is stamped at `host_op_publish.rs:159` *before and unconditionally of* the `publish_raw_with_signer_via_nmp` call at `:165`, and `register_podcast_signer_in_kernel` (`nmp_dispatch.rs:44`) has no error return — so `last_published_at` is not a valid proxy for "the kernel signed with the per-podcast key." Headless scenarios are not in CI (test.yml has no headless invocation).

➡️ NEXT (recommendations):
1. **(SHOULD-FIX) Make C assert on the signed event, not the stamp.** If the harness can observe the kernel's signed output (an `action_results` slot keyed by correlation_id, an outbox/published-events snapshot, or a kind:10154 event with `pubkey == podcast_pubkey_hex`), assert that the published event's `pubkey` equals `podcast_pubkey_hex` and `sig` is 128-char hex. That is the only assertion that actually proves the seam. If no such observable exists, file a follow-up to expose one (e.g., a test-only signed-event tap), because without it this scenario cannot distinguish "signed correctly" from "stamped and silently dropped."
2. **(SHOULD-FIX) Down-weight the doc claims for C and D.** The module doc (lines 33, 50-55) overstates C as proof of signing; correct it to "proves the handler reached the stamp" to avoid future false confidence.
3. **(SHOULD-FIX) Wire headless into CI** so the test isn't dead weight. It is feasible *because* the publish path is network-free (item 4) — but `relay_smoke` and any LLM scenarios in `run_all` are network/`nak`/Ollama-bound and will Skip or need gating. Recommended: add a CI job `cargo run -p nmp-app-podcast --bin headless --features headless` that treats `Skip` as non-failing and `Fail` as failing; the network-bound scenarios self-skip via `probe_tcp`, while `nipf4_publish` (network-free) runs and gates. Without this, the protection this PR adds is theoretical.
4. **(NIT) D's 600ms fixed sleep (line 339)** is flaky under CI load; prefer a `wait_for` on an episode-side observable if one exists, or accept it as best-effort.

📖 STORY EXPLANATION:
1. PR #444 rewrites one headless scenario to guard the per-podcast NIP-F4 signing seam that #436/#438 moved into the kernel (`AddSigner{make_active:false}` + `PublishRaw{signer_pubkey}`).
2. The scenario genuinely boots the real kernel and dispatches create/publish_show/publish_episode through the same production handlers — so the dispatch path is a true e2e exercise, not a null-app envelope check.
3. Assertions A (distinct pubkey), B (active account unchanged across all four stages), and E (idempotent re-register) are sound and would fail on real regressions.
4. The headline assertion C ("`last_published_at` stamped ⇒ signing succeeded") is unsound: the handler stamps it at `host_op_publish.rs:159` unconditionally, before the publish dispatch, and the register call cannot error — so C passes even if signing is silently broken or the register call is deleted.
5. That is false confidence — exactly the failure the review brief asked me to flag; C only catches an early-return before the stamp, not a signing regression.
6. The scenario never skips, so it won't mask failures (good), and the publish path is fire-and-forget with no synchronous relay contact, so it's runnable offline (good) — but that network-free design is precisely why it can't observe a real signature.
7. Scope is clean: one test-only file, no production changes, golden unaffected; and headless is NOT in any CI workflow, so today this test gates nothing automatically.
8. Verdict: fix C to assert on the signed event's pubkey/sig (or expose an observable for it), correct the overstated docs, and wire the network-free headless run into CI so the protection is real.

🎯 COMPLETED: Assertion C proves the stamp, not signing — false confidence; fix first.

VERDICT: FIX-FIRST — (SHOULD-FIX) Assertion C is not a valid proxy for "the kernel signed with the per-podcast key": `last_published_at` is stamped unconditionally at `host_op_publish.rs:159` before `publish_raw_with_signer_via_nmp` and `register_podcast_signer_in_kernel` has no failure return, so C passes even if signing is dropped — assert on the signed event's pubkey/sig instead (or expose an observable for it); (SHOULD-FIX) correct the overstated doc claims for C/D; (SHOULD-FIX) wire the network-free headless run into CI so this scenario actually gates regressions. A/B/E are sound and the scope/Skip/network behavior are clean — none of these are BLOCKERs to merging the test as-is, but the central guarantee is currently illusory.
