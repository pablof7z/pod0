---
type: research-record
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
source_lines: 11538-11633
agent_attribution: Opus review PR #455 Android NIP-46
has_preregistered_criteria: true
has_method: true
has_structured_report: true
characterization: "Adversarial code review of PR #455 (Android NIP-46 remote signer): 7 criteria evaluated — PASS on broker-init safety, JNI mangling/marshalling/memory, FFI signature match, scope, and URI tests; BLOCKER on async handshake reactivity (identity projection never emits 'bunker'/'nip46' mode, causing spinner-forever UX)"
captured_at: 2026-06-14T04:44:09Z
---

Adversarial code review of PR #455 (Android NIP-46 remote signer): 7 criteria evaluated — PASS on broker-init safety, JNI mangling/marshalling/memory, FFI signature match, scope, and URI tests; BLOCKER on async handshake reactivity (identity projection never emits 'bunker'/'nip46' mode, causing spinner-forever UX)

---

User: [Agent task result: Agent "Opus review PR #455 Android NIP-46" completed]
Voice endpoint is down (404), noted. Here is the review.

---

📋 SUMMARY: Adversarial read-only review of PR #455 (Android NIP-46 remote signer). The FFI/JNI/init plumbing is correct, but the reactive completion signal the screens depend on is never emitted — a BLOCKER.

🔍 ANALYSIS: Verified every claim against iOS (`origin/main`) and the pinned NMP checkout (`6418a7a`, the rev in `Cargo.lock`).

---

## 1. `nmp_signer_broker_init` addition — PASS (with a note)

`apps/nmp-app-podcast/src/android.rs:177` adds `nmp_signer_broker_init(app)` in `nativeNew`, right after `nmp_external_signer_init(app)` and before the `Session` is boxed / before `nativeStart`.

- (a) **Called exactly once** — it sits in `nativeNew`, which runs once per kernel. And `nmp_signer_broker_init` itself is idempotent: `signer_broker.rs:41` uses `GLOBAL_BROKER.get_or_init(...)` (an `OnceLock`), so even a double-call is a no-op. PASS.
- (b) **No conflict with NIP-55** — broker (`signer_broker` module) and external-signer (`external_signer` module) are fully independent modules with separate process-global state. Broker is genuinely NIP-46-only; this is purely additive and cannot break the working Amber/NIP-55 flow. Confirmed NIP-55 worked without broker init before because `nmp_app_signin_nip55` routes through `external_signer`, not the broker. PASS.
- (c) **Order** — iOS (`KernelBridge.swift:69`) calls `nmp_signer_broker_init(raw)` immediately after `nmp_app_new()`, before storage-path and projection registration. Android calls it after `nmp_app_new` + `set_update_callback` + `podcast_register` + `external_signer_init`, before `nativeStart`. The broker only needs `app.actor_sender()` (available immediately post-`nmp_app_new`), so Android's later-but-pre-start placement is safe. PASS.
- **Note (not a bug):** iOS does not call `nmp_external_signer_init` in its `init()` at all; Android does. That's a pre-existing platform divergence, out of scope for this PR.

## 2. JNI correctness — PASS

`android.rs:999/1027/1051`. Mangled names `Java_io_f7z_podcast_KernelBridge_nativeSignInBunker` / `nativeCancelBunkerHandshake` / `nativeNostrconnectUri` exactly match the Kotlin `private external fun` declarations at `KernelBridge.kt:79-81` (package `io.f7z.podcast`, class `KernelBridge`). All three wrapped in `ffi_guard(...)` (no panic crosses FFI), guarded by `session_ref` null-check, `env.get_string` has an `Err(_) => return` guard. `nativeNostrconnectUri` correctly does `CStr::from_ptr` → owned String → `nmp_free_string(uri_ptr)` → `env.new_string(...).into_raw()` — no leak, no UAF (frees the C buffer after copying, matching the existing `nativeClaimProfile` pattern at `android.rs:130`). `make_active: jint` → passed as integer literal `1`/`0`, coerced to the FFI's `u8` param. PASS.

## 3. FFI signatures match real NMP — PASS

Against pinned `6418a7a`:
- `nmp_app_signin_bunker(app: *mut NmpApp, uri: *const c_char, make_active: u8)` — `identity.rs:132`. ✓
- `nmp_app_cancel_bunker_handshake(_app: *mut NmpApp)` — `signer_broker.rs:88` (arg ignored; uses `GLOBAL_BROKER`). ✓
- `nmp_app_nostrconnect_uri(app, relay_url: *const c_char, callback_scheme: *const c_char) -> *mut c_char` — `signer_broker.rs:99`; caller frees via `nmp_free_string`. ✓

All four symbols are `pub use`-re-exported from `nmp_ffi` (`lib.rs:114-117` identity, `lib.rs:154-156` signer_broker), so the Android `use nmp_ffi::{...}` import block resolves — no link error. PASS.

## 4. Async handshake reactivity — **BLOCKER (FAIL)**

This is the implementer's flagged gap, and it is real. The screens gate completion on:
```kotlin
val isPaired = account?.mode?.lowercase()?.let { it == "bunker" || it == "nip46" || it == "nip-46" } ?: false
```
(`RemoteSignerScreen.kt`, `NostrConnectScreen.kt`).

But the podcast app's identity projection (`apps/nmp-app-podcast/src/ffi/snapshot_identity.rs`) only ever emits **two** `mode` values:
- `MODE_LOCAL_KEY = "local_key"` (line 7) when the kernel-active hex matches the app-owned local key,
- `MODE_NIP55 = "nip55"` (line 10) for **every** external signer — i.e. any kernel-active account whose secret is NOT in the app's `IdentityStore`.

A NIP-46 bunker account is kernel/broker-owned, so it has no matching local secret → it resolves through `external_account_summary` (`snapshot_identity.rs:131`) → `mode = "nip55"`. **The projection never emits `"bunker"`, `"nip46"`, or `"nip-46"`.**

Consequence: on a successful bunker handshake, `activeAccount.mode == "nip55"`, so `isPaired` stays `false`. `NostrConnectScreen` never auto-navigates back; `RemoteSignerScreen` never clears `isConnecting` and never shows `ConnectedSection`. **The user sees a spinner forever even though sign-in actually succeeded.** (The kernel-side handshake/sign-in itself is fine; only the UI completion signal is dead.)

This is consistent with iOS doctrine, which explicitly warns against exactly this approach: `KernelIdentityProjection.swift:144-146` — *"Reads `signer_is_remote` directly per NMP doctrine — never string-matches on `signer_kind`."* iOS detects connection via a dedicated `signer_is_remote` boolean and via `connectRemoteSigner` being an awaited async call, not by string-matching a mode token. The wire `signer_kind` token is documented "Diagnostic only" (`KernelIdentityProjection.swift:31`) and is `"nip46"`/`"nip55"`/`"local"` — never `"bunker"`.

Note the pre-existing `ModeBadge` (`IdentityScreen.kt:269`) has the same `"bunker"/"nip46"` dead branch — a latent bug that predates this PR — but this PR newly makes screen *reactivity* depend on it, promoting it from a cosmetic mislabel to a functional dead-end.

**Fix options:** either (a) extend the app projection to distinguish nip46 from nip55 and emit a `"nip46"`/`"bunker"` mode (kernel `signer_kind()` already returns `"nip46"`; the app's `external_account_summary` flattens it to `"nip55"`), or (b) have the screens treat "any external signer became active while we were connecting" (i.e. transition into `nip55`/external from not-signed-in) as success, or surface the kernel's `bunker_connection_state` / `bunkerHandshake` projection. Option (a) is the doctrinally correct one and also fixes the pre-existing badge mislabel.

## 5. Scope — PASS

Diff touches only `android/**` (build.gradle.kts, KernelBridge.kt, AppNavigation.kt, IdentityScreen.kt, two new screens, one new test) + `apps/nmp-app-podcast/src/android.rs`. No iOS, no kernel logic, no Rust outside the JNI/init. ZXing Core `3.5.3` is encoder-only (`QRCodeWriter`), reasonable for the nostrconnect QR. PASS.

## 6. URI validation tests — PASS (with a caveat)

The 10 `Nip46UriValidationTest` cases are behavioral and meaningfully gate `bunker://`/`nostrconnect://` prefix + min-length (reject nsec, reject nostrconnect-as-bunker, reject bare scheme, accept 64-hex host). PASS.

**Caveat (SHOULD-FIX):** the tests exercise top-level helpers `isPlausibleBunkerUri` / `isPlausibleNostrconnectUri` defined *in the test file itself* (`Nip46UriValidationTest.kt:931/942`). The production screen (`RemoteSignerScreen.kt:706`) uses its own inline `trimmed.startsWith("bunker://")` check, NOT these helpers. So the tests validate a parallel reimplementation, not the shipped code path — they can pass while the real gate drifts. Extract the helpers into production and have both the screen and the tests call them.

## 7. Other real bugs

- **NIT:** `NostrConnectScreen` has an "I scanned it — waiting for approval" button that sets `phase = Connecting` purely client-side; combined with the #4 blocker, once you tap it there's no path back to `Ready` and no real completion — reinforces the spin-forever UX. Resolves naturally once #4 is fixed.
- **NIT:** `makeQrBitmap` swallows all exceptions to `null` (`catch (_: Exception)`), which is the intended D6 degrade, fine — but the `Error` state it produces ("Failed to generate QR code") only offers "Try again", which re-runs the same generation; acceptable.
- No `pub use` / link errors, no missing symbol, no obvious cancel-during-pending UAF (cancel just calls `broker.cancel()` on the global broker; `DisposableEffect.onDispose` cancel is idempotent per `signer_broker.rs:88`).

---

⚡ ACTIONS: Read full `gh pr diff 455`, cross-checked iOS `KernelBridge.swift`/`RemoteSignerView.swift`/`KernelIdentityProjection.swift`, read pinned NMP `signer_broker.rs`/`identity.rs`/`lib.rs` at rev `6418a7a`, and traced the app's `snapshot_identity.rs` mode derivation.

✅ RESULTS: Init order, JNI mangling/marshalling/memory, FFI signatures, re-exports, scope, and tests all PASS. One BLOCKER: the snapshot never carries a bunker/nip46 mode, so the handshake-complete signal the screens poll for is unreachable.

📊 STATUS: Statically reviewed, read-only. No files modified. No shared-root mutation. Live NIP-46 handshake is NOT statically verifiable — it needs a real Android device + a real signer (Amber / nsec.app) to confirm the end-to-end pairing; what I verified is compile/link correctness, FFI ABI match, and the (broken) reactive completion contract.

📁 CAPTURE: Bunker/NIP-46 active accounts surface as `mode == "nip55"` (not `"bunker"`/`"nip46"`) because `apps/nmp-app-podcast/src/ffi/snapshot_identity.rs` flattens every external signer to `MODE_NIP55`. iOS deliberately reads `signer_is_remote` (boolean), never string-matches `signer_kind`. Any Android UI gating on `mode == "bunker"` is dead (including the pre-existing `ModeBadge`).

➡️ NEXT: Fix #4 (emit a distinct nip46/bunker mode from the app projection, or gate on `signer_is_remote`-equivalent), then extract the URI validators into production (#6 caveat). Re-test on-device with a real signer.

📖 STORY EXPLANATION:
1. The riskiest change — adding `nmp_signer_broker_init` to Android `nativeNew` — is safe: it's idempotent (`OnceLock`), called once, and the broker is a separate module from the NIP-55 path so it can't break the working Amber flow.
2. The three new JNI entry points are mangled correctly, marshal their args with proper error guards, free the returned C string without leaking, and never let a panic cross FFI.
3. The FFI signatures the JNI calls exactly match the pinned NMP rev `6418a7a`, and all four symbols are `pub use`-re-exported, so there's no link error.
4. The fatal flaw is reactivity: the screens decide "connected" by checking `activeAccount.mode == "bunker"/"nip46"`.
5. But the podcast app's identity projection only ever emits `"local_key"` or `"nip55"` — a bunker account, being kernel-owned, flattens to `"nip55"`.
6. So a successful handshake never flips `isPaired` true; the screen spins forever despite sign-in actually working.
7. iOS proves the correct pattern: it reads a dedicated `signer_is_remote` boolean and explicitly warns "never string-match on signer_kind."
8. Everything else (scope, ZXing dep, tests) is sound — but the core user-visible flow is broken until the projection emits a distinguishable mode or the screens gate on the remote-signer boolean.

🎯 COMPLETED: One blocker — bunker handshake never surfaces as connected; everything else passes.

VERDICT: FIX-FIRST — Finding #4 is a BLOCKER: the Android screens gate completion on `activeAccount.mode == "bunker"/"nip46"`, but the app projection (`snapshot_identity.rs`) only emits `"local_key"` or `"nip55"`, so a successful bunker handshake renders as `"nip55"` and the UI spins forever / never shows Connected. Plus SHOULD-FIX #6 (tests exercise test-file-local validators, not the shipped screen path). The init plumbing, JNI, FFI ABI, re-exports, and scope are all correct. The live NIP-46 handshake itself can only be confirmed on a real Android device with a real signer; the static layer that *is* verifiable shows the completion signal is unreachable as written.
