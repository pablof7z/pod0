use serde_json::{json, Value};
use std::ffi::{c_void, CStr, CString};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::{AppRuntime, Result};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, PartialEq, Eq)]
enum NostrSubscribeIntent {
    DirectRef(String),
    Nip05(String),
    SecretLike,
    Unsupported(&'static str),
}

#[derive(Debug, PartialEq, Eq)]
enum DecodedNostrRef {
    AuthorPubkey(String),
    Event,
}

extern "C" {
    fn nmp_app_intent_classify(
        app: *mut c_void,
        request_json: *const std::ffi::c_char,
    ) -> *mut std::ffi::c_char;
    fn nmp_app_intent_dispatch(
        app: *mut c_void,
        request_json: *const std::ffi::c_char,
        session_id: *const std::ffi::c_char,
    ) -> *mut std::ffi::c_char;
}

impl AppRuntime {
    pub fn subscribe_input(&self, input: &str) -> Result<String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("subscribe input is empty".to_owned());
        }

        match self.classify_nostr_subscribe_intent(trimmed)? {
            Some(NostrSubscribeIntent::DirectRef(uri)) => match decode_nostr_ref(&uri)? {
                DecodedNostrRef::AuthorPubkey(pubkey) => self.subscribe_nostr(&pubkey),
                DecodedNostrRef::Event => Err(
                    "Nostr event links are not subscribable here; use an npub or nprofile"
                        .to_owned(),
                ),
            },
            Some(NostrSubscribeIntent::Nip05(identifier)) => {
                self.dispatch_nostr_intent(trimmed)?;
                Ok(format!("looking up NIP-05: {identifier}"))
            }
            Some(NostrSubscribeIntent::SecretLike) => {
                Err("Nostr private keys must not be pasted into subscribe".to_owned())
            }
            Some(NostrSubscribeIntent::Unsupported(message)) => Err(message.to_owned()),
            None => self.subscribe(trimmed),
        }
    }

    fn subscribe_nostr(&self, author_pubkey_hex: &str) -> Result<String> {
        self.dispatch_action_value(
            "podcast",
            &json!({"op": "subscribe_nostr", "author_pubkey_hex": author_pubkey_hex}),
        )
    }

    fn classify_nostr_subscribe_intent(&self, input: &str) -> Result<Option<NostrSubscribeIntent>> {
        let request = intent_request_json(input);
        let request = CString::new(request)
            .map_err(|_| "subscribe input contains an unsupported NUL byte".to_owned())?;
        let ptr = unsafe { nmp_app_intent_classify(self.app_ptr().cast(), request.as_ptr()) };
        let raw = take_ffi_string(ptr, "nmp_app_intent_classify")?;
        let value: Value = serde_json::from_str(&raw)
            .map_err(|e| format!("intent classification returned invalid JSON: {e}"))?;
        parse_nostr_subscribe_intent(&value)
    }

    fn dispatch_nostr_intent(&self, input: &str) -> Result<String> {
        let request = intent_request_json(input);
        let request = CString::new(request)
            .map_err(|_| "subscribe input contains an unsupported NUL byte".to_owned())?;
        let session_id = CString::new(format!("tui-subscribe-{}", session_suffix()))
            .map_err(|_| "generated session id contains NUL".to_owned())?;
        let ptr = unsafe {
            nmp_app_intent_dispatch(self.app_ptr().cast(), request.as_ptr(), session_id.as_ptr())
        };
        take_ffi_string(ptr, "nmp_app_intent_dispatch")
    }
}

fn intent_request_json(input: &str) -> String {
    json!({
        "input": input,
        "scopes": [
            {"namespace": "nostr", "name": "ref"},
            {"namespace": "nip50", "name": "profiles"}
        ],
        "text_targets": "UserPreferred"
    })
    .to_string()
}

fn parse_nostr_subscribe_intent(value: &Value) -> Result<Option<NostrSubscribeIntent>> {
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Ok(None);
    }
    let Some(classification) = value.get("classification") else {
        return Ok(None);
    };
    if let Some(rejection) = classification.get("Rejection") {
        return Ok(parse_intent_rejection(rejection));
    }
    let Some(target) = classification
        .get("Candidates")
        .and_then(Value::as_array)
        .and_then(|candidates| candidates.first())
        .and_then(|candidate| candidate.get("target"))
    else {
        return Ok(None);
    };
    Ok(parse_intent_target(target))
}

fn parse_intent_rejection(value: &Value) -> Option<NostrSubscribeIntent> {
    match value.as_str() {
        Some("SecretLike") => Some(NostrSubscribeIntent::SecretLike),
        Some("Unparseable") => None,
        _ => Some(NostrSubscribeIntent::Unsupported(
            "that Nostr input is not available from subscribe yet",
        )),
    }
}

fn parse_intent_target(target: &Value) -> Option<NostrSubscribeIntent> {
    if let Some(uri) = target
        .get("DirectRef")
        .and_then(|body| body.get("uri"))
        .and_then(Value::as_str)
    {
        return Some(NostrSubscribeIntent::DirectRef(uri.to_owned()));
    }
    if let Some(identifier) = target
        .get("Nip05")
        .and_then(|body| body.get("identifier"))
        .and_then(Value::as_str)
    {
        return Some(NostrSubscribeIntent::Nip05(identifier.to_owned()));
    }
    if target.get("TextQuery").is_some() || target.get("RelayUrl").is_some() {
        return None;
    }
    if target.get("Registered").is_some() {
        return Some(NostrSubscribeIntent::Unsupported(
            "that Nostr input is not supported here yet",
        ));
    }
    None
}

fn decode_nostr_ref(uri: &str) -> Result<DecodedNostrRef> {
    let uri = CString::new(uri).map_err(|_| "Nostr reference contains NUL".to_owned())?;
    let ptr = nmp_ffi::nmp_nip21_decode_uri(uri.as_ptr());
    let raw = take_ffi_string(ptr, "nmp_nip21_decode_uri")?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|e| format!("Nostr reference decode returned invalid JSON: {e}"))?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err("that Nostr reference could not be decoded".to_owned());
    }
    match value.get("target").and_then(Value::as_str) {
        Some("profile") | Some("address") => value
            .get("pubkey")
            .and_then(Value::as_str)
            .filter(|pubkey| !pubkey.is_empty())
            .map(|pubkey| DecodedNostrRef::AuthorPubkey(pubkey.to_owned()))
            .ok_or_else(|| "decoded Nostr reference did not include a pubkey".to_owned()),
        Some("event") => Ok(DecodedNostrRef::Event),
        _ => Err("that Nostr reference is not subscribable".to_owned()),
    }
}

fn take_ffi_string(ptr: *mut std::ffi::c_char, function_name: &str) -> Result<String> {
    if ptr.is_null() {
        return Err(format!("{function_name} returned null"));
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|e| format!("{function_name} returned non-UTF8 text: {e}"))?
        .to_owned();
    nmp_ffi::nmp_free_string(ptr);
    Ok(text)
}

fn session_suffix() -> u64 {
    SESSION_COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_direct_ref_candidate() {
        let value = json!({
            "ok": true,
            "classification": {
                "Candidates": [{
                    "target": {"DirectRef": {"uri": "nostr:npub1abc"}}
                }]
            }
        });

        assert_eq!(
            parse_nostr_subscribe_intent(&value).unwrap(),
            Some(NostrSubscribeIntent::DirectRef("nostr:npub1abc".to_owned()))
        );
    }

    #[test]
    fn parse_secret_like_rejection_without_echo() {
        let value = json!({
            "ok": true,
            "classification": {"Rejection": "SecretLike"}
        });

        assert_eq!(
            parse_nostr_subscribe_intent(&value).unwrap(),
            Some(NostrSubscribeIntent::SecretLike)
        );
    }

    #[test]
    fn parse_text_query_as_rss_fallback() {
        let value = json!({
            "ok": true,
            "classification": {
                "Candidates": [{
                    "target": {"TextQuery": {"request_json": "{}"}}
                }]
            }
        });

        assert_eq!(parse_nostr_subscribe_intent(&value).unwrap(), None);
    }

    #[test]
    fn parse_nip05_candidate() {
        let value = json!({
            "ok": true,
            "classification": {
                "Candidates": [{
                    "target": {"Nip05": {"identifier": "alice@example.com"}}
                }]
            }
        });

        assert_eq!(
            parse_nostr_subscribe_intent(&value).unwrap(),
            Some(NostrSubscribeIntent::Nip05("alice@example.com".to_owned()))
        );
    }
}
