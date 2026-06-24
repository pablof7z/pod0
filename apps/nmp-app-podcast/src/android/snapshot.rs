//! Snapshot and action-dispatch JNI entry points — nativeDispatchAction,
//! nativeNextUpdate, nativePodcastSnapshot.
//!
//! Doctrine: D6 — every entry point degrades silently on null / poison /
//! serde failure. No business logic lives here.

use std::ffi::{CStr, CString};

use jni::objects::{JClass, JString};
use jni::sys::{jlong, jstring};
use jni::JNIEnv;

use nmp_ffi::{nmp_app_dispatch_action, nmp_free_string};

use crate::ffi::{nmp_app_podcast_snapshot, nmp_app_podcast_snapshot_free};
use crate::ffi::guard::ffi_guard;
use super::session_ref;

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

/// `nativeNextUpdate(handle)` — blocking drain of the snapshot channel.
/// Returns `null` when the session is shut down or the channel closes.
/// Blocks until a frame arrives or shutdown is initiated.
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
        crossbeam_channel::select! {
            recv(s.rx) -> msg => match msg {
                Ok(json) => match env.new_string(json) {
                    Ok(js) => js.into_raw(),
                    Err(_) => null,
                },
                Err(_) => null,  // channel closed: nativeFree dropped the sender
            },
            recv(s.shutdown_rx_update) -> _ => null,  // explicit shutdown
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_channel_delivers_sent_frame() {
        let (tx, rx) = crossbeam_channel::unbounded::<String>();
        let (shutdown_tx, shutdown_rx) = crossbeam_channel::bounded::<()>(1);

        // Send a test message
        let test_msg = "test frame".to_string();
        tx.send(test_msg.clone()).expect("send failed");

        // Simulate the select! logic
        crossbeam_channel::select! {
            recv(rx) -> msg => {
                assert_eq!(msg.ok(), Some(test_msg));
            },
            recv(shutdown_rx) -> _ => {
                panic!("should have received from rx");
            },
        }
    }

    #[test]
    fn test_shutdown_unblocks_update_receiver() {
        let (_tx, rx) = crossbeam_channel::unbounded::<String>();
        let (shutdown_tx, shutdown_rx) = crossbeam_channel::bounded::<()>(1);

        // Signal shutdown
        shutdown_tx.send(()).expect("send shutdown failed");

        // Simulate the select! logic with no data on rx
        let mut received_from_shutdown = false;
        crossbeam_channel::select! {
            recv(rx) -> msg => {
                assert!(msg.is_err()); // rx is closed
            },
            recv(shutdown_rx) -> _ => {
                received_from_shutdown = true;
            },
        }
        assert!(received_from_shutdown, "should have received shutdown signal");
    }

    #[test]
    fn test_signer_channel_delivers_request() {
        let (tx, rx) = crossbeam_channel::unbounded::<String>();
        let (shutdown_tx, shutdown_rx) = crossbeam_channel::bounded::<()>(1);

        // Send a signer request
        let request_msg = r#"{"id":"1","method":"get_public_key"}"#.to_string();
        tx.send(request_msg.clone()).expect("send failed");

        // Simulate the select! logic
        crossbeam_channel::select! {
            recv(rx) -> msg => {
                assert_eq!(msg.ok(), Some(request_msg));
            },
            recv(shutdown_rx) -> _ => {
                panic!("should have received from rx");
            },
        }
    }

    #[test]
    fn test_signer_shutdown_unblocks() {
        let (_tx, rx) = crossbeam_channel::unbounded::<String>();
        let (shutdown_tx, shutdown_rx) = crossbeam_channel::bounded::<()>(1);

        // Signal shutdown
        shutdown_tx.send(()).expect("send shutdown failed");

        // Simulate the select! logic with no data on rx
        let mut received_from_shutdown = false;
        crossbeam_channel::select! {
            recv(rx) -> msg => {
                assert!(msg.is_err()); // rx is closed
            },
            recv(shutdown_rx) -> _ => {
                received_from_shutdown = true;
            },
        }
        assert!(received_from_shutdown, "should have received shutdown signal");
    }
}
