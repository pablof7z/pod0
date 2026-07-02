use std::ffi::{c_char, CStr, CString};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, OnceLock};

use nmp_app_podcast::ffi::PodcastUpdate;
use nmp_app_podcast::{
    nmp_app_podcast_audio_report, nmp_app_podcast_elevenlabs_voice_catalog,
    nmp_app_podcast_http_report, nmp_app_podcast_local_model_catalog,
    nmp_app_podcast_provider_model_catalog, nmp_app_podcast_register, nmp_app_podcast_set_data_dir,
    nmp_app_podcast_speech_model_catalog, nmp_app_podcast_unregister, PodcastHandle,
    AUDIO_CAPABILITY_NAMESPACE,
};
use nmp_native_runtime::NmpApp;
use podcast_feeds::http::{
    HttpCommand, HttpMethod, HttpReport, HttpRequest, HttpResult, HTTP_ASYNC_CAPABILITY_NAMESPACE,
    HTTP_CAPABILITY_NAMESPACE,
};
use serde_json::Value;

use crate::audio_host::AudioHost;
use crate::bridge::{self, NmpEvent};
use crate::local_model_catalog::{decode_local_model_catalog, LocalModelCatalog};
use crate::provider_model_catalog::{decode_provider_catalog, ProviderCatalogModel};
use crate::provider_voice_catalog::{decode_elevenlabs_voice_catalog, ProviderCatalogVoice};
use crate::speech_model_catalog::{decode_speech_model_catalog, SpeechModelCatalog};

static AUDIO_HOST: OnceLock<Arc<Mutex<AudioHost>>> = OnceLock::new();

/// `PodcastHandle` pointer (stored as `usize` so it is `Send`) for the async
/// HTTP capability executor. The capability callback is a free `extern "C"`
/// function registered with a null `ctx`, so the off-thread report path cannot
/// reach the handle through `AppRuntime`; mirroring [`AUDIO_HOST`], we stash the
/// pointer here when the handle is registered. The handle outlives the process
/// (it is only freed in `AppRuntime::drop` at shutdown), so casting it back on
/// the worker thread is sound.
static PODCAST_HANDLE: OnceLock<usize> = OnceLock::new();

pub struct AppRuntime {
    app: Box<NmpApp>,
    podcast: *mut PodcastHandle,
    update_bridge: Option<Box<bridge::NmpUpdateBridge>>,
}

pub type Result<T> = std::result::Result<T, String>;

impl AppRuntime {
    pub(crate) fn app_ptr(&self) -> *mut NmpApp {
        (&*self.app as *const NmpApp).cast_mut()
    }

    #[must_use]
    pub fn new(data_dir: &Option<String>) -> Result<(Self, Receiver<NmpEvent>)> {
        let mut app = Box::new(nmp_native_runtime::new_app());
        let app_ptr: *mut NmpApp = app.as_mut();

        let audio_host = Arc::new(Mutex::new(AudioHost::new()));
        let _ = AUDIO_HOST.set(audio_host);

        nmp_uniffi_support::set_capability_callback(&app, Some(Box::new(())), |_, request| {
            dispatch_capability_request(&request)
        });

        let podcast = nmp_app_podcast_register(app_ptr);
        if podcast.is_null() {
            return Err("nmp_app_podcast_register returned null".to_string());
        }
        // Publish the handle for the async HTTP capability executor. Set before
        // `nmp_app_start`, so it is always present by the time any capability
        // request can be dispatched.
        let _ = PODCAST_HANDLE.set(podcast as usize);

        if let Some(dir) = data_dir {
            let dir_cstr =
                CString::new(dir.as_str()).map_err(|_| "data_dir contains NUL".to_string())?;
            nmp_app_podcast_set_data_dir(podcast, dir_cstr.as_ptr());
        }

        let (mut bridge, rx) = bridge::NmpUpdateBridge::channel();
        bridge::NmpUpdateBridge::register(app_ptr, &mut bridge);

        // ADR-0053 / NMP v0.8: TUI currently consumes the full built-in
        // projection set, with podcast sidecars registered app-locally.
        app.consume_all_builtin_projections();
        app.start_runtime(200, 10);

        Ok((
            Self {
                app,
                podcast,
                update_bridge: Some(bridge),
            },
            rx,
        ))
    }

    pub fn dispatch_action(&self, namespace: &str, action_json: &str) -> Result<String> {
        // ADR-0064: route through the typed byte doorway.
        nmp_app_podcast::dispatch_bytes::dispatch_action_bytes_for(
            self.app_ptr(),
            namespace,
            action_json,
        )
    }

    pub fn dispatch_action_value(&self, namespace: &str, action: &Value) -> Result<String> {
        self.dispatch_action(namespace, &action.to_string())
    }

    /// Sample mpv's playback position and forward any pending [`AudioReport`]s
    /// to the kernel via `nmp_app_podcast_audio_report`.
    ///
    /// D4/D7: called every 250 ms (≤4 Hz, D8 ceiling). Enqueues a
    /// `Playing` report on each successful position sample; `Paused` /
    /// `Stopped` reports are enqueued by the command handlers and flushed
    /// here as well. The return value (follow-up `AudioCommand` JSON) is
    /// freed immediately — the TUI already drives mpv directly and does not
    /// need kernel-initiated follow-up commands at this stage.
    pub fn poll_audio_position(&self) {
        let Some(host) = AUDIO_HOST.get() else {
            return;
        };
        let reports = {
            let mut h = host.lock().unwrap();
            h.poll_position();
            h.drain_reports()
        };

        let handle_addr = match PODCAST_HANDLE.get() {
            Some(&addr) => addr,
            None => return,
        };
        let handle = handle_addr as *mut PodcastHandle;

        for report in reports {
            let report_json = match serde_json::to_string(&report) {
                Ok(j) => j,
                Err(_) => continue,
            };
            let Ok(c_json) = CString::new(report_json) else {
                continue;
            };
            // SAFETY: handle outlives the process (freed only in AppRuntime::drop).
            // nmp_app_podcast_audio_report degrades silently on any error (D6).
            let ret = nmp_app_podcast_audio_report(handle, c_json.as_ptr());
            if !ret.is_null() {
                free_owned_c_string(ret);
            }
        }
    }

    pub(crate) fn provider_model_catalog(&self) -> Result<Vec<ProviderCatalogModel>> {
        if self.podcast.is_null() {
            return Err("podcast handle unavailable".to_owned());
        }
        let ptr = nmp_app_podcast_provider_model_catalog(self.podcast);
        if ptr.is_null() {
            return Err("provider catalog returned null".to_owned());
        }
        let text = take_owned_c_string(ptr, "provider catalog")?;
        decode_provider_catalog(&text)
    }

    pub(crate) fn elevenlabs_voice_catalog(&self) -> Result<Vec<ProviderCatalogVoice>> {
        if self.podcast.is_null() {
            return Err("podcast handle unavailable".to_owned());
        }
        let ptr = nmp_app_podcast_elevenlabs_voice_catalog(self.podcast);
        if ptr.is_null() {
            return Err("voice catalog returned null".to_owned());
        }
        let text = take_owned_c_string(ptr, "voice catalog")?;
        decode_elevenlabs_voice_catalog(&text)
    }

    pub(crate) fn speech_model_catalog(&self) -> Result<SpeechModelCatalog> {
        if self.podcast.is_null() {
            return Err("podcast handle unavailable".to_owned());
        }
        let ptr = nmp_app_podcast_speech_model_catalog(self.podcast);
        if ptr.is_null() {
            return Err("speech catalog returned null".to_owned());
        }
        let text = take_owned_c_string(ptr, "speech catalog")?;
        decode_speech_model_catalog(&text)
    }

    pub(crate) fn local_model_catalog(&self) -> Result<LocalModelCatalog> {
        if self.podcast.is_null() {
            return Err("podcast handle unavailable".to_owned());
        }
        let ptr = nmp_app_podcast_local_model_catalog(self.podcast);
        if ptr.is_null() {
            return Err("local model catalog returned null".to_owned());
        }
        let text = take_owned_c_string(ptr, "local model catalog")?;
        decode_local_model_catalog(&text)
    }

    /// Read the current podcast state directly from the handle.
    ///
    /// This is the Rust-native path — no JSON round-trip. Called on the
    /// main UI thread when the kernel update callback fires.
    pub fn podcast_update(&self) -> Option<PodcastUpdate> {
        if self.podcast.is_null() {
            return None;
        }
        Some(unsafe { (*self.podcast).update() })
    }
}

fn dispatch_capability_request(request_str: &str) -> String {
    let req: nmp_core::substrate::CapabilityRequest = match serde_json::from_str(request_str) {
        Ok(r) => r,
        Err(e) => return error_envelope("unknown", "", &format!("parse error: {e}")),
    };

    let result_json = match req.namespace.as_str() {
        AUDIO_CAPABILITY_NAMESPACE => {
            if let Some(host) = AUDIO_HOST.get() {
                host.lock().unwrap().handle_request(request_str)
            } else {
                serde_json::json!({"ok": false, "error": "audio host not initialized"}).to_string()
            }
        }
        HTTP_CAPABILITY_NAMESPACE => handle_http(&req.payload_json),
        HTTP_ASYNC_CAPABILITY_NAMESPACE => handle_http_async(&req.payload_json),
        ns => serde_json::json!({"ok": false, "error": format!("stub: {ns}")}).to_string(),
    };

    serde_json::to_string(&nmp_core::substrate::CapabilityEnvelope {
        namespace: req.namespace,
        correlation_id: req.correlation_id,
        result_json,
    })
    .unwrap_or_else(|_| "{}".to_string())
}

fn handle_http(payload_json: &str) -> String {
    let http_req: HttpRequest = match serde_json::from_str(payload_json) {
        Ok(r) => r,
        Err(e) => {
            let res = HttpResult::Error {
                message: format!("decode: {e}"),
            };
            return serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string());
        }
    };

    let res = run_http(http_req);
    serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string())
}

/// Async HTTP capability executor.
///
/// Decodes the [`HttpCommand`], runs the transport off the actor thread (so the
/// kernel actor is never blocked on the RSS download), and reports the result
/// back through `nmp_app_podcast_http_report`. Returns an immediate `accepted`
/// ack as the inner `result_json` — `dispatch_capability_request` wraps it in
/// the [`CapabilityEnvelope`], so this must *not* build an envelope itself.
fn handle_http_async(payload_json: &str) -> String {
    let command: HttpCommand = match serde_json::from_str(payload_json) {
        Ok(c) => c,
        Err(e) => {
            // No `request_id` to report back with; degrade as a decode error
            // (D6 — never panic across the FFI boundary).
            return serde_json::json!({"ok": false, "error": format!("decode: {e}")}).to_string();
        }
    };

    // The capability callback registers a null `ctx`, so the handle is reached
    // via the published `PODCAST_HANDLE` (stored as `usize` for `Send`).
    let handle_addr = match PODCAST_HANDLE.get() {
        Some(addr) => *addr,
        None => {
            return serde_json::json!({"ok": false, "error": "podcast handle unavailable"})
                .to_string()
        }
    };

    std::thread::spawn(move || {
        let result = run_http(command.request);
        let report = HttpReport {
            request_id: command.request_id,
            result,
        };
        let report_json = match serde_json::to_string(&report) {
            Ok(json) => json,
            Err(_) => return,
        };
        let Ok(report_cstr) = CString::new(report_json) else {
            return;
        };
        // SAFETY: the handle outlives the process — it is only freed in
        // `AppRuntime::drop` at shutdown, after the capability callback is torn
        // down. `apply_report` touches only the shared store / signal Arcs.
        let handle = handle_addr as *mut PodcastHandle;
        let ret = nmp_app_podcast_http_report(handle, report_cstr.as_ptr());
        // The report FFI always returns NULL (no follow-up), but free defensively.
        if !ret.is_null() {
            free_owned_c_string(ret);
        }
    });

    serde_json::json!({"status": "accepted"}).to_string()
}

/// Run an [`HttpRequest`] over the blocking transport and return an
/// [`HttpResult`]. Shared by the synchronous ([`handle_http`]) and async
/// ([`handle_http_async`]) capability paths so transport behavior is identical.
fn run_http(http_req: HttpRequest) -> HttpResult {
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
                reqwest::header::HeaderName::from_bytes(pair[0].as_bytes()),
                reqwest::header::HeaderValue::from_str(&pair[1]),
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
    let envelope = nmp_core::substrate::CapabilityEnvelope {
        namespace: namespace.to_owned(),
        correlation_id: correlation_id.to_owned(),
        result_json: serde_json::json!({"ok": false, "error": msg}).to_string(),
    };
    serde_json::to_string(&envelope).unwrap_or_else(|_| "{}".to_string())
}

fn take_owned_c_string(ptr: *mut c_char, function_name: &str) -> Result<String> {
    if ptr.is_null() {
        return Err(format!("{function_name} returned null"));
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    free_owned_c_string(ptr);
    Ok(text)
}

fn free_owned_c_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        let _ = unsafe { CString::from_raw(ptr) };
    }
}

#[cfg(test)]
fn parse_dispatch_envelope(value: &Value) -> Result<String> {
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(error.to_string());
    }
    if value.get("ok").is_some() {
        parse_action_result(value)?;
    }
    if let Some(result_json) = value.get("result_json").and_then(Value::as_str) {
        parse_result_json(result_json)?;
    }
    value
        .get("correlation_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "action dispatch envelope missing correlation_id".to_string())
}

#[cfg(test)]
fn parse_result_json(result_json: &str) -> Result<()> {
    let trimmed = result_json.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let value: Value = serde_json::from_str(trimmed)
        .map_err(|e| format!("action result returned invalid JSON: {e}"))?;
    parse_action_result(&value)
}

#[cfg(test)]
fn parse_action_result(value: &Value) -> Result<()> {
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(action_error_message(value));
    }
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(error.to_string());
    }
    Ok(())
}

#[cfg(test)]
fn action_error_message(value: &Value) -> String {
    value
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            value
                .get("status")
                .and_then(Value::as_str)
                .map(|status| format!("action failed: {status}"))
        })
        .unwrap_or_else(|| "action failed".to_owned())
}

impl Drop for AppRuntime {
    fn drop(&mut self) {
        bridge::unregister(self.app_ptr());
        self.update_bridge.take();
        if !self.podcast.is_null() {
            nmp_app_podcast_unregister(self.podcast);
            self.podcast = std::ptr::null_mut();
        }
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
