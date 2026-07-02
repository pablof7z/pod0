//! Scenario: cross-account social-state leak guard.
//!
//! ## What it validates
//!
//! `clear_for_account_switch` (wired in `register.rs` via
//! `register_identity_change_observer`) is the guard that prevents account A's
//! social data from bleeding into account B's session.  Unit tests for this
//! function live in `state/social.rs:469` and `state/social.rs:490`, but they
//! test `SocialState` in isolation — no FFI layer, no registration plumbing.
//!
//! This scenario closes the coverage gap by exercising the full path:
//!
//! 1. Import account A (the standard headless test nsec).
//! 2. Inject synthetic social data (follow-list + an inbound agent note) via
//!    the `headless_inject_*` methods on `PodcastHandle` (compiled only under
//!    `--features headless`), simulating what would have arrived reactively
//!    from the relay in a live session.
//! 3. Assert data is present (pre-switch sanity check).
//! 4. Drive `clear_for_account_switch` via `headless_trigger_account_switch_clear` —
//!    this calls the exact same method that `register_identity_change_observer`
//!    fires in production.
//! 5. Assert that `social_slot` and `agent_notes` are cleared.
//!
//! ## Why the headless harness cannot drive a full kernel account switch
//!
//! The `register_identity_change_observer` callback fires when the NMP kernel's
//! `active_account_handle` changes — this only happens on a NMP sign-in flow
//! (Nostr keypair / Amber / Bunker handshake through the kernel actor).
//! The podcast app's `podcast.identity.ImportNsec` action only updates the
//! LOCAL `IdentityStore`; it does not change the kernel's active-account slot
//! and therefore does not fire the identity-change hook.  A full kernel
//! account switch requires a real relay connection and a signer handshake that
//! is not supported in the headless harness.
//!
//! As a consequence:
//! - Step 4 invokes `clear_for_account_switch` DIRECTLY via the headless test
//!   surface (same code, different call site).
//! - The scenario asserts the leak guard's post-condition (slots cleared) with
//!   real slot Mutexes, real data, and a real call to the function under test.
//! - The wiring between `register_identity_change_observer` and
//!   `clear_for_account_switch` is separately verified by the unit tests in
//!   `state/social.rs` and the `register.rs` code path inspection.
//!
//! ## Coverage guarantee
//!
//! The assertions in step 5 will FAIL if `clear_for_account_switch` regresses
//! (e.g. stops clearing one of the slots).  This scenario runs in the full
//! live binary against the real `PodcastHandle` + `SocialState` composition.
//!
//! ## No relay required
//!
//! Social data is injected directly via `PodcastHandle::headless_inject_*`.
//! The `clear_for_account_switch` is called via
//! `PodcastHandle::headless_trigger_account_switch_clear`.  No relay, no LLM,
//! fully offline.

use nmp_app_podcast::ffi::{ContactSummary, SocialSnapshot};
use nmp_app_podcast::{CachedAgentNote, PodcastHandle};
use nmp_native_runtime::NmpApp;
use serde_json::json;

use crate::fixtures;
use crate::harness::{dispatch, snapshot, wait_for};
use crate::scenarios::ScenarioResult::{self, Fail, Pass};

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // ── Step 1: Import account A ──────────────────────────────────────────────
    //
    // Fast-path: if identity_import ran before us and A's key is already
    // active, skip the re-dispatch (the kernel deduplicates identical
    // dispatches within its TTL window, so re-importing the same nsec may be
    // silently dropped — making wait_for time out on a rev change that never
    // comes).
    let has_account_a = snapshot(handle)
        .as_ref()
        .and_then(|u| u.active_account.as_ref())
        .map(|a| a.pubkey_hex == fixtures::HEADLESS_TEST_PUBKEY_HEX)
        .unwrap_or(false);

    if !has_account_a {
        let res = dispatch(
            app,
            "podcast.identity",
            json!({"type": "ImportNsec", "nsec": fixtures::HEADLESS_TEST_NSEC}),
        );
        if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
            return Fail(format!("ImportNsec (account A) rejected: {err}"));
        }
        match wait_for(handle, 5_000, |u| {
            u.active_account
                .as_ref()
                .is_some_and(|a| a.pubkey_hex == fixtures::HEADLESS_TEST_PUBKEY_HEX)
        }) {
            Err(e) => return Fail(format!("account A identity not set: {e}")),
            Ok(_) => {}
        }
    }

    // ── Step 2: Inject synthetic social data for account A ───────────────────
    //
    // `PodcastHandle::headless_inject_*` methods are compiled only under
    // `--features headless`.  They reach directly into `state.social` to
    // populate the same slots that `FollowListObserver` and `AgentNotesObserver`
    // would write in a live relay session — without needing a relay connection.
    //
    // SAFETY: `handle` is a valid non-null pointer from `nmp_app_podcast_register`
    // and is not freed until after `run_all` returns.
    let h = unsafe { &*handle };

    h.headless_inject_social_snapshot(SocialSnapshot {
        following: vec![ContactSummary {
            npub: "npub1xtscya34g58tk0z605fvr788k263gsu6cy9x0mhnm87echrgufzsevkk5s".to_string(),
            pubkey_hex: "32e1827635450ebb3c5a7d12c1f8e7b2b514439ac10a67eef3d9fd9c5c68e245"
                .to_string(),
            display_name: None,
            picture_url: None,
        }],
        following_count: 1,
        approved_pubkeys: Vec::new(),
        blocked_pubkeys: Vec::new(),
    });

    h.headless_inject_agent_note(CachedAgentNote {
        id: "account-a-note-synthetic-1".to_string(),
        author_hex: "32e1827635450ebb3c5a7d12c1f8e7b2b514439ac10a67eef3d9fd9c5c68e245"
            .to_string(),
        author_npub: "npub1xtscya34g58tk0z605fvr788k263gsu6cy9x0mhnm87echrgufzsevkk5s"
            .to_string(),
        content: "account A synthetic agent note".to_string(),
        created_at: 1_700_000_000,
        root_event_id: None,
    });

    // ── Step 3: Pre-switch sanity check ──────────────────────────────────────
    if h.headless_social_snapshot().is_none() {
        return Fail(
            "pre-switch sanity: social_slot must be Some after injection".into(),
        );
    }
    if h.headless_agent_notes_len() == 0 {
        return Fail(
            "pre-switch sanity: agent_notes must be non-empty after injection".into(),
        );
    }

    // ── Step 4: Drive clear_for_account_switch ────────────────────────────────
    //
    // This calls the exact method that `register_identity_change_observer` fires
    // in production (register.rs:363).  The headless surface invokes it directly
    // because a full kernel account switch (needed to trigger the hook via the
    // identity-change observer) requires a relay sign-in flow not supported in
    // the offline headless harness.  See module-level doc for the full analysis.
    h.headless_trigger_account_switch_clear();

    // ── Step 5: Assert social state was cleared ───────────────────────────────
    //
    // Both assertions will FAIL if clear_for_account_switch regresses.
    if let Some(snap) = h.headless_social_snapshot() {
        return Fail(format!(
            "cross-account leak: social_slot still has data after clear_for_account_switch \
             (following_count={}). The method did not clear social_slot.",
            snap.following_count
        ));
    }

    let notes_after = h.headless_agent_notes_len();
    if notes_after != 0 {
        return Fail(format!(
            "cross-account leak: agent_notes still has {notes_after} entries \
             after clear_for_account_switch. The method did not clear agent_notes."
        ));
    }

    Pass
}
