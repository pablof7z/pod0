//! Headless capability host: handles `nmp.http.capability` requests with
//! real `reqwest::blocking` HTTP; returns no-op stubs for audio, download,
//! notification, and keyring namespaces.
//!
//! The callback is an `extern "C"` function pointer — all unsafe FFI is
//! contained here, matching the D6 "errors as data" contract used by the
//! kernel's `mock_handler` reference implementation.

use std::ffi::{c_char, c_void, CStr, CString};

use nmp_core::substrate::{CapabilityEnvelope, CapabilityRequest};
use nmp_ffi::{nmp_app_set_capability_callback, NmpApp};
use podcast_feeds::http::{HttpMethod, HttpRequest, HttpResult, HTTP_CAPABILITY_NAMESPACE};
use reqwest::header::{HeaderName, HeaderValue};

/// Install the headless capability callback on `app`. Must be called before
/// `nmp_app_start`.
pub fn install(app: *mut NmpApp) {
    nmp_app_set_capability_callback(
        app,
        std::ptr::null_mut(), // context unused
        Some(capability_handler),
    );
}

/// The C-ABI capability handler. Receives a `CapabilityRequest` JSON, routes
/// by namespace, and returns a `CapabilityEnvelope` JSON pointer. The caller
/// (kernel) owns and frees the returned pointer.
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
    // serde_json output never contains interior NUL bytes (D6 fallback).
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
        "nmp.keyring.capability" => {
            // Keyring: headless has no real keyring. Return "not found" for
            // Retrieve (so the kernel treats no stored identity as a clean
            // slate); return ok(None) for Store/Delete (no-ops).
            use nmp_core::substrate::KeyringRequest;
            match serde_json::from_str::<KeyringRequest>(&req.payload_json) {
                Ok(KeyringRequest::Retrieve { .. }) => {
                    serde_json::to_string(
                        &nmp_core::substrate::KeyringResult::not_found()
                    ).unwrap_or_else(|_| "{}".into())
                }
                _ => {
                    serde_json::to_string(
                        &nmp_core::substrate::KeyringResult::ok(None)
                    ).unwrap_or_else(|_| "{}".into())
                }
            }
        }
        ns => {
            // Stub for audio, download, notification, etc.
            eprintln!("[headless] stub capability: {ns}");
            serde_json::json!({"ok": false, "error": format!("stub: {ns}")}).to_string()
        }
    };

    serde_json::to_string(&CapabilityEnvelope {
        namespace: req.namespace,
        correlation_id: req.correlation_id,
        result_json,
    }).unwrap_or_else(|_| "{}".into())
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
                .map(|(k, v)| {
                    vec![
                        k.as_str().to_owned(),
                        v.to_str().unwrap_or("").to_owned(),
                    ]
                })
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
