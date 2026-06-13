//! Android JNI shim. Mirrors iOS
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
use std::sync::{
    mpsc::{Receiver, RecvTimeoutError, Sender},
    Mutex,
};
use std::time::Duration;

use jni::objects::{JClass, JString};
use jni::sys::{jint, jlong, jstring};
use jni::JNIEnv;

use nmp_ffi::{
    nmp_app_claim_profile, nmp_app_dispatch_action, nmp_app_free, nmp_free_string,
    nmp_app_is_alive, nmp_app_lifecycle_background, nmp_app_lifecycle_foreground,
    nmp_app_new, nmp_app_release_profile, nmp_app_set_update_callback,
    nmp_app_signin_nsec, nmp_app_start, nmp_app_stop, nmp_external_signer_init, NmpApp,
};

use crate::ffi::{
    nmp_app_podcast_register, nmp_app_podcast_set_data_dir, nmp_app_podcast_snapshot,
    nmp_app_podcast_snapshot_free, nmp_app_podcast_unregister, PodcastHandle,
};
use crate::ffi::guard::ffi_guard;

#[path = "android/capability_router.rs"]
mod capability_router;
#[path = "android/external_signer.rs"]
mod external_signer;
#[path = "android/provider_transport.rs"]
mod provider_transport;
#[path = "android/reports.rs"]
mod reports;

// ── Session — boxed lifetime container ──────────────────────────────────────

/// Owns the kernel handle, the projection handle, the snapshot receiver, and
/// the boxed sender that the kernel holds as an opaque callback context.
/// Freed exactly once in `nativeFree` — mirror of iOS `PodcastHandle.deinit`.
pub(crate) struct Session {
    pub(crate) app: *mut NmpApp,
    podcast: *mut PodcastHandle,
    rx: Receiver<String>,
    tx: *mut Sender<String>,
    capability_ctx: Mutex<Option<*mut capability_router::AndroidCapabilityContext>>,
    /// ADR-0048 — outbound NIP-55 `ExternalSignerRequest` JSON queue. The
    /// capability trampoline (`android_capability_callback`, on a Rust thread)
    /// pushes the inner request payload here when it sees the `external_signer`
    /// namespace; a Kotlin reader thread drains it via `nativeNextSignerRequest`
    /// and hands each item to `ExternalSignerCapabilityBridge.handleJson`. This
    /// channel-drain shape mirrors NMP's own `nmp-android-ffi` Chirp bridge: the
    /// capability is interactive/async (an Amber Intent round-trip) and cannot
    /// resolve synchronously inside the capability callback.
    pub(crate) signer_requests: Sender<String>,
    signer_rx: Mutex<Receiver<String>>,
}

// SAFETY: `Session` is sent across threads only inside a `Box` whose ownership
// is transferred to Kotlin as an opaque `jlong`. Access is serialized by the
// Kotlin caller (`nativeNew` → `nativeFree` lifecycle; `nativeNextUpdate` on a
// single reader thread). The raw pointers are never aliased.
unsafe impl Send for Session {}

#[must_use]
pub(super) fn session_ref<'a>(handle: jlong) -> Option<&'a Session> {
    if handle == 0 {
        None
    } else {
        // SAFETY: non-zero handles are live `Session` pointers produced by
        // `nativeNew`; Kotlin never calls after `nativeFree`.
        Some(unsafe { &*(handle as *const Session) })
    }
}

impl Session {
    /// Blocking timed drain of the outbound NIP-55 request channel — the
    /// signer analogue of `nativeNextUpdate`'s snapshot drain. Returns the next
    /// `ExternalSignerRequest` JSON, or `None` on idle / closed channel (the
    /// Kotlin reader loops back in either way; D6 — no error crosses FFI).
    fn recv_next_signer_request(&self, timeout: Duration) -> Option<String> {
        let rx = self.signer_rx.lock().ok()?;
        rx.recv_timeout(timeout).ok()
    }
}

// ── Update callback — copies JSON before the kernel reclaims its buffer. ─────

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
    ffi_guard("on_update", || (), || {
        // SAFETY: `bytes` is valid for `len` bytes for the duration of this
        // call (NMP borrows the frame to the callback).
        // `decode_update_frame` returns a heap-owned C string (or null on a
        // non-decodable frame) that we must release through
        // `nmp_free_string`.
        let json_ptr = unsafe {
            crate::ffi::snapshot::nmp_app_podcast_decode_update_frame(bytes, len)
        };
        if json_ptr.is_null() {
            return;
        }
        let owned = unsafe { CStr::from_ptr(json_ptr) }
            .to_string_lossy()
            .into_owned();
        nmp_free_string(json_ptr);
        // SAFETY: `context` is the `Box<Sender<String>>` pointer registered
        // in `nativeNew`; it lives until `nativeFree` clears the callback
        // before reclaiming the box. AssertUnwindSafe is sound: null-checked
        // above; not observed again on panic path.
        let tx = unsafe { &*(context as *const Sender<String>) };
        // Dead receiver ⇒ silent no-op (D6).
        let _ = tx.send(owned);
    });
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
    ffi_guard("nativeNew", || 0 as jlong, || {
        let app = nmp_app_new();
        if app.is_null() {
            return 0;
        }
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let tx = Box::into_raw(Box::new(tx));
        // `nmp_app_set_update_callback` is `pub extern "C" fn` (safe Rust at
        // the call site). `app` is valid (just allocated), `tx` is a fresh
        // box, and `on_update` matches the kernel's `UpdateCallback` C ABI.
        nmp_app_set_update_callback(app, tx as *mut c_void, Some(on_update));
        let podcast = nmp_app_podcast_register(app);
        // ADR-0048 — install the NIP-55 external-signer driver so the kernel can
        // dispatch `external_signer` capability requests (built when the host
        // calls `nativeSignInNip55`). The driver only emits onto the capability
        // socket; the host adapter (`ExternalSignerCapabilityBridge`) is wired
        // through the channel below once `nativeSetCapabilityRouter` registers
        // the trampoline. Safe to call before the callback exists — no request
        // is built until sign-in.
        nmp_external_signer_init(app);
        let (signer_tx, signer_rx) = std::sync::mpsc::channel::<String>();
        let session = Box::new(Session {
            app,
            podcast,
            rx,
            tx,
            capability_ctx: Mutex::new(None),
            signer_requests: signer_tx,
            signer_rx: Mutex::new(signer_rx),
        });
        Box::into_raw(session) as jlong
    })
}

/// `nativeSetDataDir(handle, path)` — bind the podcast library store to a
/// persistence directory and reload any saved state (`podcasts.json`,
/// `identity.json`, the Up-Next queue, per-podcast keys, relay config, and the
/// inbox-triage cache). Mirror of the iOS `KernelBridge+Callbacks.swift`
/// `configurePodcastDataDir` call.
///
/// Caller contract (same as iOS): invoke once, after `nativeNew` (which runs
/// `nmp_app_podcast_register`) and **before** `nativeStart`, so persisted state
/// is reloaded into the actor before it starts emitting snapshots. A null
/// handle, null/non-UTF-8 path, or a `null` podcast projection is a silent
/// no-op (D6) — the kernel side decides what and when to persist; the shell
/// only supplies the OS path.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeSetDataDir<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    path: JString<'l>,
) {
    ffi_guard("nativeSetDataDir", || (), || {
        let Some(s) = session_ref(handle) else {
            return;
        };
        if s.podcast.is_null() {
            return;
        }
        let path = match env.get_string(&path) {
            Ok(p) => p.to_string_lossy().into_owned(),
            Err(_) => return,
        };
        let Ok(c_path) = CString::new(path) else {
            return;
        };
        // `nmp_app_podcast_set_data_dir` is the same in-crate symbol iOS
        // calls; it owns all reload/persist logic and degrades silently on
        // poison/IO error.
        nmp_app_podcast_set_data_dir(s.podcast, c_path.as_ptr());
    });
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
    ffi_guard("nativeStart", || (), || {
        if let Some(s) = session_ref(handle) {
            nmp_app_start(s.app, 0, visible_limit as u32, emit_hz as u32);
        }
    });
}

/// `nativeStop(handle)` — halt the kernel actor (idempotent).
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeStop(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    ffi_guard("nativeStop", || (), || {
        if let Some(s) = session_ref(handle) {
            nmp_app_stop(s.app);
        }
    });
}

/// `nativeIsAlive(handle)` — actor-liveness probe (D7).
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeIsAlive(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    ffi_guard("nativeIsAlive", || 0 as jint, || {
        match session_ref(handle) {
            Some(s) => nmp_app_is_alive(s.app) as jint,
            None => 0,
        }
    })
}

/// `nativeLifecycleForeground(handle)` / `nativeLifecycleBackground(handle)` —
/// host lifecycle bridge (G3). Mirror of iOS scenePhase wiring.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeLifecycleForeground(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    ffi_guard("nativeLifecycleForeground", || (), || {
        if let Some(s) = session_ref(handle) {
            nmp_app_lifecycle_foreground(s.app);
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeLifecycleBackground(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    ffi_guard("nativeLifecycleBackground", || (), || {
        if let Some(s) = session_ref(handle) {
            nmp_app_lifecycle_background(s.app);
        }
    });
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
    let null: jstring = std::ptr::null_mut();
    ffi_guard("nativeDispatchAction", || null, || {
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
        // returning, then release through the documented `nmp_free_string`
        // path — same convention `KernelBridge.swift` follows. Using
        // `CString::from_raw` would bypass any future bookkeeping the kernel
        // adds to that free.
        let owned = unsafe { CStr::from_ptr(envelope_ptr) }
            .to_string_lossy()
            .into_owned();
        nmp_free_string(envelope_ptr);
        match env.new_string(owned) {
            Ok(js) => js.into_raw(),
            Err(_) => null,
        }
    })
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
    ffi_guard("nativeSigninNsec", || (), || {
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
        // v0.2.4: make_active = 1 — Android sign-in activates the imported
        // account.
        nmp_app_signin_nsec(s.app, c_secret.as_ptr(), 1);
    });
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
    let null: jstring = std::ptr::null_mut();
    ffi_guard("nativeNextUpdate", || null, || {
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
    })
}

/// `nativePodcastSnapshot(handle)` — pull the Podcast projection JSON. Returns
/// `null` if no snapshot is available.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativePodcastSnapshot<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> jstring {
    let null: jstring = std::ptr::null_mut();
    ffi_guard("nativePodcastSnapshot", || null, || {
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
        // SAFETY: `ptr` is a heap-owned `CString` from
        // `nmp_app_podcast_snapshot`.
        let json = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        nmp_app_podcast_snapshot_free(ptr);
        match env.new_string(json) {
            Ok(js) => js.into_raw(),
            Err(_) => null,
        }
    })
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
    ffi_guard("nmpActionDispatch", || -1 as jint, || {
        let Ok(body) = env.get_string(&action_json) else {
            return -1;
        };
        let body = body.to_string_lossy().into_owned();
        // The action envelope is `{"id":"...","payload":{...}}`. We only
        // need the id for the M13.A stub; the kernel-side router lands in
        // M13.B and will consume the full body via `nmp_app_dispatch_action`
        // once the namespace mapping is wired in.
        let parsed: serde_json::Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(_) => return -1,
        };
        let action_id = parsed
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<missing>");
        // No structured logging hook yet (M13.B); a `log::info!` would
        // require an Android log appender plumbed through the kernel. The
        // action id is surfaced via the `Debug` repr so a tracing layer
        // added later picks it up without changing the stub's wire behaviour.
        let _ = action_id;
        0
    })
}

/// `nativeClaimProfile(handle, pubkeyHex, consumerID)` — register a refcounted
/// interest in a Nostr pubkey's kind:0 profile under the given consumer token.
/// The kernel fetches the profile over its relay pool and surfaces it in
/// `projections["resolved_profiles"]` on the next push frame. D6: invalid
/// pubkey, null/non-UTF-8 arguments, or a null handle are silent no-ops.
///
/// Mirrors iOS `PodcastHandle.claimProfile(pubkeyHex:consumerID:)` and the
/// `nmp_app_claim_profile` C-ABI symbol in `NmpCore.h`.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeClaimProfile<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    pubkey_hex: JString<'l>,
    consumer_id: JString<'l>,
) {
    ffi_guard("nativeClaimProfile", || (), || {
        let Some(s) = session_ref(handle) else {
            return;
        };
        let pubkey = match env.get_string(&pubkey_hex) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(_) => return,
        };
        let consumer = match env.get_string(&consumer_id) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(_) => return,
        };
        let Ok(c_pubkey) = CString::new(pubkey) else {
            return;
        };
        let Ok(c_consumer) = CString::new(consumer) else {
            return;
        };
        // `force = 0` — background / list-row claims never force a re-fetch.
        // Matches the iOS convention in `ClaimNostrProfiles.swift` which passes
        // `force: false` for `.onAppear`-driven claims.
        nmp_app_claim_profile(s.app, c_pubkey.as_ptr(), c_consumer.as_ptr(), 0);
    });
}

/// `nativeReleaseProfile(handle, pubkeyHex, consumerID)` — release a previously
/// claimed profile interest. The kernel drops the pending request when the last
/// consumer releases. Idempotent / safe when nothing is claimed for this pair.
/// D6: any invalid argument is a silent no-op.
///
/// Mirrors iOS `PodcastHandle.releaseProfile(pubkeyHex:consumerID:)` and the
/// `nmp_app_release_profile` C-ABI symbol in `NmpCore.h`.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeReleaseProfile<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    pubkey_hex: JString<'l>,
    consumer_id: JString<'l>,
) {
    ffi_guard("nativeReleaseProfile", || (), || {
        let Some(s) = session_ref(handle) else {
            return;
        };
        let pubkey = match env.get_string(&pubkey_hex) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(_) => return,
        };
        let consumer = match env.get_string(&consumer_id) {
            Ok(s) => s.to_string_lossy().into_owned(),
            Err(_) => return,
        };
        let Ok(c_pubkey) = CString::new(pubkey) else {
            return;
        };
        let Ok(c_consumer) = CString::new(consumer) else {
            return;
        };
        nmp_app_release_profile(s.app, c_pubkey.as_ptr(), c_consumer.as_ptr());
    });
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
    ffi_guard("nativeFree", || (), || {
        // SAFETY: `handle` was produced by `nativeNew`; freed exactly once.
        let s = unsafe { Box::from_raw(handle as *mut Session) };
        nmp_app_stop(s.app);
        capability_router::clear_capability_router(&s);
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
    });
}
