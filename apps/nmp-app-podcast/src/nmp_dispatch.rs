//! NMP dispatch helpers — the seams Rust action handlers use to hand
//! work to NMP without naming relay URLs or holding secret keys.
//!
//! * [`register_podcast_signer_in_kernel`] — register a per-podcast secret as
//!   a non-active signer in the kernel's identity roster via
//!   `nmp_app_signin_nsec(make_active=0)`. Idempotent: re-registering an
//!   already-registered key is safe (the kernel's `AddSigner` path overwrites
//!   the slot). Must be dispatched BEFORE the corresponding `publish_raw_with_signer_via_nmp`
//!   or `blossom_upload_via_nmp` call — the kernel queue is FIFO so the signer
//!   is guaranteed to be present when the sign-time lookup runs (D13).
//! * [`publish_raw_with_signer_via_nmp`] — dispatch unsigned
//!   `{kind, tags, content, signer_pubkey}` to `nmp.publish { PublishRaw }`.
//!   `signer_pubkey: Some(hex)` routes signing to the named per-podcast key
//!   (registered via `register_podcast_signer_in_kernel`); `None` falls back
//!   to the active account. NMP's `sign_with_account_nonblocking` resolves the
//!   named key across local-key + remote maps, transparent to the caller.
//! * [`publish_raw_via_nmp`] — dispatch unsigned `{kind, tags, content}` to
//!   `nmp.publish { PublishRaw }`. NMP signs with the active signer (local
//!   nsec or NIP-46 bunker — both handled transparently by the kernel's
//!   `sign_active_nonblocking` / `PendingSign` path), stamps `created_at`
//!   (D9), and routes via Auto. Used for all events the user signs:
//!   kind:10064 author-claims, kind:1111 comments, kind:1 agent notes.
//! * [`blossom_upload_via_nmp`] — dispatch `nmp.blossom.upload` with
//!   `signer_pubkey: Some(hex)` so the kernel signs the kind:24242 Blossom
//!   auth event with the named per-podcast key (D13 — no raw secret bytes
//!   in app code). Returns the correlation id string; the result rides the
//!   `action_results` snapshot slot.
//! * [`publish_via_nmp`] — hand a pre-signed `nostr::Event` to `nmp.publish`
//!   with `target: Auto`. Retained for callers that already have a signed
//!   event object (e.g. NIP-09 deletion); new per-podcast publish paths use
//!   `publish_raw_with_signer_via_nmp` instead.
//! * [`push_interest_via_nmp`] — push a [`LogicalInterest`] into NMP's relay
//!   pool so the kernel opens the subscription without any iOS WebSocket.

use std::ffi::{CString, c_char};

use nmp_core::planner::LogicalInterest;
use nostr::Event;

/// Register a per-podcast secret key in the kernel's identity roster without
/// activating it. `secret_hex` must be a 64-char lowercase hex string (the
/// form [`crate::store::podcast_keys::PodcastKeyStore`] stores and returns).
///
/// The kernel's `AddSigner` path is idempotent: re-registering an already
/// registered key overwrites the roster slot without side effects. Call this
/// BEFORE the matching `publish_raw_with_signer_via_nmp` / `blossom_upload_via_nmp`
/// — the FIFO actor queue guarantees the signer is present when the sign-time
/// lookup fires. No-op on a null app (unit tests / pre-login).
pub(crate) fn register_podcast_signer_in_kernel(app: *mut nmp_ffi::NmpApp, secret_hex: &str) {
    if app.is_null() {
        return;
    }
    // `nmp_app_signin_nsec` accepts either a bech32 `nsec1…` OR a raw 64-char
    // hex string — `parse_secret` in nmp-core tries both forms. We pass the
    // hex directly so no bech32 encoding is needed here.
    let Ok(secret_c) = CString::new(secret_hex) else {
        return;
    };
    // SAFETY: app is non-null (checked above); secret_c is a valid C string.
    unsafe {
        nmp_ffi::nmp_app_signin_nsec(app, secret_c.as_ptr() as *const c_char, 0);
    }
}

/// Dispatch unsigned event parameters to `nmp.publish { PublishRaw }` with an
/// explicit `signer_pubkey`. The kernel's `sign_with_account_nonblocking` looks
/// up the named pubkey hex across the local-key + remote (NIP-46) roster —
/// identical to signing the active account but without switching it.
///
/// The named signer MUST be registered before this call; use
/// [`register_podcast_signer_in_kernel`] immediately before dispatching.
///
/// Returns `"queued"` (async) or `"signed"` (null app).
pub(crate) fn publish_raw_with_signer_via_nmp(
    app: *mut nmp_ffi::NmpApp,
    kind: u32,
    tags: &[Vec<String>],
    content: &str,
    signer_pubkey_hex: &str,
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
            "signer_pubkey": signer_pubkey_hex,
        }
    });
    dispatch_nmp_publish(app, body)
}

/// Dispatch `nmp.blossom.upload` routing the kind:24242 signature to a named
/// per-podcast key. The kernel Build → Sign → Transport pipeline handles
/// hashing, auth-event construction, and HTTP transport — no raw secret bytes
/// or HTTP in app code (D13).
///
/// `servers` must contain at least one valid `http(s)://` URL or the kernel's
/// `UploadAction::start` will reject the dispatch.
///
/// The named signer MUST be registered before this call; use
/// [`register_podcast_signer_in_kernel`] immediately before dispatching.
///
/// Returns a correlation id string the caller can use to retrieve the blob
/// descriptor from the `action_results` snapshot slot, or `None` when `app`
/// is null (unit tests / pre-login) or the dispatch was rejected.
pub(crate) fn blossom_upload_via_nmp(
    app: *mut nmp_ffi::NmpApp,
    file_path: &str,
    servers: &[String],
    signer_pubkey_hex: &str,
) -> Option<String> {
    if app.is_null() {
        return None;
    }
    let body = serde_json::json!({
        "file_path": file_path,
        "servers": servers,
        "signer_pubkey": signer_pubkey_hex,
    });
    let Ok(ns_c) = CString::new("nmp.blossom.upload") else {
        return None;
    };
    let Ok(body_c) = CString::new(body.to_string()) else {
        return None;
    };
    let raw = nmp_ffi::nmp_app_dispatch_action(app, ns_c.as_ptr(), body_c.as_ptr());
    if raw.is_null() {
        return None;
    }
    // SAFETY: raw is a heap-owned NUL-terminated C string from nmp_app_dispatch_action.
    let response = unsafe { std::ffi::CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();
    nmp_ffi::nmp_free_string(raw);

    // Parse the correlation_id from the accept envelope.
    let parsed: serde_json::Value = serde_json::from_str(&response).ok()?;
    parsed
        .get("correlation_id")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

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

/// Dispatch unsigned event parameters to `nmp.publish { PublishRaw }` using
/// the ACTIVE account signer. NMP signs with the active signer (local nsec or
/// NIP-46 bunker — transparent to the caller; bunker ops park on the kernel's
/// `PendingSign` queue and resolve asynchronously), stamps `created_at` (D9),
/// and routes via Auto. No secret bytes in app code.
/// Used for events the user signs: kind:10064 author-claims, kind:1111 comments.
/// For per-podcast keys use [`publish_raw_with_signer_via_nmp`] instead.
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
