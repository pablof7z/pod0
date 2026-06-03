//! NMP dispatch helpers — the two seams all Rust action handlers use to hand
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
use nostr::{Event, JsonUtil};

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
            "created_at": event.created_at.as_u64(),
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
    // SAFETY: app is non-null (callers check before calling this).
    let raw =
        unsafe { nmp_ffi::nmp_app_dispatch_action(app, ns_c.as_ptr(), body_c.as_ptr()) };
    if !raw.is_null() {
        // SAFETY: NMP allocated this string; we free it immediately.
        unsafe { nmp_ffi::nmp_app_free_string(raw) };
    }
    "queued"
}
