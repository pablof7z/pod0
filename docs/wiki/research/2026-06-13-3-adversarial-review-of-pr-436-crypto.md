---
type: research-record
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
source_lines: 9261-9319
agent_attribution: Opus review PR #436 crypto retirement
has_preregistered_criteria: true
has_method: true
has_structured_report: true
characterization: "Adversarial review of PR #436 (crypto retirement) against seven pre-registered checks, verdict: MERGE"
captured_at: 2026-06-13T19:33:08Z
---

Adversarial review of PR #436 (crypto retirement) against seven pre-registered checks, verdict: MERGE

---

User: [Agent task result: Agent "Opus review PR #436 crypto retirement" completed]
(Voice endpoint returned 404 — server not running. Proceeding with the written report.)

---

📋 SUMMARY: Adversarial read-only review of PR #436 (retire app-side per-podcast crypto, route through kernel `signer_pubkey` seam). Verified against the pinned NMP rev `6418a7a`. All seven checks evaluated; the architecture is correct and the NIP-09 semantics change is a clean equivalent, not a regression.

🔍 ANALYSIS — check-by-check, with file:line:

**1. No app-side crypto remains — PASS.**
`git grep` on the PR head finds zero code occurrences of `sign_event`, `build_auth_event`, `auth_event_tags`, `sign_with_keys`, `upload_to_blossom` (code), `resolve_episode_tags` (code), `event_id_from_json`. Remaining hits are all legitimate: `blossom.rs:14` (doc comment), `host_op_publish_tests.rs:10` (doc comment), `nostr_episodes.rs:309` (`audio_sha256_hex: None` — a struct field, not the deleted fn), and `store/podcast_keys.rs:241` `SecretKey::from_slice` inside `derive_pubkey_hex` (pubkey derivation, not signing — explicitly allowed by your rule). `hex = "0.4"` removed from `apps/nmp-app-podcast/Cargo.toml` (diff line 17); `sha2`/`base64` correctly retained for other paths. All `nostr::EventBuilder/Keys/SecretKey` imports gone from `host_op_publish.rs`, `host_op_publish_lifecycle.rs`, `blossom.rs`.

**2. Signer registration correctness + idempotency — PASS.**
`register_podcast_signer_in_kernel` (`nmp_dispatch.rs:49`) calls `nmp_app_signin_nsec(app, secret, 0)` → `ActorCommand::AddSigner{LocalNsec, make_active:false}` (`identity.rs:103-119`). The `add_signer` reducer (`identity.rs:948`) on the LocalNsec branch calls `identity.add(keys)` and, with `make_active=false`, **never touches `identity.active`**. `IdentityRuntime::add` (`identity.rs:513`) keys by pubkey hex: the `!contains_key` guard means re-registration does **not** push a duplicate `order` entry, and `keys.insert` overwrites the slot with the identical key. Re-registering on a second publish is a clean no-op — no panic, no duplicate, no active switch. **FIFO confirmed:** both `signin_nsec` and `nmp_app_dispatch_action` call `app.send_cmd` → single MPSC `self.tx.send` (`nmp-ffi/src/lib.rs:1332`), drained by one actor thread. The app calls register synchronously before the publish dispatch on the same host thread, so `AddSigner` is enqueued and dequeued first. LocalNsec registration is fully synchronous (no async broker round-trip), so the key is present when the publish dequeues.

**3. PublishRaw signer_pubkey threading — PASS.**
`publish_raw_with_signer_via_nmp` (`nmp_dispatch.rs`) emits `{"PublishRaw":{kind,tags,content,target:"Auto","signer_pubkey":<pubkey_hex>}}`. Crucially the code passes the **derived pubkey** (`pubkey_hex` from `keys.pubkey_hex(...)`) as `signer_pubkey`, and the **secret** (`secret_hex`) only to the registration call — correctly separated. Kernel resolves via `sign_with_account_nonblocking(identity, pubkey, unsigned)` (`identity.rs:793`) which looks up `identity.keys.get(pubkey)`. `parse_secret` (`identity.rs:1535`) accepts 64-char hex via `SecretKey::from_hex`, confirming the registration-side hex acceptance. The pubkey used for resolution is the right derived x-only pubkey, not the secret.

**4. NIP-09 deletion change — PASS (clean equivalent, NOT a regression).** This was the hardest item; I checked `origin/main` directly.
- (a) **Episodes (kind:54) were NEVER NIP-09-deleted** — not in old code, not in new. The old `delete_owned` (main `host_op_publish_lifecycle.rs:210-300`) emitted a single deletion with `["e",show_event_id]` + `["k","10154"]` — show only. So the PR does **not** regress episode deletion; that gap is pre-existing. (Episodes are still removed from the local store via `remove_podcast_and_episodes`.)
- (b) **No over-deletion.** A per-podcast key authors exactly one kind:10154. `["k","10154"]` deletes all kind:10154 from that single-purpose pubkey = exactly that one show. No collateral.
- (c) **`k`-tag NIP-09 is valid** and honored by NIP-09-supporting relays (kind:5 with `["k","10154"]` is the standard kind-targeted deletion form). The e-tag→k-tag switch is forced because the kernel signs and does not return the event id at dispatch time — a reasonable and correct substitute. New code emits exactly `vec![vec!["k", KIND_SHOW]]` (diff line 864).
- NIT: the docstring says deletion "targets kind:10154/54" but the code only emits `["k","10154"]`, never `["k","54"]`. Misleading comment, not a bug.

**5. Test coverage — PASS (behavioral), with one boundary limitation.**
`create_owned_then_publish_show_dispatches_via_kernel` and `publish_episode_dispatches_via_kernel` assert `status=="signed"`, assert response `pubkey_hex` matches the registered key, and **explicitly assert `event_json`/`sig` are absent** — a regression to app-side signing would fail these. `delete_owned_removes_row_key_and_state` asserts `deletion_event_id` absent + `deletion_status=="signed"`. These are real assertions, not trivially-true. No golden/snapshot fixtures are touched (publish is a write path, not a projection) — consistent with the byte-identical claim. **Limitation (NIT):** with a null `app`, `publish_raw_with_signer_via_nmp` returns `"signed"` before touching FFI, so these tests prove the app-side envelope shape but do **not** integration-test register→resolve→sign against a live kernel. The kernel's own suite covers signer_pubkey threading (`publish/action/tests.rs:207 execute_publish_raw_threads_signer_pubkey_onto_actor_command`), so coverage exists upstream, just not end-to-end in this repo.

**6. D-doctrine (D13) — PASS.**
`secret_hex`/`secret_bytes` are never logged (no `println!`/`eprintln!`/`log::` references them in any changed file). The secret crosses into the kernel exactly once via `nmp_app_signin_nsec`, which immediately wraps it in `zeroize::Zeroizing` (`identity.rs:112`). D13's intent — no app-side *signing* with raw bytes — is satisfied; the key now lives in the kernel keystore. The secret still transits the FFI as hex (inherent to importing a key into the kernel; this is NMP's standard import path, not app-side crypto).

**7. Real bugs / races — none material.**
The race-the-dispatch concern is resolved by FIFO single-queue ordering (check #2). The silent-no-op concern is resolved: if the signer were somehow unresolved, `publish_unsigned_event` (`publish.rs:139-146`) routes the `Err` to `fail_publish(kernel, reason, correlation_id)` — a recorded terminal failure, **not** a silent drop — and the `signer_pubkey.is_some()` path correctly **skips** the active-account guard (`publish.rs:126`) so a non-active per-podcast publish succeeds with no active account. Bonus correctness win: wire `created_at` is now stamped by the kernel (`dispatch.rs:811 ctx.kernel.now_secs()`); the app's `Utc::now()` is used only for the local `last_published_at` UI field — strictly more D7/D9-compliant than the old `custom_created_at`.

⚡ ACTIONS: Read full PR diff (1495 lines); inspected pinned NMP rev `6418a7a` reducers (`add_signer`, `IdentityRuntime::add`, `parse_secret`, `sign_with_account_nonblocking`, `publish_unsigned_event`, `PublishRawEvent` dispatch arm, `nmp.blossom.upload` action, `nmp_app_signin_nsec` FFI, `send_cmd` queue); diffed old vs new `delete_owned`; grepped PR head for residual crypto and secret logging.

✅ RESULTS: All 7 checks PASS. Two NITs and one SHOULD-FIX-lite (documentation/cleanup, no functional defect).

📊 STATUS: Mergeable. No BLOCKERs. No SHOULD-FIX correctness issues.

📁 CAPTURE: PR #436 routes per-podcast NIP-F4 signing through kernel `AddSigner{make_active:false}` + `PublishRaw{signer_pubkey}` + `nmp.blossom.upload{signer_pubkey}`. Idempotency guaranteed by `IdentityRuntime::add` pubkey-keyed dedup (identity.rs:513). FIFO via single MPSC actor queue. NIP-09 episode (kind:54) deletion never existed in this repo — pre-existing gap, not a PR regression.

➡️ NEXT (optional cleanups, none blocking):
- NIT: remove the now-dead `publish_via_nmp` (pre-signed-event variant) in `nmp_dispatch.rs:151` — zero callers remain; its docstring falsely claims the NIP-09 path still uses it.
- NIT: fix the `delete_owned` docstring — it says deletion targets "kind:10154/54" but the code emits only `["k","10154"]`.
- SHOULD-FIX-lite: consider a tracked follow-up for (a) end-to-end register→sign coverage against a live kernel, and (b) whether owned-podcast deletion *should* also emit `["k","54"]` to clean up published episode events on relays (pre-existing product gap surfaced by this review).

📖 STORY EXPLANATION:
1. The PR's goal is to delete all hand-rolled secp256k1 signing and BUD-02 Blossom upload from the app, routing per-podcast NIP-F4 keys through the kernel — and the grep proves that deletion is complete, with only legitimate pubkey-derivation remaining.
2. The registration path is genuinely idempotent: the kernel keys signers by pubkey hex, so re-registering the same key on a second publish overwrites itself without duplicating the roster or flipping the active account.
3. Ordering is safe because registration and the publish dispatch ride the same single FIFO actor queue, and local-key registration is synchronous, so the signer is always present when the sign-time lookup fires.
4. The signer is selected correctly — the derived pubkey goes to `signer_pubkey`, the secret goes only into the one-time kernel import, and the kernel resolves the named key exactly as it would the active account.
5. The scariest item, the NIP-09 e-tag→k-tag change, is a clean equivalent: a per-podcast key authors exactly one kind:10154 show, so kind-targeted deletion removes precisely that event with no over-deletion.
6. Episode (kind:54) NIP-09 deletion is unchanged because it never existed — checking origin/main confirmed the old code only ever deleted the show event, so this is a pre-existing gap, not a regression introduced here.
7. No secret bytes are logged, the key is Zeroized on kernel import, and an unresolved signer would produce a recorded terminal failure rather than a silent no-op — the failure modes are observable.
8. Net result: the architecture is correct, D13 intent is satisfied, created_at handling actually improves, and the only follow-ups are cosmetic dead-code/docstring cleanups — so this merges.

🎯 COMPLETED: PR 436 verified sound across all seven checks; merge with minor cleanups.

VERDICT: MERGE
