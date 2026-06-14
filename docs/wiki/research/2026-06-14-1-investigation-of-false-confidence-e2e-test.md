---
type: research-record
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
source_lines: 10481-10511
agent_attribution: subagent (Cycle-13 #2: per-podcast publish e2e test)
has_preregistered_criteria: true
has_method: true
has_structured_report: true
characterization: "Investigation of false-confidence e2e test assertion, identifying correct signing observable via sign-and-return seam, mutation-verified fix with PASS verdict"
captured_at: 2026-06-14T02:12:42Z
---

Investigation of false-confidence e2e test assertion, identifying correct signing observable via sign-and-return seam, mutation-verified fix with PASS verdict

---

User: [Agent task result: Agent "Cycle-13 #2: per-podcast publish e2e test" completed]
Done. Worktree is clean, the new PR is open. Here is the summary.

## Summary

The reviewer was right: assertion C in the merged PR #444 was false confidence. I investigated all the observables, found the one that genuinely proves signing, rewrote C/D against it, and mutation-verified the fix.

**The problem confirmed:** `host_op_publish.rs` stamps `last_published_at` + `rev.fetch_add` *unconditionally, before* the `PublishRaw` dispatch, and `register_podcast_signer_in_kernel` has no failure path. So the old C passed even with the register call deleted — it only caught an early return before the stamp, never the signing seam.

**Investigation (offline observables for a kind:10154/54 `PublishRaw`):**
- `action_results` — records `result_json: None` for a publish terminal. No signed bytes.
- publish-outbox projection — carries `event_id` but not `pubkey`/`sig`.
- raw-event observer (`nmp_app_register_raw_event_observer`) — fires only on store ingest, which never happens for a self-published event with no relay echo.
- `signed_events` sidecar — populated only by `SignEventForReturn`, not `PublishRaw`.
- `nostr_relay` capability — invoked only when relay targets resolve (`Auto` → NoTargets offline).

**The seam I used:** the D13 sign-and-return path (`nmp_app_sign_event_for_return`) is the one network-free seam that exposes a signed event's `pubkey` + `sig`. It resolves the named signer via the *identical* `sign_with_account_nonblocking(identity, pubkey, …)` call that `PublishRaw { signer_pubkey }` uses, so it directly guards the register→sign path. The `headless` feature already enables `nmp-ffi/test-support`, so the symbol links.

**New assertion C (headline):** after the publish dispatch registers the per-podcast signer, drive a sign-and-return naming that pubkey, read the signed event from the `signed_events` push projection (via a new `sign_tap.rs` update-frame callback), and assert `pubkey == podcast_pubkey_hex` (not the active account), valid 128-hex `sig`, 64-hex `id`, matching `kind` — for both kind:10154 and kind:54.

**Signed-event observable used:** `signed_events` push projection, decoded via `nmp_app_podcast_decode_update_frame` — `apps/nmp-app-podcast/src/bin/headless/sign_tap.rs` (`sign_for_return_blocking` + `assert_kernel_signs_with`).

**Mutation-check proof:** commenting out both `register_podcast_signer_in_kernel` calls in `host_op_publish.rs` made the scenario FAIL with `kernel sign-for-return failed: no signer for account … — add it first`. Production code restored.

**Real run output:** `nipf4_publish` PASSES on top of current `main`. The 4 other headless failures (`rss_subscribe`/`inbox_triage`/`comments`/`social`) are pre-existing and network-dependent (async-HTTP capability stub / live relay), unrelated.

**Other fixes:** D downgraded honestly to "handler ran end-to-end, not a signing proof"; E re-runs sign-and-return after the second registration; the flaky 600ms sleep replaced by the sign-and-return's deterministic wait; module docs corrected.

**Validation:** `cargo check --workspace` clean; `cargo test -p nmp-app-podcast --lib` 1286 passed.

**Note on the original PR:** #444 had already been squash-merged (commit `23b74ccf` on `main`) and its branch auto-deleted, so `--force-with-lease` on the old branch failed. I rebased the strengthening onto current `origin/main` as a new branch and opened **PR #446**: https://github.com/pablof7z/podcast-player/pull/446
