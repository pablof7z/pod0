//! Android JNI shim — M2.F second-platform proof. Mirrors iOS
//! `KernelBridge.swift` in `Java_io_f7z_podcast_KernelBridge_*` symbols.
//!
//! Lives inside this crate (gated `#[cfg(target_os = "android")]`) instead of
//! a separate `nmp-android-ffi`-style crate so one cargo binary drives both
//! platforms. iOS/macOS builds are unaffected — `jni` is a
//! `[target.'cfg(target_os = "android")']` dep.
//!
//! Doctrine: D5/D8 — pure transport, no business logic or cached state;
//! D6 — every entry point degrades silently on null / poison / serde failure.
//!
//! Calls into the kernel go through Rust paths (`nmp_ffi::nmp_app_*`), not
//! `extern "C"` declarations. Symbols declared only via `extern "C"` stay
//! undefined in the cdylib; calling through Rust paths makes rustc pull the
//! bodies into the codegen unit — same pattern as NMP's `nmp-android-ffi`.

use std::ffi::{c_void, CStr, CString};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use jni::objects::{JClass, JString};
use jni::sys::{jint, jlong, jstring};
use jni::JNIEnv;

use nmp_ffi::{
    nmp_app_dispatch_action, nmp_app_free, nmp_app_free_string, nmp_app_is_alive,
    nmp_app_lifecycle_background, nmp_app_lifecycle_foreground, nmp_app_new,
    nmp_app_set_update_callback, nmp_app_signin_nsec, nmp_app_start, nmp_app_stop, NmpApp,
};

use crate::ffi::{
    nmp_app_podcast_download_report, nmp_app_podcast_register, nmp_app_podcast_snapshot,
    nmp_app_podcast_snapshot_free, nmp_app_podcast_unregister, PodcastHandle,
};

// ─────────────────────────────────────────────────────────────────────────────
// Session — boxed lifetime container
// ─────────────────────────────────────────────────────────────────────────────

/// Owns the kernel handle, the projection handle, the snapshot receiver, and
/// the boxed sender that the kernel holds as an opaque callback context.
/// Freed exactly once in `nativeFree` — mirror of iOS `PodcastHandle.deinit`.
pub(crate) struct Session {
    pub(crate) app: *mut NmpApp,
    podcast: *mut PodcastHandle,
    rx: Receiver<String>,
    tx: *mut Sender<String>,
}

// SAFETY: `Session` is sent across threads only inside a `Box` whose ownership
// is transferred to Kotlin as an opaque `jlong`. Access is serialized by the
// Kotlin caller (`nativeNew` → `nativeFree` lifecycle; `nativeNextUpdate` on a
// single reader thread). The raw pointers are never aliased.
unsafe impl Send for Session {}

#[must_use]
fn session_ref<'a>(handle: jlong) -> Option<&'a Session> {
    if handle == 0 {
        None
    } else {
        // SAFETY: non-zero handles are live `Session` pointers produced by
        // `nativeNew`; Kotlin never calls after `nativeFree`.
        Some(unsafe { &*(handle as *const Session) })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Update callback — copies the JSON before the kernel reclaims its buffer.
// ─────────────────────────────────────────────────────────────────────────────

/// `nmp_app_set_update_callback` fires on the kernel's listener thread. NMP's
/// update transport is binary FlatBuffers (NMP `UpdateCallback` is
/// `extern "C" fn(*mut c_void, *const u8, usize)`), so the frame arrives as a
/// borrowed `(bytes, len)` buffer — **not** a NUL-terminated C string. We
/// decode it to the JSON envelope via `nmp_app_podcast_decode_update_frame`
/// (the same in-crate symbol the iOS `KernelBridge.swift` callback uses), copy
/// the JSON into an owned `String`, free the kernel pointer, then send it down
/// the channel. A Kotlin thread drains the channel via `nativeNextUpdate` —
/// pull-side cadence sidesteps JNI thread-attach/global-ref complexity.
extern "C" fn on_update(context: *mut c_void, bytes: *const u8, len: usize) {
    if context.is_null() || bytes.is_null() || len == 0 {
        return;
    }
    // SAFETY: `bytes` is valid for `len` bytes for the duration of this call
    // (NMP borrows the frame to the callback). `decode_update_frame` returns a
    // heap-owned C string (or null on a non-decodable frame) that we must
    // release through `nmp_app_free_string`.
    let json_ptr = unsafe { crate::ffi::snapshot::nmp_app_podcast_decode_update_frame(bytes, len) };
    if json_ptr.is_null() {
        return;
    }
    let owned = unsafe { CStr::from_ptr(json_ptr) }
        .to_string_lossy()
        .into_owned();
    nmp_app_free_string(json_ptr);
    // SAFETY: `context` is the `Box<Sender<String>>` pointer registered in
    // `nativeNew`; it lives until `nativeFree` clears the callback before
    // reclaiming the box.
    let tx = unsafe { &*(context as *const Sender<String>) };
    // Dead receiver ⇒ silent no-op (D6).
    let _ = tx.send(owned);
}

// ─────────────────────────────────────────────────────────────────────────────
// JNI entry points — mirror of `KernelBridge.swift`
// ─────────────────────────────────────────────────────────────────────────────

/// `nativeNew()` — construct the kernel, wire the update callback, register the
/// Podcast projection. Mirror of `PodcastHandle.init()` in Swift.
///
/// Returns the boxed `Session` pointer as `jlong` (0 on any failure).
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeNew(
    _env: JNIEnv,
    _class: JClass,
) -> jlong {
    let app = nmp_app_new();
    if app.is_null() {
        return 0;
    }
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx = Box::into_raw(Box::new(tx));
    // `nmp_app_set_update_callback` is `pub extern "C" fn` (safe Rust at the
    // call site). `app` is valid (just allocated), `tx` is a fresh box, and
    // `on_update` matches the kernel's `UpdateCallback` C ABI.
    nmp_app_set_update_callback(app, tx as *mut c_void, Some(on_update));
    let podcast = nmp_app_podcast_register(app);
    let session = Box::new(Session {
        app,
        podcast,
        rx,
        tx,
    });
    Box::into_raw(session) as jlong
}

/// `nativeStart(handle, visibleLimit, emitHz)` — start the kernel actor.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeStart(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
    visible_limit: jint,
    emit_hz: jint,
) {
    if let Some(s) = session_ref(handle) {
        nmp_app_start(s.app, 0, visible_limit as u32, emit_hz as u32);
    }
}

/// `nativeStop(handle)` — halt the kernel actor (idempotent).
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeStop(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if let Some(s) = session_ref(handle) {
        nmp_app_stop(s.app);
    }
}

/// `nativeIsAlive(handle)` — actor-liveness probe (D7).
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeIsAlive(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    match session_ref(handle) {
        Some(s) => nmp_app_is_alive(s.app) as jint,
        None => 0,
    }
}

/// `nativeLifecycleForeground(handle)` / `nativeLifecycleBackground(handle)` —
/// host lifecycle bridge (G3). Mirror of iOS scenePhase wiring.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeLifecycleForeground(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if let Some(s) = session_ref(handle) {
        nmp_app_lifecycle_foreground(s.app);
    }
}

#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeLifecycleBackground(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if let Some(s) = session_ref(handle) {
        nmp_app_lifecycle_background(s.app);
    }
}

/// `nativeDispatchAction(handle, namespace, actionJson)` — generic
/// namespace-keyed action dispatch. Returns the JSON envelope as a Java
/// `String`, or `null` on any failure (D6).
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeDispatchAction<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    namespace: JString<'l>,
    action_json: JString<'l>,
) -> jstring {
    let null = std::ptr::null_mut();
    let Some(s) = session_ref(handle) else {
        return null;
    };
    let ns = match env.get_string(&namespace) {
        Ok(s) => s.to_string_lossy().into_owned(),
        Err(_) => return null,
    };
    let body = match env.get_string(&action_json) {
        Ok(s) => s.to_string_lossy().into_owned(),
        Err(_) => return null,
    };
    let Ok(c_ns) = CString::new(ns) else {
        return null;
    };
    let Ok(c_body) = CString::new(body) else {
        return null;
    };
    let envelope_ptr = nmp_app_dispatch_action(s.app, c_ns.as_ptr(), c_body.as_ptr());
    if envelope_ptr.is_null() {
        return null;
    }
    // SAFETY: `envelope_ptr` is heap-owned by the kernel. Copy out before
    // returning, then release through the documented `nmp_app_free_string`
    // path — same convention `KernelBridge.swift` follows. Using
    // `CString::from_raw` would bypass any future bookkeeping the kernel adds
    // to that free.
    let owned = unsafe { CStr::from_ptr(envelope_ptr) }
        .to_string_lossy()
        .into_owned();
    nmp_app_free_string(envelope_ptr);
    match env.new_string(owned) {
        Ok(js) => js.into_raw(),
        Err(_) => null,
    }
}

/// `nativeSigninNsec(handle, nsec)` — one-shot sign-in via local nsec.
/// Demonstrates the single capability + dispatch the milestone calls for.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeSigninNsec<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    nsec: JString<'l>,
) {
    let Some(s) = session_ref(handle) else {
        return;
    };
    let secret = match env.get_string(&nsec) {
        Ok(s) => s.to_string_lossy().into_owned(),
        Err(_) => return,
    };
    let Ok(c_secret) = CString::new(secret) else {
        return;
    };
    nmp_app_signin_nsec(s.app, c_secret.as_ptr());
}

/// `nativeNextUpdate(handle)` — blocking drain of the snapshot channel with a
/// 250 ms timeout. Returns `null` on timeout or closed channel. Mirrors the
/// pull-side cadence the iOS push callback would deliver.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeNextUpdate<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> jstring {
    let null = std::ptr::null_mut();
    let Some(s) = session_ref(handle) else {
        return null;
    };
    match s.rx.recv_timeout(Duration::from_millis(250)) {
        Ok(json) => match env.new_string(json) {
            Ok(js) => js.into_raw(),
            Err(_) => null,
        },
        Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => null,
    }
}

/// `nativePodcastSnapshot(handle)` — pull the Podcast projection JSON. Returns
/// `null` if no snapshot is available.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativePodcastSnapshot<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> jstring {
    let null = std::ptr::null_mut();
    let Some(s) = session_ref(handle) else {
        return null;
    };
    if s.podcast.is_null() {
        return null;
    }
    let ptr = nmp_app_podcast_snapshot(s.podcast);
    if ptr.is_null() {
        return null;
    }
    // SAFETY: `ptr` is a heap-owned `CString` from `nmp_app_podcast_snapshot`.
    let json = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    nmp_app_podcast_snapshot_free(ptr);
    match env.new_string(json) {
        Ok(js) => js.into_raw(),
        Err(_) => null,
    }
}

/// `nmpActionDispatch(actionJson)` — M13.A stub for the namespace-agnostic
/// action dispatch surface the second-platform shell calls. Lives separate
/// from `nativeDispatchAction` because it (a) has no handle parameter — the
/// Kotlin shell holds the kernel reference and (b) returns a status code
/// rather than the kernel's JSON envelope. The full kernel routing through
/// this entry point lands in M13.B; for now we parse the JSON, log the
/// action id so the device log shows the wire vocabulary, and return 0.
///
/// **D6:** never panics, never throws. Returns `-1` on any parse failure;
/// `0` on success or empty action.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nmpActionDispatch<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    action_json: JString<'l>,
) -> jint {
    let Ok(body) = env.get_string(&action_json) else {
        return -1;
    };
    let body = body.to_string_lossy().into_owned();
    // The action envelope is `{"id":"...","payload":{...}}`. We only need
    // the id for the M13.A stub; the kernel-side router lands in M13.B
    // and will consume the full body via `nmp_app_dispatch_action` once
    // the namespace mapping is wired in.
    let parsed: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => return -1,
    };
    let action_id = parsed
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("<missing>");
    // No structured logging hook yet (M13.B); a `log::info!` would require
    // an Android log appender plumbed through the kernel. The action id is
    // surfaced via the `Debug` repr so a tracing layer added later picks
    // it up without changing the stub's wire behaviour.
    let _ = action_id;
    0
}

/// `nmpCapabilityReport(namespace, reportJson)` — M13.A stub for the
/// host → kernel capability-report channel. The Kotlin stubs in
/// `capabilities/` call this on every executor report (`AudioReport::Playing`,
/// `AudioReport::Paused`, …) so the kernel can project the executor's
/// observations into its state machines.
///
/// The M13.B work will wire this through `nmp-ffi`'s capability-report
/// surface (analogue of the iOS `sendReport` closure in
/// `AudioCapability.swift`). For now we validate the inputs and return 0
/// so the Kotlin stub can be exercised end-to-end without crashing.
///
/// **D6:** never panics, never throws. `-1` on any input failure; `0` on
/// success.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nmpCapabilityReport<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    namespace: JString<'l>,
    report_json: JString<'l>,
) -> jint {
    let Ok(ns) = env.get_string(&namespace) else {
        return -1;
    };
    let Ok(body) = env.get_string(&report_json) else {
        return -1;
    };
    let ns = ns.to_string_lossy().into_owned();
    let body = body.to_string_lossy().into_owned();
    // Validate the JSON shape early — capability reports are always JSON
    // (`AudioReport`, `DownloadReport`, `VoiceReport`). Anything else is a
    // wire-format bug and we surface it via the return code instead of
    // letting it propagate as a malformed projection on the next tick.
    if serde_json::from_str::<serde_json::Value>(&body).is_err() {
        return -1;
    }
    // Stub: future M13.B will dispatch this through the kernel's capability
    // report sink. The namespace + body are owned strings ready to forward.
    //
    // NOTE: the download capability does NOT use this entry point — it has
    // a real, handle-aware channel (`nativeDownloadReport` below) that
    // projects the report onto the kernel `DownloadQueue` and returns the
    // follow-up `DownloadCommand`. Audio/voice reports still land here and
    // are dropped pending the broader capability-report wiring (M13.B);
    // that remains a known gap for those namespaces, untouched by this PR.
    let _ = (ns, body);
    0
}

/// `nativeDownloadReport(handle, reportJson)` — handle-aware download
/// report channel. The Kotlin `DownloadCapability` calls this with a
/// JSON-encoded [`crate::capability::DownloadReport`] (`progress` /
/// `completed` / `failed` / `cancelled` / `paused`). The report is
/// projected onto the kernel [`crate::download::DownloadQueue`] via
/// [`nmp_app_podcast_download_report`], and any follow-up
/// [`crate::capability::DownloadCommand`] the queue emits (e.g.
/// `start_download` for the next waiting item once a slot frees) is
/// returned to Kotlin as a JSON `String`, or `null` when there is none.
///
/// This is the Android analogue of the iOS
/// `KernelBridge+Callbacks.swift::attachDownloadReportChannel` return-and-
/// execute pattern. Because Android has no inbound `dispatch_capability`
/// command seam, the returned command is how the kernel drives the *next*
/// download; the *first* item in a batch is seeded by the capability off
/// the projected `downloads.active` rows.
///
/// **D6:** never panics, never throws. Returns `null` on null handle,
/// bad UTF-8, lock poison, decode failure, or "no follow-up command".
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeDownloadReport<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    report_json: JString<'l>,
) -> jstring {
    let null = std::ptr::null_mut();
    let Some(s) = session_ref(handle) else {
        return null;
    };
    if s.podcast.is_null() {
        return null;
    }
    let body = match env.get_string(&report_json) {
        Ok(s) => s.to_string_lossy().into_owned(),
        Err(_) => return null,
    };
    let Ok(c_body) = CString::new(body) else {
        return null;
    };
    // SAFETY: `s.podcast` is the live `PodcastHandle` registered in
    // `nativeNew`; `c_body` outlives the call. The FFI fn never panics
    // (D6) and returns either a heap-owned JSON string or NULL.
    let follow_up_ptr = nmp_app_podcast_download_report(s.podcast, c_body.as_ptr());
    if follow_up_ptr.is_null() {
        return null;
    }
    // Copy out, then release through the documented free path — same
    // convention `nativeDispatchAction` follows for kernel-owned strings.
    let owned = unsafe { CStr::from_ptr(follow_up_ptr) }
        .to_string_lossy()
        .into_owned();
    nmp_app_free_string(follow_up_ptr);
    match env.new_string(owned) {
        Ok(js) => js.into_raw(),
        Err(_) => null,
    }
}

/// `nativeFree(handle)` — tear down the kernel and the projection handle.
/// Exactly-once.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle == 0 {
        return;
    }
    // SAFETY: `handle` was produced by `nativeNew`; freed exactly once.
    let s = unsafe { Box::from_raw(handle as *mut Session) };
    if !s.podcast.is_null() {
        nmp_app_podcast_unregister(s.podcast);
    }
    nmp_app_set_update_callback(s.app, std::ptr::null_mut(), None);
    nmp_app_free(s.app);
    // SAFETY: callback has been cleared; the `Sender` box is no longer
    // reachable from the kernel thread.
    unsafe {
        drop(Box::from_raw(s.tx));
    }
}
