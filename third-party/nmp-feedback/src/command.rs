use std::sync::atomic::{AtomicU64, Ordering};

use nmp_core::dispatch_envelope::{encode_dispatch_envelope, DISPATCH_ENVELOPE_SCHEMA_VERSION};
use nmp_core::publish::PublishAction;
use nmp_core::substrate::ActionPayload;
use nmp_ffi::{nmp_app_dispatch_action_bytes, nmp_free_string, NmpApp};
use serde::{Deserialize, Serialize};

use crate::config::{text_note_kind, FeedbackConfig};

/// Process-local correlation-id source. Prefix `feedback-` distinguishes
/// these ids from other podcast-app or kernel-internal ids.
static NEXT_CORRELATION_ID: AtomicU64 = AtomicU64::new(1);

fn mint_correlation_id() -> String {
    let n = NEXT_CORRELATION_ID.fetch_add(1, Ordering::Relaxed);
    format!("feedback-{n}")
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct FeedbackCommandOutcome {
    pub ok: bool,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl FeedbackCommandOutcome {
    #[must_use]
    pub fn accepted(status: impl Into<String>) -> Self {
        Self {
            ok: true,
            status: status.into(),
            error: None,
        }
    }

    #[must_use]
    pub fn rejected(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            status: "rejected".to_string(),
            error: Some(error.into()),
        }
    }

    #[must_use]
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| {
            serde_json::json!({
                "ok": false,
                "status": "rejected",
                "error": "feedback outcome encoding failed"
            })
        })
    }
}

#[must_use]
pub fn fetch_feedback(app: *mut NmpApp, config: &FeedbackConfig) -> FeedbackCommandOutcome {
    if app.is_null() {
        return FeedbackCommandOutcome::rejected("feedback runtime unavailable");
    }
    // SAFETY: app is non-null and owned by the host NMP runtime.
    unsafe { &*app }.push_interest(config.interest());
    FeedbackCommandOutcome::accepted("subscribed")
}

#[must_use]
pub fn publish_feedback(
    app: *mut NmpApp,
    config: &FeedbackConfig,
    category: &str,
    content: &str,
    parent_event_id: Option<&str>,
    reply_to_pubkey: Option<&str>,
) -> FeedbackCommandOutcome {
    let content = content.trim();
    if content.is_empty() {
        return FeedbackCommandOutcome::rejected("empty feedback");
    }
    if app.is_null() {
        return FeedbackCommandOutcome::rejected("feedback runtime unavailable");
    }
    let tags = config.tags(category, parent_event_id, reply_to_pubkey);
    let body = serde_json::json!({
        "PublishRaw": {
            "kind": text_note_kind(),
            "tags": tags,
            "content": content,
            "target": { "Explicit": { "relays": [&config.relay_url] } },
        }
    });
    dispatch_nmp_publish(app, body)
}

fn dispatch_nmp_publish(app: *mut NmpApp, body: serde_json::Value) -> FeedbackCommandOutcome {
    let body_str = body.to_string();
    let action: PublishAction = match serde_json::from_str(&body_str) {
        Ok(a) => a,
        Err(e) => {
            return FeedbackCommandOutcome::rejected(format!(
                "feedback publish body shape error: {e}"
            ))
        }
    };
    let payload = action.encode();
    let correlation_id = mint_correlation_id();
    let envelope = encode_dispatch_envelope(
        &correlation_id,
        "nmp.publish",
        DISPATCH_ENVELOPE_SCHEMA_VERSION,
        &payload,
    );
    // SAFETY: app is non-null (checked by caller); envelope is valid bytes
    // produced by encode_dispatch_envelope.
    let result_ptr =
        unsafe { nmp_app_dispatch_action_bytes(app, envelope.as_ptr(), envelope.len()) };
    if result_ptr.is_null() {
        return FeedbackCommandOutcome::rejected("publish dispatch failed");
    }
    // SAFETY: result_ptr is heap-owned from the kernel; read it then free it.
    // Reading BEFORE freeing is required — the string is invalidated by nmp_free_string.
    let result_json = unsafe {
        let c_str = std::ffi::CStr::from_ptr(result_ptr as *const i8);
        let s = c_str.to_string_lossy().to_string();
        nmp_free_string(result_ptr);
        s
    };
    // Propagate a kernel rejection; fall through to accepted on any other shape.
    match serde_json::from_str::<serde_json::Value>(&result_json) {
        Ok(serde_json::Value::Object(map)) => {
            if let Some(serde_json::Value::String(err)) = map.get("error") {
                return FeedbackCommandOutcome::rejected(err.clone());
            }
        }
        _ => {}
    }
    FeedbackCommandOutcome::accepted("queued")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_publish_is_rejected_before_dispatch() {
        let config = FeedbackConfig::new("31933:abc:app");
        let out = publish_feedback(std::ptr::null_mut(), &config, "bug", "  ", None, None);
        assert_eq!(out.ok, false);
        assert_eq!(out.error.as_deref(), Some("empty feedback"));
    }

    #[test]
    fn null_runtime_is_rejected_not_faked() {
        let config = FeedbackConfig::new("31933:abc:app");
        let fetch = fetch_feedback(std::ptr::null_mut(), &config);
        let publish =
            publish_feedback(std::ptr::null_mut(), &config, "bug", "real text", None, None);
        assert_eq!(fetch.error.as_deref(), Some("feedback runtime unavailable"));
        assert_eq!(
            publish.error.as_deref(),
            Some("feedback runtime unavailable")
        );
    }
}
