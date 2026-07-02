//! Headless capability host: handles `nmp.http.capability` (sync) and
//! `nmp.http.async.capability` (fire-and-forget) with real `reqwest::blocking`
//! HTTP, `nostr_relay` capability with a real `tokio-tungstenite` WebSocket
//! client, and no-op stubs for audio, download, and notification namespaces.
//!
//! ## Async HTTP
//!
//! The `nmp.http.async.capability` path mirrors the iOS `HttpCapability`:
//! the kernel fires a fire-and-forget [`HttpCommand`], the host spawns a
//! std thread to run the transport (using reqwest blocking), then calls
//! [`nmp_app_podcast_http_report`] to deliver the [`HttpReport`] back to the
//! kernel's [`FeedFetchCoordinator`]. The handle pointer is stored in a
//! `OnceLock<usize>` (as a raw address, which is `Send`) and set after
//! registration.
//!
//! ## Tokio runtime lifetime
//!
//! A `tokio::runtime::Runtime` is stored in a `OnceLock` so the async relay
//! client (`relay_client`) can be driven from the synchronous `extern "C"`
//! callback via `Runtime::block_on`. The runtime is initialised once in
//! `install` and lives for the process lifetime (the `OnceLock` never drops).

use std::ffi::CString;
use std::sync::OnceLock;

use nmp_app_podcast::capability::{
    NostrRelayRequest, NostrRelayResult, NOSTR_RELAY_CAPABILITY_NAMESPACE,
};
use nmp_app_podcast::ffi::PodcastHandle;
use nmp_app_podcast::nmp_app_podcast_http_report;
use nmp_core::substrate::{CapabilityEnvelope, CapabilityRequest};
use nmp_native_runtime::NmpApp;
use podcast_feeds::http::{
    HttpCommand, HttpMethod, HttpReport, HttpRequest, HttpResult, HTTP_ASYNC_CAPABILITY_NAMESPACE,
    HTTP_CAPABILITY_NAMESPACE,
};
use reqwest::header::{HeaderName, HeaderValue};

use super::relay_client;

/// Tokio runtime used solely for the Nostr relay capability executor.
/// Initialised once in `install`; lives for the process lifetime.
static RELAY_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// The `PodcastHandle` pointer stored as a `usize` so it is `Send + Sync`.
/// Set by [`set_handle`] after `nmp_app_podcast_register` returns. The
/// capability callback only fires during scenario runs (after `nmp_app_start`),
/// so the handle is always set by then.
static PODCAST_HANDLE_ADDR: OnceLock<usize> = OnceLock::new();

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

    if app.is_null() {
        return;
    }
    // SAFETY: the headless harness owns `app` for the binary lifetime and clears
    // it only after scenarios finish. The support crate stores an owned callback
    // closure in the app's native capability slot.
    let app_ref = unsafe { &*app };
    nmp_uniffi_support::set_capability_callback(app_ref, Some(Box::new(())), |_, request| {
        handle_request(&request)
    });
}

/// Register the `PodcastHandle` for the async HTTP report-back path.
///
/// Called after `nmp_app_podcast_register` returns the handle. The handle
/// pointer is stored as a `usize` so it is `Send + Sync`-compatible in the
/// `OnceLock`. The capability callback retrieves it when it needs to call
/// `nmp_app_podcast_http_report`.
pub fn set_handle(handle: *mut PodcastHandle) {
    PODCAST_HANDLE_ADDR.get_or_init(|| handle as usize);
}

/// Route the request JSON to the right handler. Returns the envelope JSON.
fn handle_request(request_str: &str) -> String {
    let req: CapabilityRequest = match serde_json::from_str(request_str) {
        Ok(r) => r,
        Err(e) => return error_envelope("unknown", "", &format!("parse error: {e}")),
    };

    let result_json = match req.namespace.as_str() {
        HTTP_CAPABILITY_NAMESPACE => handle_http(&req.payload_json),
        HTTP_ASYNC_CAPABILITY_NAMESPACE => {
            handle_http_async(&req.payload_json);
            // Fire-and-forget: return an immediate ack (empty ok envelope).
            // The actual result arrives via nmp_app_podcast_http_report.
            serde_json::json!({"ok": true}).to_string()
        }
        NOSTR_RELAY_CAPABILITY_NAMESPACE => handle_nostr_relay(&req.payload_json),
        "nmp.keyring.capability" => {
            use nmp_core::substrate::KeyringRequest;
            match serde_json::from_str::<KeyringRequest>(&req.payload_json) {
                Ok(KeyringRequest::Retrieve { .. }) => {
                    serde_json::to_string(&nmp_core::substrate::KeyringResult::not_found())
                        .unwrap_or_else(|_| "{}".into())
                }
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

/// Handle the async HTTP capability path.
///
/// Decodes the [`HttpCommand`] from `payload_json`, spawns a std thread to
/// execute the HTTP request with reqwest blocking, then calls
/// [`nmp_app_podcast_http_report`] to deliver the result to the kernel's
/// [`FeedFetchCoordinator`]. Returns immediately (fire-and-forget).
fn handle_http_async(payload_json: &str) {
    let cmd: HttpCommand = match serde_json::from_str(payload_json) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[headless] http_async: decode error: {e}");
            return;
        }
    };

    // Retrieve the handle address stored after nmp_app_podcast_register.
    let handle_addr = match PODCAST_HANDLE_ADDR.get() {
        Some(&addr) => addr,
        None => {
            eprintln!(
                "[headless] http_async: handle not set; dropping {}",
                cmd.request_id
            );
            return;
        }
    };

    // Clone fields needed on the spawned thread.
    let request_id = cmd.request_id.clone();
    let http_request = cmd.request;

    std::thread::spawn(move || {
        // Execute the HTTP request synchronously on this thread.
        let result = execute_http_request(&http_request);

        let report = HttpReport {
            request_id: request_id.clone(),
            result,
        };
        let report_json = match serde_json::to_string(&report) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("[headless] http_async: report encode error: {e}");
                return;
            }
        };
        let c_json = match CString::new(report_json) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("[headless] http_async: report JSON contains NUL byte");
                return;
            }
        };

        // SAFETY: handle_addr was obtained from a valid *mut PodcastHandle
        // returned by nmp_app_podcast_register. The kernel keeps the handle
        // alive for the entire binary lifetime (unregister happens after all
        // scenarios complete). This pointer is valid for the duration of this
        // call, which completes before the binary tears down.
        let handle_ptr = handle_addr as *mut PodcastHandle;
        let _ = nmp_app_podcast_http_report(handle_ptr, c_json.as_ptr());
    });
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
        NostrRelayRequest::Publish {
            event_json,
            relay_urls,
        } => {
            let timeout = std::time::Duration::from_secs(15);
            let (accepted, errors) = rt.block_on(relay_client::publish_event(
                &event_json,
                &relay_urls,
                timeout,
            ));
            NostrRelayResult::Published {
                ok: !accepted.is_empty(),
                accepted_relays: accepted,
                errors,
            }
        }
        NostrRelayRequest::Subscribe {
            sub_id,
            filter,
            relay_urls,
            timeout_ms,
        } => {
            let timeout = std::time::Duration::from_millis(timeout_ms);
            let events = rt.block_on(relay_client::subscribe_until_eose(
                &sub_id,
                &filter,
                &relay_urls,
                timeout,
            ));
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
            let res = HttpResult::Error {
                message: format!("decode: {e}"),
            };
            return serde_json::to_string(&res).unwrap_or_else(|_| "{}".into());
        }
    };
    serde_json::to_string(&execute_http_request(&http_req)).unwrap_or_else(|_| "{}".into())
}

/// Shared reqwest transport: executes `req` and returns an [`HttpResult`].
/// Used by both the sync and async HTTP capability paths.
fn execute_http_request(http_req: &HttpRequest) -> HttpResult {
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
    match http_req.body_bytes() {
        Ok(Some(bytes)) => builder = builder.body(bytes.into_owned()),
        Ok(None) => {}
        Err(e) => {
            return HttpResult::Error {
                message: format!("invalid-body-base64: {e}"),
            }
        }
    }

    match builder.send() {
        Ok(resp) => {
            let status_code = resp.status().as_u16();
            let headers: Vec<Vec<String>> = resp
                .headers()
                .iter()
                .map(|(k, v)| vec![k.as_str().to_owned(), v.to_str().unwrap_or("").to_owned()])
                .collect();
            match resp.bytes() {
                Ok(bytes) => HttpResult::ok_with_body_bytes(status_code, headers, bytes.as_ref()),
                Err(e) => HttpResult::Error {
                    message: format!("body: {e}"),
                },
            }
        }
        Err(e) => HttpResult::Error {
            message: format!("transport: {e}"),
        },
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
