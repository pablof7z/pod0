---
type: research-record
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
source_lines: 9750-9816
agent_attribution: a0f4e5a031abc664f
has_preregistered_criteria: true
has_method: true
has_structured_report: true
characterization: "Structured adversarial code review of PR #439 with 7 pre-registered verification criteria (JNI correctness, decode contract, lifecycle, merge, additive, Rust additivity, tests), all PASS, verdict MERGE"
captured_at: 2026-06-13T21:20:44Z
---

Structured adversarial code review of PR #439 with 7 pre-registered verification criteria (JNI correctness, decode contract, lifecycle, merge, additive, Rust additivity, tests), all PASS, verdict MERGE

---

User: [Agent task result: Agent "Opus review PR #439 Android profiles" completed]
The voice server returned 404 (notification endpoint unavailable), but I attempted it. Here is the review.

---

📋 SUMMARY: Read-only review of PR #439 — wires Android to the kernel's profile-resolution seam so Nostr conversation participants render names/avatars. All 7 verification points checked against the actual code and the pinned `nmp-ffi` rev `6418a7a`.

🔍 ANALYSIS: I verified the Kotlin against the Rust JNI export, the Rust call against the actual pinned FFI signature, the decode field names against the kernel's own serializer and tests, and the claim/release lifecycle in both screens. The work is genuinely additive and matches the established conventions in the file.

⚡ ACTIONS: `gh pr view/diff`, `git diff origin/main...origin/feat/...`, `git show`, `git grep`, and reads of the pinned cargo git checkout. No mutating git commands. No files modified.

✅ RESULTS — point by point (PASS/FAIL with file:line):

**1. JNI correctness — PASS.**
- `android.rs:481` `Java_io_f7z_podcast_KernelBridge_nativeClaimProfile` and `:511` `..._nativeReleaseProfile`. Mangling is correct: package `io.f7z.podcast` → `io_f7z_podcast`, class `KernelBridge`, methods `nativeClaimProfile`/`nativeReleaseProfile`. These match the Kotlin `private external fun nativeClaimProfile(handle: Long, pubkeyHex: String, consumerID: String)` / `nativeReleaseProfile(...)` (KernelBridge.kt:383-384) — arity, types (jlong↔Long, jstring↔String), and names all align. No `UnsatisfiedLinkError` risk.
- Arg marshalling: `env.get_string` with `Err(_) => return`, `CString::new(...)` guards interior NUL bytes, `session_ref(handle)` null-guards the handle (android.rs:80). Same pattern as `nmpActionDispatch` (android.rs:446).
- `force=0` is correct: the real Rust `nmp_app_claim_profile(app, pubkey, consumer_id, force: c_int)` takes 4 args at rev `6418a7a` (timeline.rs:132-137); release takes 3 (timeline.rs:159-163). The call sites match exactly.
- No panic across FFI: both wrapped in `ffi_guard("...", || (), || {...})` which `catch_unwind`s (guard.rs:74). Unit fallback `|| ()` is correct for a void return.

**2. resolved_profiles decode contract — PASS.**
- `ResolvedProfile` (DomainFrames.kt) uses `@SerialName("display_name") val display` and `@SerialName("picture_url") val pictureUrl`. The kernel's `ProfileCard` derives plain `Serialize` (no rename_all) with fields `display_name`/`picture_url` (nmp-core types.rs:146,150), and the kernel test asserts `entry["display_name"] == "Claimed User"` (resolved_profiles_tests.rs:95). Names match the wire exactly. iOS `ResolvedProfile` CodingKeys confirm the same mapping (KernelIdentityProjection.swift:98-101).
- Top-level path: Android reads `projections["resolved_profiles"]` (DomainFrames.kt decode block). Kernel tests read `snapshot["projections"]["resolved_profiles"]` (resolved_profiles_tests.rs:51,88) and iOS reads `v.projections[...]` (KernelBridge.swift:191). Correct location.

**3. claim/release lifecycle — PASS.**
- List (NostrConversationsScreen.kt): `consumerID = "NostrConversationsScreen"`, `DisposableEffect(conversations)` captures `claimed = ...distinct()`, claims each, `onDispose` releases the *same captured* set. Re-keying on `conversations` releases the old captured set before claiming the new one — no leak, no stale over-release.
- Detail (NostrConversationDetailScreen.kt): `consumerID = "NostrConversationDetail:${rootEventId}"` (conversation-stable, distinct from the list's ID), `DisposableEffect(conversation.rootEventId)` captures `pubkeys` and releases the same set. List→detail→back cannot release each other's claims because the consumer IDs differ; the kernel refcounts per (pubkey, consumer). `distinct()` prevents double-claim within a frame.

**4. Additive merge / no rev gate — PASS.**
- `mergeFrames` (DomainFrames.kt): `snap.copy(resolvedProfiles = snap.resolvedProfiles + frames.resolvedProfiles)` — Kotlin map `+` is overlay (new keys win, existing survive). Guarded by `if (frames.resolvedProfiles.isNotEmpty())`, so a frame lacking the key never clobbers prior state with empty. Correct for an accumulating profile map with no per-domain rev. Matches iOS `mergeResolvedProfiles`.

**5. Rust purely additive — PASS.**
- `android.rs` only ADDS two `#[no_mangle]` fns and two names to the existing `use nmp_ffi::{...}` import. No kernel/projection code changed; the FFI symbols pre-existed at the pinned rev. No projection shape change → golden unaffected.

**6. Tests genuinely behavioral — PASS.**
- 6 new cases (DomainFrameWireTest.kt section 9) call the real `SnapshotCodec.decodeDomainFrames` / `mergeFrames` (DomainFrames.kt:312,379) via the real `envelope(...)` helper (test:34). They assert field mapping, null partials, additive old-key survival, same-key overwrite, coexistence with `podcast.social`, and absent-key empty-map. Not trivially-true — each asserts specific merge/decode semantics.

**7. Other real bugs — PASS.**
- Coil: `import coil.compose.AsyncImage` matches pinned `io.coil-kt:coil-compose:2.6.0` (build.gradle.kts:73) and the project convention (5 existing files). `AsyncImage(model=, contentDescription=, modifier=)` is the correct 2.x signature. Null/blank picture handled via `if (!pictureUrl.isNullOrBlank())` → falls back to `NostrAvatarPlaceholder`. `CircleShape` already imported (NostrConversationsScreen.kt:16).
- Detail call site: AppNavigation.kt:160-163 passes `snapshot` and `bridge`, both in `AppNavigation` scope (params at :61-62). Only call site — no stale-signature preview/test left.

📊 STATUS: Code is correct and internally consistent. No BLOCKER, no SHOULD-FIX. Two NITs.

NITs:
- **NIT (stale C header):** `App/Sources/Bridge/NmpCore.h:197` declares `nmp_app_claim_profile` with **3** params (no `force`), while the Rust source at the pinned rev has **4**. This header is iOS-only (Swift bridge) and does not affect the Android JNI shim (which links the Rust symbol directly), so it's harmless for this PR — but the Kotlin/Rust doc-comments point readers at `NmpCore.h` as the authority, and that header is out of date. Worth a follow-up to regenerate the header.
- **NIT (iOS push path is a no-op, not this PR's bug):** iOS `KernelIdentityProjection.from(domainFrames:)` always sets `resolvedProfiles: [:]` (KernelIdentityProjection.swift:177-190), so the iOS *push* path never actually surfaces resolved profiles — only mentioned because the PR cites iOS as the parity reference. Android's implementation (decoding `projections["resolved_profiles"]` directly) is actually *more* correct than the iOS push path it claims to mirror. No action needed for Android.

📁 CAPTURE: Android kotlinx-serialization needs explicit `@SerialName` for snake_case (no auto-convert, unlike iOS `convertFromSnakeCase`). Kernel `resolved_profiles` lives at `projections["resolved_profiles"]` (under `v` in the push envelope), fields `display_name`/`picture_url` from plain-`Serialize` `ProfileCard`. `nmp_app_claim_profile` is 4-arg (`force: c_int`) at rev `6418a7a`; the iOS `NmpCore.h` is stale at 3-arg. iOS push path leaves `resolvedProfiles` empty — Android's direct decode is more complete.

➡️ NEXT: Run the Gradle compile + unit tests locally before merge (see below). Optionally file a follow-up to regenerate `NmpCore.h`.

📖 STORY EXPLANATION:
1. The PR adds two Rust JNI entry points that forward to pre-existing `nmp_app_claim_profile`/`nmp_app_release_profile` FFI symbols — no new kernel code.
2. I confirmed the Rust calls match the *actual* pinned FFI signatures (4-arg claim, 3-arg release) by reading the cargo git checkout at rev `6418a7a`, not just the stale C header.
3. The JNI symbol names mangle correctly to the Kotlin `external fun` declarations, so there's no hidden `UnsatisfiedLinkError`.
4. The `@SerialName` decode contract is load-bearing on Android, and it matches the kernel's plain-serde field names `display_name`/`picture_url`, cross-checked against the kernel's own tests.
5. The claim/release lifecycle is refcounted with stable, distinct per-screen consumer IDs and captured release sets — no leaks, no over-release across list→detail→back.
6. The merge is a correct additive map overlay, guarded so empty frames never clobber accumulated profiles.
7. The 6 new tests genuinely exercise decode and merge semantics, and the Coil import + AppNavigation call-site args are all correct.
8. The code is sound, but Android Kotlin is not CI-gated and JNI/signature/call-site risks are exactly the class of error a compile catches — so a Gradle build is still required before merge.

**CRITICAL — is a Gradle compile still required?** YES, explicitly required. Static review confirms names/arities/imports line up, but Android Kotlin is not CI-gated here and the risk classes that only a compiler catches (JNI external-fun↔Rust export, the detail-screen new params, kotlinx serializer wiring) are present. Run, on the actual checkout, before merge:
`./gradlew :app:compileDebugKotlin :app:compileDebugUnitTestKotlin :app:testDebugUnitTest`
The PR description claims these already passed; treat that as unverified until reproduced locally. (Note the Rust JNI shim is NOT exercised by Gradle — it needs `cargo-ndk`/cargo build of `nmp-app-podcast` to confirm the 4-arg call compiles; the Rust signature match I verified by source, but a `cargo check` on the Android crate would close it definitively.)

VERDICT: MERGE — pending a local `:app:compileDebugKotlin` + `:app:compileDebugUnitTestKotlin` + `:app:testDebugUnitTest` green run (and ideally a `cargo check` of `nmp-app-podcast` for the JNI shim). The code is correct on every reviewed dimension; the gate is purely the uncompiled-Android-Kotlin / un-cargo-checked-Rust risk, not any defect I found.

🎯 COMPLETED: PR 439 passes all seven checks; merge after a local Gradle compile.
