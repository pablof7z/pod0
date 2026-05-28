use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, OnceLock};

use nmp_app_podcast::{
    nmp_app_podcast_register, nmp_app_podcast_set_data_dir, nmp_app_podcast_unregister,
    nmp_signer_broker_init, PodcastHandle, AUDIO_CAPABILITY_NAMESPACE,
};
use nmp_app_podcast::ffi::PodcastUpdate;
use nmp_ffi::{
    nmp_app_dispatch_action, nmp_app_free, nmp_app_free_string, nmp_app_new,
    nmp_app_set_capability_callback, nmp_app_start, NmpApp,
};
use podcast_feeds::http::{HttpMethod, HttpRequest, HttpResult, HTTP_CAPABILITY_NAMESPACE};
use serde_json::{json, Value};

use crate::audio_host::AudioHost;
use crate::bridge::{self, NmpEvent};

static AUDIO_HOST: OnceLock<Arc<Mutex<AudioHost>>> = OnceLock::new();

pub struct AppRuntime {
    app: *mut NmpApp,
    podcast: *mut PodcastHandle,
    update_bridge: Option<Box<bridge::NmpUpdateBridge>>,
}

pub type Result<T> = std::result::Result<T, String>;

impl AppRuntime {
    #[must_use]
    pub fn new(data_dir: &Option<String>) -> Result<(Self, Receiver<NmpEvent>)> {
        let app = nmp_app_new();
        if app.is_null() {
            return Err("nmp_app_new returned null".to_string());
        }
        nmp_signer_broker_init(app);

        let audio_host = Arc::new(Mutex::new(AudioHost::new()));
        let _ = AUDIO_HOST.set(audio_host);

        nmp_app_set_capability_callback(
            app,
            std::ptr::null_mut(),
            Some(capability_handler),
        );

        let podcast = nmp_app_podcast_register(app);
        if podcast.is_null() {
            nmp_app_free(app);
            return Err("nmp_app_podcast_register returned null".to_string());
        }

        if let Some(dir) = data_dir {
            let dir_cstr = CString::new(dir.as_str()).map_err(|_| "data_dir contains NUL".to_string())?;
            nmp_app_podcast_set_data_dir(podcast, dir_cstr.as_ptr());
        }

        let (mut bridge, rx) = bridge::NmpUpdateBridge::channel();
        bridge::NmpUpdateBridge::register(app, &mut bridge);

        nmp_app_start(app, 0, 200, 10);

        Ok((
            Self {
                app,
                podcast,
                update_bridge: Some(bridge),
            },
            rx,
        ))
    }

    pub fn dispatch_action(
        &self, namespace: &str, action_json: &str) -> Result<String> {
        let namespace = CString::new(namespace)
            .map_err(|_| "action namespace contains NUL byte".to_string())?;
        let action =
            CString::new(action_json).map_err(|_| "action JSON contains NUL byte".to_string())?;
        let ptr = nmp_app_dispatch_action(self.app, namespace.as_ptr(), action.as_ptr());
        if ptr.is_null() {
            return Err("action dispatch returned null".to_string());
        }
        let text = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        nmp_app_free_string(ptr);
        let value: Value = serde_json::from_str(&text)
            .map_err(|e| format!("action dispatch returned invalid JSON: {e}"))?;
        parse_dispatch_envelope(&value)
    }

    pub fn dispatch_action_value(&self, namespace: &str, action: &Value) -> Result<String> {
        self.dispatch_action(namespace, &action.to_string())
    }

    pub fn play_episode(&self, episode_id: &str, _position_secs: f64) -> Result<String> {
        let action = json!({
            "op": "play",
            "episode_id": episode_id,
        })
        .to_string();
        self.dispatch_action("podcast.player", &action)
    }

    pub fn pause(&self) -> Result<String> {
        let action = json!({"op": "pause"}).to_string();
        self.dispatch_action("podcast.player", &action)
    }

    pub fn resume(&self) -> Result<String> {
        let action = json!({"op": "play"}).to_string();
        self.dispatch_action("podcast.player", &action)
    }

    pub fn seek(&self, position_secs: f64) -> Result<String> {
        let action = json!({
            "op": "seek",
            "position_secs": position_secs,
        })
        .to_string();
        self.dispatch_action("podcast.player", &action)
    }

    pub fn skip_forward(&self) -> Result<String> {
        let action = json!({"op": "skip_forward"}).to_string();
        self.dispatch_action("podcast.player", &action)
    }

    pub fn skip_backward(&self) -> Result<String> {
        let action = json!({"op": "skip_backward"}).to_string();
        self.dispatch_action("podcast.player", &action)
    }

    pub fn set_speed(&self, speed: f32) -> Result<String> {
        let action = json!({
            "op": "set_speed",
            "speed": speed,
        })
        .to_string();
        self.dispatch_action("podcast.player", &action)
    }

    pub fn set_volume(&self, volume: f32) -> Result<String> {
        let action = json!({
            "op": "set_volume",
            "volume": volume,
        })
        .to_string();
        self.dispatch_action("podcast.player", &action)
    }

    pub fn stop(&self) -> Result<String> {
        let action = json!({"op": "stop"}).to_string();
        self.dispatch_action("podcast.player", &action)
    }

    pub fn subscribe(&self, feed_url: &str) -> Result<String> {
        let action = json!({
            "op": "subscribe",
            "feed_url": feed_url,
        })
        .to_string();
        self.dispatch_action("podcast", &action)
    }

    pub fn search_itunes(&self, query: &str) -> Result<String> {
        let action = json!({
            "op": "search_itunes",
            "query": query,
        })
        .to_string();
        self.dispatch_action("podcast", &action)
    }

    pub fn download_episode(&self, episode_id: &str) -> Result<String> {
        let action = json!({
            "op": "download",
            "episode_id": episode_id,
        })
        .to_string();
        self.dispatch_action("podcast", &action)
    }

    pub fn add_to_queue(&self, episode_id: &str) -> Result<String> {
        let action = json!({
            "op": "add_last",
            "episode_id": episode_id,
        })
        .to_string();
        self.dispatch_action("podcast.queue", &action)
    }

    pub fn remove_from_queue(&self, episode_id: &str) -> Result<String> {
        let action = json!({
            "op": "remove",
            "episode_id": episode_id,
        })
        .to_string();
        self.dispatch_action("podcast.queue", &action)
    }

    pub fn mark_played(&self, episode_id: &str) -> Result<String> {
        let action = json!({
            "op": "mark_listened",
            "episode_id": episode_id,
        })
        .to_string();
        self.dispatch_action("podcast.inbox", &action)
    }

    pub fn mark_unplayed(&self, episode_id: &str) -> Result<String> {
        let action = json!({
            "op": "mark_unlistened",
            "episode_id": episode_id,
        })
        .to_string();
        self.dispatch_action("podcast.inbox", &action)
    }

    pub fn star(&self, episode_id: &str) -> Result<String> {
        let action = json!({
            "op": "star_episode",
            "episode_id": episode_id,
            "starred": true,
        })
        .to_string();
        self.dispatch_action("podcast", &action)
    }

    pub fn unstar(&self, episode_id: &str) -> Result<String> {
        let action = json!({
            "op": "star_episode",
            "episode_id": episode_id,
            "starred": false,
        })
        .to_string();
        self.dispatch_action("podcast", &action)
    }

    pub fn poll_audio_position(&self) {
        if let Some(host) = AUDIO_HOST.get() {
            let _ = host.lock().unwrap().poll_position();
        }
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

extern "C" fn capability_handler(_ctx: *mut c_void, request_json: *const c_char) -> *mut c_char {
    let request_str = if request_json.is_null() {
        ""
    } else {
        match unsafe { CStr::from_ptr(request_json) }.to_str() {
            Ok(s) => s,
            Err(_) => "",
        }
    };

    let result_json = dispatch_capability_request(request_str);

    CString::new(result_json)
        .unwrap_or_else(|_| CString::new("{}").unwrap())
        .into_raw()
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
        ns => {
            eprintln!("[podcast-tui] stub capability: {ns}");
            serde_json::json!({"ok": false, "error": format!("stub: {ns}")}).to_string()
        }
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
            let res = HttpResult::Error { message: format!("decode: {e}") };
            return serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string());
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
                reqwest::header::HeaderName::from_bytes(pair[0].as_bytes()),
                reqwest::header::HeaderValue::from_str(&pair[1]),
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
                    serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string())
                }
                Err(e) => {
                    let res = HttpResult::Error { message: format!("body: {e}") };
                    serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string())
                }
            }
        }
        Err(e) => {
            let res = HttpResult::Error { message: format!("transport: {e}") };
            serde_json::to_string(&res).unwrap_or_else(|_| "{}".to_string())
        }
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

fn parse_dispatch_envelope(value: &Value) -> Result<String> {
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(error.to_string());
    }
    value
        .get("correlation_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "action dispatch envelope missing correlation_id".to_string())
}

impl Drop for AppRuntime {
    fn drop(&mut self) {
        if !self.app.is_null() {
            bridge::unregister(self.app);
        }
        self.update_bridge.take();
        if !self.podcast.is_null() {
            nmp_app_podcast_unregister(self.podcast);
            self.podcast = std::ptr::null_mut();
        }
        if !self.app.is_null() {
            nmp_app_free(self.app);
            self.app = std::ptr::null_mut();
        }
    }
}
