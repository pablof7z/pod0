//! Headless capability host: handles `nmp.http.capability` with real
//! `reqwest::blocking` HTTP and `nostr_relay` capability with a real
//! `tokio-tungstenite` WebSocket client. Returns no-op stubs for audio,
//! download, notification, and keyring namespaces.
//!
//! The callback is an `extern "C"` function pointer — all unsafe FFI is
//! contained here, matching the D6 "errors as data" contract used by the
//! kernel's `mock_handler` reference implementation.
//!
//! ## Tokio runtime lifetime
//!
//! A `tokio::runtime::Runtime` is stored in a `OnceLock` so the async relay
//! client (`relay_client`) can be driven from the synchronous `extern "C"`
//! callback via `Runtime::block_on`. The runtime is initialised once in
//! `install` and lives for the process lifetime (the `OnceLock` never drops).

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::OnceLock;

use nmp_core::substrate::{CapabilityEnvelope, CapabilityRequest};
use nmp_ffi::{nmp_app_set_capability_callback, NmpApp};
use nmp_app_podcast::capability::{
    NostrRelayRequest, NostrRelayResult, NOSTR_RELAY_CAPABILITY_NAMESPACE,
};
use podcast_feeds::http::{HttpMethod, HttpRequest, HttpResult, HTTP_CAPABILITY_NAMESPACE};
use reqwest::header::{HeaderName, HeaderValue};

use super::relay_client;

/// Tokio runtime used solely for the Nostr relay capability executor.
/// Initialised once in `install`; lives for the process lifetime.
static RELAY_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// Install the headless capability callback on `app`. Must be called before
/// `nmp_app_start`. Also initialises the Tokio relay runtime.
pub fn install(app: *mut NmpApp) {
    // Ensure the Tokio runtime is ready before the first capability call.
    RELAY_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("relay runtime")
    });

    nmp_app_set_capability_callback(
        app,
        std::ptr::null_mut(), // context unused — runtime is in the static
        Some(capability_handler),
    );
}

/// C-ABI capability handler. Receives `CapabilityRequest` JSON, routes by
/// namespace, and returns a `CapabilityEnvelope` JSON pointer.
///
/// D6: never returns null; every failure is data in the envelope.
extern "C" fn capability_handler(
    _ctx: *mut c_void,
    request_json: *const c_char,
) -> *mut c_char {
    let request_str = if request_json.is_null() {
        ""
    } else {
        // SAFETY: kernel guarantees a valid NUL-terminated C string.
        match unsafe { CStr::from_ptr(request_json) }.to_str() {
            Ok(s) => s,
            Err(_) => "",
        }
    };

    let result_json = handle_request(request_str);
    CString::new(result_json)
        .unwrap_or_else(|_| CString::new("{}").unwrap())
        .into_raw()
}

/// Route the request JSON to the right handler. Returns the envelope JSON.
fn handle_request(request_str: &str) -> String {
    let req: CapabilityRequest = match serde_json::from_str(request_str) {
        Ok(r) => r,
        Err(e) => return error_envelope("unknown", "", &format!("parse error: {e}")),
    };

    let result_json = match req.namespace.as_str() {
        HTTP_CAPABILITY_NAMESPACE => handle_http(&req.payload_json),
        NOSTR_RELAY_CAPABILITY_NAMESPACE => handle_nostr_relay(&req.payload_json),
        "nmp.keyring.capability" => {
            use nmp_core::substrate::KeyringRequest;
            match serde_json::from_str::<KeyringRequest>(&req.payload_json) {
                Ok(KeyringRequest::Retrieve { .. }) => serde_json::to_string(
                    &nmp_core::substrate::KeyringResult::not_found(),
                )
                .unwrap_or_else(|_| "{}".into()),
                _ => serde_json::to_string(&nmp_core::substrate::KeyringResult::ok(None))
                    .unwrap_or_else(|_| "{}".into()),
            }
        }
        ns => {
            eprintln!("[headless] stub capability: {ns}");
            serde_json::json!({"ok": false, "error": format!("stub: {ns}")}).to_string()
        }
    };

    serde_json::to_string(&CapabilityEnvelope {
        namespace: req.namespace,
        correlation_id: req.correlation_id,
        result_json,
    })
    .unwrap_or_else(|_| "{}".into())
}

/// Execute a real WebSocket Nostr relay operation (publish or subscribe).
fn handle_nostr_relay(payload_json: &str) -> String {
    let relay_req: NostrRelayRequest = match serde_json::from_str(payload_json) {
        Ok(r) => r,
        Err(e) => {
            let res = NostrRelayResult::Error {
                message: format!("decode: {e}"),
            };
            return serde_json::to_string(&res).unwrap_or_else(|_| "{}".into());
        }
    };

    let rt = match RELAY_RUNTIME.get() {
        Some(rt) => rt,
        None => {
            let res = NostrRelayResult::Error {
                message: "relay runtime not initialised".into(),
            };
            return serde_json::to_string(&res).unwrap_or_else(|_| "{}".into());
        }
    };

    let result = match relay_req {
        NostrRelayRequest::Publish { event_json, relay_urls } => {
            let timeout = std::time::Duration::from_secs(15);
            let (accepted, errors) =
                rt.block_on(relay_client::publish_event(&event_json, &relay_urls, timeout));
            NostrRelayResult::Published {
                ok: !accepted.is_empty(),
                accepted_relays: accepted,
                errors,
            }
        }
        NostrRelayRequest::Subscribe { sub_id, filter, relay_urls, timeout_ms } => {
            let timeout = std::time::Duration::from_millis(timeout_ms);
            let events =
                rt.block_on(relay_client::subscribe_until_eose(&sub_id, &filter, &relay_urls, timeout));
            NostrRelayResult::Events {
                eose: true, // best-effort; we always return after EOSE or timeout
                events,
            }
        }
    };

    serde_json::to_string(&result).unwrap_or_else(|_| "{}".into())
}

/// Execute a real HTTP request using `reqwest::blocking`.
fn handle_http(payload_json: &str) -> String {
    let http_req: HttpRequest = match serde_json::from_str(payload_json) {
        Ok(r) => r,
        Err(e) => {
            let res = HttpResult::Error { message: format!("decode: {e}") };
            return serde_json::to_string(&res).unwrap_or_else(|_| "{}".into());
        }
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::blocking::Client::new());

    let method = match http_req.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
    };

    let mut builder = client.request(method, &http_req.url);
    for pair in &http_req.headers {
        if pair.len() == 2 {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(pair[0].as_bytes()),
                HeaderValue::from_str(&pair[1]),
            ) {
                builder = builder.header(name, val);
            }
        }
    }
    if let Some(body) = http_req.body {
        builder = builder.body(body);
    }

    match builder.send() {
        Ok(resp) => {
            let status_code = resp.status().as_u16();
            let headers: Vec<Vec<String>> = resp
                .headers()
                .iter()
                .map(|(k, v)| vec![k.as_str().to_owned(), v.to_str().unwrap_or("").to_owned()])
                .collect();
            match resp.text() {
                Ok(body) => {
                    let res = HttpResult::Ok { status_code, headers, body };
                    serde_json::to_string(&res).unwrap_or_else(|_| "{}".into())
                }
                Err(e) => {
                    let res = HttpResult::Error { message: format!("body: {e}") };
                    serde_json::to_string(&res).unwrap_or_else(|_| "{}".into())
                }
            }
        }
        Err(e) => {
            let res = HttpResult::Error { message: format!("transport: {e}") };
            serde_json::to_string(&res).unwrap_or_else(|_| "{}".into())
        }
    }
}

fn error_envelope(namespace: &str, correlation_id: &str, msg: &str) -> String {
    let envelope = CapabilityEnvelope {
        namespace: namespace.to_owned(),
        correlation_id: correlation_id.to_owned(),
        result_json: serde_json::json!({"ok": false, "error": msg}).to_string(),
    };
    serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".into())
}
