//! Scenario: dispatch `publish_profile` (kind:0) and assert the seam is intact.
//!
//! ## What this validates
//!
//! The `podcast.social` `publish_profile` action routes a kind:0 metadata
//! event through the signing kernel. The full path under test:
//!
//! ```text
//! dispatch("podcast.social", {"op":"publish_profile","name":"...","display_name":"...","picture":"..."})
//!   → SocialActionModule::execute
//!   → PodcastHostOpHandler::handle_social_action (SocialAction::PublishProfile)
//!   → social_publish_handler::handle_publish_profile
//!     └─ require_signed_in guard
//!     └─ build_profile_fields
//!     └─ nmp_dispatch::publish_profile_via_nmp   ← the seam under test
//!          → nmp.publish { PublishProfile { fields } }
//!          → kernel signs + queues the kind:0 event
//! ```
//!
//! ## Dispatch acceptance vs. handler result
//!
//! `nmp_app_dispatch_action` (the underlying FFI) validates the action, mints a
//! `correlation_id`, and enqueues an `ActorCommand::DispatchHostOp`. It returns
//! `{"correlation_id":"..."}` immediately on acceptance, or `{"error":"..."}` on
//! synchronous rejection (unknown namespace, serde decode failure, etc.). The
//! handler's own `{"ok":true,"status":"queued"}` result is asynchronous and
//! rides the `action_results` sidecar — it is NOT in the dispatch return value.
//!
//! This means:
//! * Acceptance is proven by `correlation_id` present in the dispatch return.
//! * Synchronous rejection (malformed payload, unknown op) is detected by
//!   `error` in the dispatch return.
//! * Handler-level failure (`require_signed_in` guard, etc.) is async — it lands
//!   in `action_results[correlation_id]` and is not observable here without
//!   polling the snapshot's action_results slot.
//!
//! ## Network-free assertion rationale
//!
//! `publish_profile_via_nmp` enqueues a `PublishProfile` command in the NMP
//! actor queue. The kernel signs the event locally with the active-account
//! secret and schedules it for relay delivery. Relay publication is
//! asynchronous and external; however, after `handle_publish_profile` succeeds
//! it self-applies the published fields to the local `IdentityStore` (optimistic
//! self-apply), and `social_actions.rs` bumps `Domain::Identity` so the next
//! push frame re-emits with an updated `AccountSummary`. Therefore:
//!
//! * **Seam assertion**: dispatch is accepted (`correlation_id` present, no
//!   synchronous `error`) — proves `SocialActionModule` decoded the payload and
//!   routed it to the actor.
//! * **Mutation-sanity guard**: a malformed payload (missing required `name`
//!   field) must NOT produce a `correlation_id` — it must be rejected at serde
//!   decode with an `error` field. This makes the success path meaningful.
//! * **Self-apply reflection**: after dispatch, `AccountSummary.display_name`
//!   must equal the published value within 2 s — proves the self-apply→bump-rev
//!   chain fires and the push frame re-emits. Without the self-apply this
//!   assertion would FAIL (the old behavior left `display_name` as `None` forever).
//!
//! This scenario is FULLY network-free (no relay connection needed). It always
//! runs and always returns Pass or Fail — never Skip — because it only exercises
//! the local signing pipeline.
//!
//! ## What the Android `EditProfile` slice depends on
//!
//! The Android `EditProfile` UI dispatches `podcast.social { op: publish_profile,
//! name, display_name, picture }` and expects the action to be accepted
//! (a `correlation_id` back, no synchronous error). This scenario guards that
//! exact contract so a regression in the routing chain is caught before it
//! reaches the device.

use nmp_app_podcast::PodcastHandle;
use nmp_native_runtime::NmpApp;
use serde_json::json;

use crate::fixtures;
use crate::harness::{dispatch, snapshot, wait_for};
use crate::scenarios::ScenarioResult::{self, Fail, Pass};

/// Known display name used for the publish_profile dispatch.
const TEST_DISPLAY_NAME: &str = "Headless Test User";
/// Known picture URL used for the publish_profile dispatch.
const TEST_PICTURE_URL: &str = "https://example.com/avatar.jpg";

pub fn run(app: *mut NmpApp, handle: *mut PodcastHandle) -> ScenarioResult {
    // ── Identity setup ────────────────────────────────────────────────────────
    //
    // Import the test nsec if no identity is loaded yet. If a prior scenario
    // already loaded the same key, take the fast path.
    let already_has_identity = snapshot(handle)
        .as_ref()
        .and_then(|u| u.active_account.as_ref())
        .is_some();

    if !already_has_identity {
        let id_res = dispatch(
            app,
            "podcast.identity",
            json!({"type": "ImportNsec", "nsec": fixtures::HEADLESS_TEST_NSEC}),
        );
        if let Some(err) = id_res.get("error").and_then(|v| v.as_str()) {
            return Fail(format!("ImportNsec dispatch rejected: {err}"));
        }

        match wait_for(handle, 5_000, |u| u.active_account.is_some()) {
            Ok(_) => {}
            Err(e) => return Fail(format!("active_account never appeared: {e}")),
        }
    }

    // Snapshot the account state before publishing.
    let pre_account = match snapshot(handle).and_then(|u| u.active_account) {
        Some(a) => a,
        None => return Fail("active_account missing after identity import".into()),
    };

    if pre_account.npub != fixtures::HEADLESS_TEST_NPUB {
        return Fail(format!(
            "unexpected npub before publish: {}",
            pre_account.npub
        ));
    }

    // ── Mutation-sanity guard: malformed payload must be rejected ─────────────
    //
    // `SocialAction::PublishProfile` requires `name` (non-optional). A payload
    // missing `name` must fail serde deserialization and return a synchronous
    // `{"error":"..."}` from the action module, NOT a `correlation_id`. This
    // proves that a correct `name`-bearing dispatch is meaningful — the kernel
    // actually validated the shape, not silently accepted anything.
    let bad_res = dispatch(
        app,
        "podcast.social",
        json!({
            "op": "publish_profile",
            // "name" intentionally omitted — required field
            "display_name": TEST_DISPLAY_NAME,
        }),
    );

    if bad_res.get("correlation_id").is_some() {
        return Fail(format!(
            "malformed publish_profile (missing name) must be rejected, got correlation_id: {bad_res}"
        ));
    }
    if bad_res.get("error").is_none() {
        return Fail(format!(
            "malformed publish_profile must return error field, got: {bad_res}"
        ));
    }

    // ── Seam assertion: dispatch publish_profile with valid payload ────────────
    //
    // Dispatch the kind:0 action through the REAL kernel action path.
    // `name` is required; `display_name` and `picture` are optional enrichments
    // per the SocialAction::PublishProfile wire contract.
    let res = dispatch(
        app,
        "podcast.social",
        json!({
            "op": "publish_profile",
            "name": "headless-test",
            "display_name": TEST_DISPLAY_NAME,
            "picture": TEST_PICTURE_URL,
        }),
    );

    // Must not carry a synchronous error.
    if let Some(err) = res.get("error").and_then(|v| v.as_str()) {
        return Fail(format!("publish_profile dispatch returned error: {err}"));
    }

    // Must be accepted: kernel mints a correlation_id for the queued action.
    if res.get("correlation_id").is_none() {
        return Fail(format!(
            "publish_profile was not accepted by kernel (no correlation_id): {res}"
        ));
    }

    // ── Self-apply reflection: AccountSummary must carry the published values ──
    //
    // After the dispatch is accepted, `handle_publish_profile` mirrors
    // `display_name` and `picture` into the local `IdentityStore` (optimistic
    // self-apply), then `social_actions.rs` bumps `Domain::Identity` so the
    // push frame re-emits with fresh `AccountSummary` values. We wait up to 2 s
    // for the snapshot rev to advance and the predicate to hold.
    //
    // Mutation-sanity note: WITHOUT the self-apply (the old behavior) the
    // `IdentityStore.display_name` field was never set after a publish, so
    // `AccountSummary.display_name` would remain `None` forever and this
    // `wait_for` would time out and return `Fail`. The assertion proves the
    // fix works and CI-gates it.
    match wait_for(handle, 2_000, |u| {
        u.active_account
            .as_ref()
            .and_then(|a| a.display_name.as_deref())
            == Some(TEST_DISPLAY_NAME)
    }) {
        Ok(update) => {
            let account = update.active_account.unwrap();
            // Also verify pubkey integrity so we catch accidental identity replacement.
            if account.pubkey_hex != pre_account.pubkey_hex {
                return Fail(format!(
                    "pubkey_hex changed after publish_profile: before={} after={}",
                    pre_account.pubkey_hex, account.pubkey_hex
                ));
            }
            if account.npub != fixtures::HEADLESS_TEST_NPUB {
                return Fail(format!(
                    "npub corrupted by publish_profile: {}",
                    account.npub
                ));
            }
        }
        Err(timeout_msg) => {
            // On timeout, read the current snapshot for a diagnostic.
            let current_display_name = snapshot(handle)
                .and_then(|u| u.active_account)
                .and_then(|a| a.display_name);
            return Fail(format!(
                "AccountSummary.display_name never reflected published value \
                 (expected {:?}, got {:?}): {}",
                TEST_DISPLAY_NAME, current_display_name, timeout_msg
            ));
        }
    }

    Pass
}
