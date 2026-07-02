//! Android JNI surface for the NIP-55 external-signer capability (ADR-0048).
//!
//! Three JNI entry points, mirroring NMP's own `nmp-android-ffi` Chirp bridge
//! but threaded through this crate's boxed-pointer `Session` (vs. NMP's session
//! registry) and its single registered capability callback:
//!
//! 1. **`nativeSignInNip55`** — user intent in. Rust builds the
//!    `get_public_key` + permission-batch request (D7 — Kotlin reports intent
//!    only) via `nmp_app_signin_nip55`. The request is emitted onto the
//!    capability socket, intercepted by the trampoline in `capability_router.rs`
//!    (namespace `external_signer`), and pushed onto the session signer channel.
//! 2. **`nativeNextSignerRequest`** — Kotlin's blocking timed drain. The Kotlin
//!    reader thread loops on this and hands each request JSON to
//!    `ExternalSignerCapabilityBridge.handleJson`, which fires the Amber Intent.
//! 3. **`nativeDeliverSignerResponse`** — raw Amber result back into the Rust
//!    driver via `nmp_app_deliver_external_signer_response` (D7 — verbatim; the
//!    driver owns correlation routing and all policy).
//!
//! Doctrine: D5/D8 — pure transport, no business logic or cached state;
//! D6 — every entry point degrades silently on null / poison / serde failure.

use std::ptr;

use jni::objects::{JClass, JObject, JString};
use jni::sys::{jlong, jstring};
use jni::JNIEnv;

use super::session_ref;
use crate::ffi::guard::ffi_guard;

/// `nativeSignInNip55(handle, signerPackage)` — begin a NIP-55 sign-in.
/// `signer_package` may be null ("let the OS resolver pick"); Rust builds the
/// `get_public_key` + permission-batch request. Result-bearing work happens
/// asynchronously through the signer-request channel + `deliverSignerResponse`.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeSignInNip55<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    signer_package: JString<'l>,
) {
    ffi_guard(
        "nativeSignInNip55",
        || (),
        || {
            let Some(s) = session_ref(handle) else {
                return;
            };
            let package = optional_jstring_to_string(&mut env, &signer_package);
            if !s.app.is_null() {
                unsafe { &*s.app }.signin_nip55(package);
            }
        },
    );
}

/// `nativeNextSignerRequest(handle)` — blocking drain of the outbound
/// NIP-55 request channel. Returns one `ExternalSignerRequest` JSON, or `null`
/// when the session is shut down or the channel closes. The signer analogue of
/// `nativeNextUpdate`. Blocks until a request arrives or shutdown is initiated (D6).
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeNextSignerRequest<'l>(
    env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
) -> jstring {
    let null: jstring = ptr::null_mut();
    ffi_guard(
        "nativeNextSignerRequest",
        || null,
        || {
            let Some(s) = session_ref(handle) else {
                return null;
            };
            crossbeam_channel::select! {
                recv(s.signer_rx) -> msg => match msg {
                    Ok(payload) => match env.new_string(payload) {
                        Ok(js) => js.into_raw(),
                        Err(_) => null,
                    },
                    Err(_) => null,  // channel closed
                },
                recv(s.shutdown_rx_signer) -> _ => null,  // explicit shutdown
            }
        },
    )
}

/// `nativeDeliverSignerResponse(handle, responseJson)` — report a raw
/// `ExternalSignerResponse` JSON back to the Rust driver (D7 — verbatim; the
/// driver owns correlation routing and all policy).
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeDeliverSignerResponse<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    response_json: JString<'l>,
) {
    ffi_guard(
        "nativeDeliverSignerResponse",
        || (),
        || {
            let Some(s) = session_ref(handle) else {
                return;
            };
            let response = match env.get_string(&response_json) {
                Ok(value) => value.to_string_lossy().into_owned(),
                Err(_) => return,
            };
            if !s.app.is_null() {
                unsafe { &*s.app }.deliver_external_signer_response(&response);
            }
        },
    );
}

/// Convert a (possibly null) Java string to an owned string, or `None` when
/// the Java reference is null. Mirrors NMP's `nmp-android-ffi` helper so a
/// `null` `signer_package` means "let the OS resolver pick the signer app".
fn optional_jstring_to_string(env: &mut JNIEnv, value: &JString) -> Option<String> {
    let obj: &JObject = AsRef::<JObject>::as_ref(value);
    if obj.as_raw().is_null() {
        return None;
    }
    Some(env.get_string(value).ok()?.to_string_lossy().into_owned())
}
