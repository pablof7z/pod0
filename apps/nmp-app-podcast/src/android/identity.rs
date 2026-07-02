//! Identity and auth JNI entry points — nsec sign-in, NIP-46 bunker sign-in,
//! profile claim/release, and nostrconnect URI generation.
//!
//! Doctrine: D6 — every entry point degrades silently on null / poison /
//! conversion failure. No business logic lives here.

use jni::objects::{JClass, JString};
use jni::sys::{jint, jlong, jstring};
use jni::JNIEnv;
use nmp_core::{ProfileShape, RefLiveness, RefNamespace, RefShape, SignerSource};
use zeroize::Zeroizing;

use super::session_ref;
use crate::ffi::guard::ffi_guard;

/// `nativeSigninNsec(handle, nsec)` — one-shot sign-in via local nsec.
/// Demonstrates the single capability + dispatch the milestone calls for.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeSigninNsec<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    nsec: JString<'l>,
) {
    ffi_guard(
        "nativeSigninNsec",
        || (),
        || {
            let Some(s) = session_ref(handle) else {
                return;
            };
            let secret = match env.get_string(&nsec) {
                Ok(s) => s.to_string_lossy().into_owned(),
                Err(_) => return,
            };
            // v0.2.4: make_active = 1 — Android sign-in activates the imported
            // account.
            if !s.app.is_null() {
                unsafe { &*s.app }
                    .add_signer(SignerSource::LocalNsec(Zeroizing::new(secret)), true);
            }
        },
    );
}

/// `nativeClaimProfile(handle, pubkeyHex, consumerID)` — register a refcounted
/// interest in a Nostr pubkey's kind:0 profile under the given consumer token.
/// The kernel fetches the profile over its relay pool and surfaces it in
/// `projections["resolved_profiles"]` on the next push frame. D6: invalid
/// pubkey, null/non-UTF-8 arguments, or a null handle are silent no-ops.
///
/// Mirrors iOS `PodcastHandle.claimProfile(pubkeyHex:consumerID:)`. Uses the
/// ADR-0063 Lane D `nmp_app_resolve_ref` entry point (namespace=0/profile,
/// shape=1/profile.card, liveness=0/CacheOk for background list-row claims).
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeClaimProfile<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    pubkey_hex: JString<'l>,
    consumer_id: JString<'l>,
) {
    ffi_guard(
        "nativeClaimProfile",
        || (),
        || {
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
            // ADR-0063 Lane D: namespace=0 (profile), shape=1 (profile.card),
            // liveness=0 (CacheOk — background list-row claims never force a re-fetch).
            if !s.app.is_null() {
                unsafe { &*s.app }.resolve_ref(
                    RefNamespace::Profile,
                    pubkey,
                    consumer,
                    RefShape::Profile(ProfileShape::Card),
                    RefLiveness::CacheOk,
                );
            }
        },
    );
}

/// `nativeReleaseProfile(handle, pubkeyHex, consumerID)` — release a previously
/// claimed profile interest. The kernel drops the pending request when the last
/// consumer releases. Idempotent / safe when nothing is claimed for this pair.
/// D6: any invalid argument is a silent no-op.
///
/// Mirrors iOS `PodcastHandle.releaseProfile(pubkeyHex:consumerID:)`. Uses the
/// ADR-0063 Lane D `nmp_app_release_ref` entry point (namespace=0/profile).
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeReleaseProfile<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    pubkey_hex: JString<'l>,
    consumer_id: JString<'l>,
) {
    ffi_guard(
        "nativeReleaseProfile",
        || (),
        || {
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
            // ADR-0063 Lane D: namespace=0 (profile).
            if !s.app.is_null() {
                unsafe { &*s.app }.release_ref(RefNamespace::Profile, pubkey, consumer);
            }
        },
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// NIP-46 remote-signer JNI wrappers (bunker:// + nostrconnect://)
// ─────────────────────────────────────────────────────────────────────────────

/// `nativeSignInBunker(handle, uri, makeActive)` — enqueue
/// `ActorCommand::SignInBunker` with the supplied `bunker://` URI.
/// Silent no-op (D6) if the broker has not been initialised — which it always
/// is because `nativeNew` calls `NmpApp::init_signer_broker`.
///
/// `makeActive = true` is the only meaningful value for the UX (the user chose
/// this signer to be their active account); pass `true` from Kotlin.
///
/// Mirrors iOS `PodcastHandle.signInBunker(uri:)` and the
/// `nmp_app_signin_bunker` C-ABI symbol in `NmpCore.h`.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeSignInBunker<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    uri: JString<'l>,
    make_active: jint,
) {
    ffi_guard(
        "nativeSignInBunker",
        || (),
        || {
            let Some(s) = session_ref(handle) else {
                return;
            };
            let uri_str = match env.get_string(&uri) {
                Ok(s) => s.to_string_lossy().into_owned(),
                Err(_) => return,
            };
            if !s.app.is_null() {
                unsafe { &*s.app }.add_signer(SignerSource::BunkerUri(uri_str), make_active != 0);
            }
        },
    );
}

/// `nativeCancelBunkerHandshake(handle)` — abort the in-flight NIP-46
/// handshake. Idempotent / safe when no handshake is in flight (D6).
///
/// Mirrors iOS `PodcastHandle.cancelBunkerHandshake()` and the
/// `nmp_app_cancel_bunker_handshake` C-ABI symbol in `NmpCore.h`.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeCancelBunkerHandshake(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    ffi_guard(
        "nativeCancelBunkerHandshake",
        || (),
        || {
            if let Some(s) = session_ref(handle) {
                if !s.app.is_null() {
                    unsafe { &*s.app }.cancel_bunker_handshake();
                }
            }
        },
    );
}

/// `nativeNostrconnectUri(handle, relayUrl, callbackScheme)` — allocate a
/// freshly-generated `nostrconnect://` URI from the broker, copy it to a Java
/// `String`, and free the C buffer.
///
/// Returns `null` when the broker is not initialised or Rust returns a null
/// pointer (D6).
///
/// `relayUrl` is retained only for Kotlin/Swift API compatibility; NMP v0.8
/// always selects the relay from the kernel relay config. `callbackScheme` is
/// optional platform callback information.
#[no_mangle]
pub extern "system" fn Java_io_f7z_podcast_KernelBridge_nativeNostrconnectUri<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    handle: jlong,
    _relay_url: JString<'l>,
    callback_scheme: JString<'l>,
) -> jstring {
    let null: jstring = std::ptr::null_mut();
    ffi_guard(
        "nativeNostrconnectUri",
        || null,
        || {
            let Some(s) = session_ref(handle) else {
                return null;
            };
            let callback: Option<String> = env
                .get_string(&callback_scheme)
                .ok()
                .map(|js| js.to_string_lossy().into_owned());
            if s.app.is_null() {
                return null;
            }
            let Some(owned) = (unsafe { &*s.app }).nostrconnect_uri(callback.as_deref()) else {
                return null;
            };
            match env.new_string(owned) {
                Ok(js) => js.into_raw(),
                Err(_) => null,
            }
        },
    )
}
