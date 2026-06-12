//! NMP dispatch helpers — the seams Rust action handlers use to hand
//! work to NMP without naming relay URLs or holding secret keys.
//!
//! * [`publish_via_nmp`] — hand a pre-signed `nostr::Event` to `nmp.publish`
//!   with `target: Auto`. Used for per-podcast keys (kind:10154/54) that
//!   NMP cannot yet sign on behalf of (pending multi-account signer API).
//! * [`publish_raw_via_nmp`] — dispatch unsigned `{kind, tags, content}` to
//!   `nmp.publish { PublishRaw }`. NMP signs with the active signer (user's
//!   nsec), stamps `created_at` (D9), and routes via Auto. Used for all
//!   events the user signs: kind:10064 author-claims, kind:1111 comments,
//!   kind:1 agent notes.
//! * [`push_interest_via_nmp`] — push a [`LogicalInterest`] into NMP's relay
//!   pool so the kernel opens the subscription without any iOS WebSocket.

use std::ffi::CString;

use nmp_core::planner::LogicalInterest;
use nostr::Event;

/// Hand a pre-signed event to `nmp.publish { Publish, target: Auto }`.
/// NMP routes through the relay pool; no relay URLs in app code.
/// Returns `"queued"` (async, fire-and-forget) or `"signed"` (null app).
pub(crate) fn publish_via_nmp(app: *mut nmp_ffi::NmpApp, event: &Event) -> &'static str {
    if app.is_null() {
        return "signed";
    }
    let signed_event = serde_json::json!({
        "id": event.id.to_hex(),
        "sig": event.sig.to_string(),
        "unsigned": {
            "pubkey": event.pubkey.to_hex(),
            "kind": u32::from(event.kind.as_u16()),
            "created_at": event.created_at.as_secs(),
            "tags": event.tags.iter().map(|t| t.as_slice().to_vec()).collect::<Vec<_>>(),
            "content": &*event.content,
        }
    });
    let body = serde_json::json!({
        "Publish": {
            "handle": uuid::Uuid::new_v4().to_string(),
            "event": signed_event,
            "target": "Auto",
        }
    });
    dispatch_nmp_publish(app, body)
}

/// Dispatch unsigned event parameters to `nmp.publish { PublishRaw }`.
/// NMP signs with the active signer (user's nsec), stamps `created_at`
/// (D9 — kernel owns the clock), and routes via Auto. No secret bytes in
/// app code.
/// Returns `"queued"` or `"signed"` (null app).
pub(crate) fn publish_raw_via_nmp(
    app: *mut nmp_ffi::NmpApp,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
) -> &'static str {
    if app.is_null() {
        return "signed";
    }
    let body = serde_json::json!({
        "PublishRaw": {
            "kind": kind,
            "tags": tags,
            "content": content,
            "target": "Auto",
        }
    });
    dispatch_nmp_publish(app, body)
}

/// Dispatch a kind:0 profile metadata update to `nmp.publish { PublishProfile }`.
/// `fields` is a flat string-valued JSON object (`name`, `display_name`,
/// `about`, `picture`, …); the kernel serialises it into the kind:0 `content`,
/// signs with the active signer, stamps `created_at` (D7), and routes via the
/// NIP-65 outbox. No secret bytes in app code; the host never builds the event.
/// Returns `"queued"` or `"signed"` (null app).
pub(crate) fn publish_profile_via_nmp(
    app: *mut nmp_ffi::NmpApp,
    fields: serde_json::Map<String, serde_json::Value>,
) -> &'static str {
    if app.is_null() {
        return "signed";
    }
    let body = serde_json::json!({ "PublishProfile": { "fields": fields } });
    dispatch_nmp_publish(app, body)
}

/// Self-enqueue a `podcast.publish` action back onto the actor queue.
///
/// `nmp_app_dispatch_action` only *validates* the action and enqueues an
/// `ActorCommand::DispatchHostOp` (D8: no actor round-trip, no blocking),
/// then returns immediately. Calling it from inside a host-op handler
/// appends a follow-up command to the actor's own queue — so the dispatched
/// op runs in its OWN later tick and the actor yields in between. This is the
/// non-blocking way for a handler to fan a single op out into N independent
/// ops without stalling reactivity (the old Swift loop's per-episode
/// `kernelPublishEpisode` had this property; this preserves it while keeping
/// the policy in the kernel — D0).
///
/// Returns `true` when the action was accepted (a `correlation_id` was
/// minted), `false` on a null app (tests / pre-login) or a rejected action.
pub(crate) fn self_dispatch_publish(app: *mut nmp_ffi::NmpApp, body: serde_json::Value) -> bool {
    if app.is_null() {
        return false;
    }
    let (Ok(ns_c), Ok(body_c)) = (CString::new("podcast.publish"), CString::new(body.to_string()))
    else {
        return false;
    };
    let raw = nmp_ffi::nmp_app_dispatch_action(app, ns_c.as_ptr(), body_c.as_ptr());
    if raw.is_null() {
        return false;
    }
    // SAFETY: `raw` is a heap-owned NUL-terminated C string minted by
    // `nmp_app_dispatch_action`; read the accept marker, then free it.
    let accepted = unsafe { std::ffi::CStr::from_ptr(raw) }
        .to_string_lossy()
        .contains("\"correlation_id\"");
    nmp_ffi::nmp_free_string(raw);
    accepted
}

/// Push a [`LogicalInterest`] into NMP's relay pool. The kernel opens the
/// subscription through its own connections — no iOS WebSocket ever opened.
pub(crate) fn push_interest_via_nmp(app: *mut nmp_ffi::NmpApp, interest: LogicalInterest) {
    if app.is_null() {
        return;
    }
    // SAFETY: app is non-null.
    unsafe { &*app }.push_interest(interest);
}

fn dispatch_nmp_publish(app: *mut nmp_ffi::NmpApp, body: serde_json::Value) -> &'static str {
    let Ok(ns_c) = CString::new("nmp.publish") else {
        return "signed";
    };
    let Ok(body_c) = CString::new(body.to_string()) else {
        return "signed";
    };
    let raw = nmp_ffi::nmp_app_dispatch_action(app, ns_c.as_ptr(), body_c.as_ptr());
    if !raw.is_null() {
        nmp_ffi::nmp_free_string(raw);
    }
    "queued"
}
