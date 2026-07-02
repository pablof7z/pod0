//! App-owned C ABI facade over `nmp-native-runtime`.
//!
//! Podcast still has a hand-written Swift bridge that links `NmpCore.h`.
//! Since current NMP deleted the generic `nmp-ffi` crate, these symbols live in
//! the app crate and forward to typed runtime APIs.

use std::collections::HashMap;
use std::ffi::{c_char, c_uint, c_void, CStr, CString};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use nmp_core::{ProfileShape, RefLiveness, RefNamespace, RefShape, SignerSource};
use nmp_native_runtime::NmpApp;
use zeroize::Zeroizing;

#[path = "runtime_facade_intent.rs"]
mod runtime_facade_intent;
pub use runtime_facade_intent::{
    nmp_app_intent_classify, nmp_app_intent_dispatch, nmp_nip21_decode_uri,
};

type UpdateCallback = extern "C" fn(*mut c_void, *const u8, usize);
type CapabilityCallback = extern "C" fn(*mut c_void, *const c_char) -> *mut c_char;

struct UpdateCallbackSink {
    context: usize,
    callback: UpdateCallback,
}

struct CapabilityCallbackSink {
    context: usize,
    callback: CapabilityCallback,
}

unsafe impl Send for UpdateCallbackSink {}
unsafe impl Sync for UpdateCallbackSink {}
unsafe impl Send for CapabilityCallbackSink {}
unsafe impl Sync for CapabilityCallbackSink {}

fn app_ref<'a>(app: *mut NmpApp) -> Option<&'a NmpApp> {
    if app.is_null() {
        None
    } else {
        Some(unsafe { &*app })
    }
}

fn c_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(str::to_owned)
}

fn optional_c_string(ptr: *const c_char) -> Option<String> {
    c_string(ptr).and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn into_c_string(value: String) -> *mut c_char {
    CString::new(value)
        .unwrap_or_else(|_| c"{\"ok\":false,\"error\":\"serialization-failed\"}".to_owned())
        .into_raw()
}

fn clamp_visible(visible_limit: c_uint) -> usize {
    if visible_limit == 0 {
        nmp_native_runtime::DEFAULT_VISIBLE_LIMIT
    } else {
        visible_limit.clamp(1, 500) as usize
    }
}

fn clamp_emit_hz(emit_hz: c_uint) -> u32 {
    if emit_hz == 0 {
        nmp_native_runtime::DEFAULT_EMIT_HZ
    } else {
        emit_hz.clamp(1, 12)
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_new() -> *mut NmpApp {
    Box::into_raw(Box::new(nmp_native_runtime::new_app()))
}

#[no_mangle]
pub extern "C" fn nmp_app_free(app: *mut NmpApp) {
    if !app.is_null() {
        unsafe {
            drop(Box::from_raw(app));
        }
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_set_update_callback(
    app: *mut NmpApp,
    context: *mut c_void,
    callback: Option<UpdateCallback>,
) {
    let Some(app) = app_ref(app) else {
        return;
    };
    let sink = callback.map(|callback| {
        Box::new(UpdateCallbackSink {
            context: context as usize,
            callback,
        })
    });
    nmp_uniffi_support::set_update_sink(app, sink, |sink, frame| {
        (sink.callback)(sink.context as *mut c_void, frame.as_ptr(), frame.len());
    });
}

#[no_mangle]
pub extern "C" fn nmp_app_set_capability_callback(
    app: *mut NmpApp,
    context: *mut c_void,
    callback: Option<CapabilityCallback>,
) {
    let Some(app) = app_ref(app) else {
        return;
    };
    let sink = callback.map(|callback| {
        Box::new(CapabilityCallbackSink {
            context: context as usize,
            callback,
        })
    });
    nmp_uniffi_support::set_capability_callback(app, sink, |sink, request_json| {
        let Ok(request) = CString::new(request_json.clone()) else {
            return nmp_core::__ffi_internal::capability_error_envelope(&request_json, "invalid-request");
        };
        let raw = (sink.callback)(sink.context as *mut c_void, request.as_ptr());
        if raw.is_null() {
            return nmp_core::__ffi_internal::capability_error_envelope(&request_json, "handler-returned-null");
        }
        unsafe { CString::from_raw(raw) }
            .into_string()
            .unwrap_or_else(|_| {
                nmp_core::__ffi_internal::capability_error_envelope(&request_json, "handler-returned-invalid-utf8")
            })
    });
}

#[no_mangle]
pub extern "C" fn nmp_app_set_storage_path(app: *mut NmpApp, path: *const c_char) {
    if let Some(app) = app_ref(app) {
        let _ = app.set_storage_path(optional_c_string(path));
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_consume_all_builtin_projections(app: *mut NmpApp) {
    if let Some(app) = app_ref(app) {
        app.consume_all_builtin_projections();
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_start(app: *mut NmpApp, visible_limit: c_uint, emit_hz: c_uint) {
    if let Some(app) = app_ref(app) {
        app.start_runtime(clamp_visible(visible_limit), clamp_emit_hz(emit_hz));
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_configure(app: *mut NmpApp, visible_limit: c_uint, emit_hz: c_uint) {
    if let Some(app) = app_ref(app) {
        app.configure_runtime(clamp_visible(visible_limit), clamp_emit_hz(emit_hz));
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_stop(app: *mut NmpApp) {
    if let Some(app) = app_ref(app) {
        app.stop_runtime();
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_reset(app: *mut NmpApp) {
    if let Some(app) = app_ref(app) {
        app.reset_runtime();
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_is_alive(app: *mut NmpApp) -> u8 {
    app_ref(app).map(|app| app.is_alive() as u8).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn nmp_app_lifecycle_foreground(app: *mut NmpApp) {
    if let Some(app) = app_ref(app) {
        app.lifecycle_foreground();
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_lifecycle_background(app: *mut NmpApp) {
    if let Some(app) = app_ref(app) {
        app.lifecycle_background();
    }
}

#[no_mangle]
pub extern "C" fn nmp_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_signin_nsec(app: *mut NmpApp, secret: *const c_char, make_active: u8) {
    let (Some(app), Some(secret)) = (app_ref(app), c_string(secret)) else {
        return;
    };
    app.add_signer(SignerSource::LocalNsec(Zeroizing::new(secret)), make_active != 0);
}

#[no_mangle]
pub extern "C" fn nmp_app_signin_bunker(app: *mut NmpApp, uri: *const c_char, make_active: u8) {
    let (Some(app), Some(uri)) = (app_ref(app), c_string(uri)) else {
        return;
    };
    app.add_signer(SignerSource::BunkerUri(uri), make_active != 0);
}

#[no_mangle]
pub extern "C" fn nmp_signer_broker_init(app: *mut NmpApp) {
    if let Some(app) = app_ref(app) {
        let _ = app.init_signer_broker();
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_cancel_bunker_handshake(app: *mut NmpApp) {
    if let Some(app) = app_ref(app) {
        app.cancel_bunker_handshake();
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_nostrconnect_uri(
    app: *mut NmpApp,
    callback_scheme: *const c_char,
) -> *mut c_char {
    let Some(app) = app_ref(app) else {
        return std::ptr::null_mut();
    };
    app.nostrconnect_uri(optional_c_string(callback_scheme).as_deref())
        .map(into_c_string)
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn nmp_app_remove_account(app: *mut NmpApp, identity_id: *const c_char) {
    let (Some(app), Some(identity_id)) = (app_ref(app), c_string(identity_id)) else {
        return;
    };
    app.remove_account(identity_id);
}

#[no_mangle]
pub extern "C" fn nmp_app_create_new_account(
    app: *mut NmpApp,
    profile_json: *const c_char,
    relays_json: *const c_char,
    mls: bool,
    make_active: u8,
) {
    let (Some(app), Some(profile_json), Some(relays_json)) =
        (app_ref(app), c_string(profile_json), c_string(relays_json))
    else {
        return;
    };
    let Ok(profile) = serde_json::from_str::<HashMap<String, String>>(&profile_json) else {
        app.show_toast("Failed to decode profile JSON".to_string());
        return;
    };
    let Ok(relays) = serde_json::from_str::<Vec<(String, String)>>(&relays_json) else {
        app.show_toast("Failed to decode relays JSON".to_string());
        return;
    };
    app.create_account(profile, relays, Vec::new(), mls, make_active != 0);
}

fn mint_correlation_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{now_ms:016x}{seq:016x}")
}

#[no_mangle]
pub extern "C" fn nmp_app_sign_event_for_return(
    app: *mut NmpApp,
    account_pubkey_hex: *const c_char,
    unsigned_json: *const c_char,
) -> *mut c_char {
    let correlation_id = mint_correlation_id();
    if let Some(app) = app_ref(app) {
        app.sign_event_for_return(
            c_string(account_pubkey_hex).unwrap_or_default(),
            c_string(unsigned_json).unwrap_or_default(),
            correlation_id.clone(),
        );
    }
    into_c_string(correlation_id)
}

fn ref_namespace(value: i32) -> Option<RefNamespace> {
    match value {
        0 => Some(RefNamespace::Profile),
        1 => Some(RefNamespace::Event),
        _ => None,
    }
}

fn ref_shape(namespace: RefNamespace, value: i32) -> RefShape {
    match namespace {
        RefNamespace::Profile => match value {
            0 => RefShape::Profile(ProfileShape::Ref),
            _ => RefShape::Profile(ProfileShape::Card),
        },
        RefNamespace::Event => match value {
            2 => RefShape::Event(nmp_core::EventShape::Embed),
            3 => RefShape::Event(nmp_core::EventShape::Raw),
            _ => RefShape::Event(nmp_core::EventShape::Embed),
        },
    }
}

fn ref_liveness(value: i32) -> RefLiveness {
    if value == 0 {
        RefLiveness::CacheOk
    } else {
        RefLiveness::Live
    }
}

#[no_mangle]
pub extern "C" fn nmp_app_resolve_ref(
    app: *mut NmpApp,
    namespace: i32,
    key: *const c_char,
    consumer_id: *const c_char,
    shape: i32,
    liveness: i32,
) {
    let (Some(app), Some(namespace), Some(key), Some(consumer_id)) =
        (app_ref(app), ref_namespace(namespace), c_string(key), c_string(consumer_id))
    else {
        return;
    };
    app.resolve_ref(namespace, key, consumer_id, ref_shape(namespace, shape), ref_liveness(liveness));
}

#[no_mangle]
pub extern "C" fn nmp_app_release_ref(
    app: *mut NmpApp,
    namespace: i32,
    key: *const c_char,
    consumer_id: *const c_char,
) {
    let (Some(app), Some(namespace), Some(key), Some(consumer_id)) =
        (app_ref(app), ref_namespace(namespace), c_string(key), c_string(consumer_id))
    else {
        return;
    };
    app.release_ref(namespace, key, consumer_id);
}
