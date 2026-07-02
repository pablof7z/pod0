use std::sync::mpsc::{self, Receiver, Sender};

use nmp_native_runtime::NmpApp;

/// Lightweight signal that the kernel has emitted a new snapshot.
/// The actual payload is read on the main thread via
/// `AppRuntime::podcast_update` so we never block the listener
/// callback thread with store locks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NmpEvent;

pub struct NmpUpdateBridge {
    tx: Sender<NmpEvent>,
}

impl NmpUpdateBridge {
    #[must_use]
    pub fn channel() -> (Box<Self>, Receiver<NmpEvent>) {
        let (tx, rx) = mpsc::channel();
        (Box::new(Self { tx }), rx)
    }

    /// Register `bridge` as `app`'s update listener.
    ///
    /// `nmp_native_runtime::NmpApp::set_update_listener` takes a plain Rust
    /// closure (`Arc<dyn Fn(&[u8]) + Send + Sync>`) instead of the deleted
    /// `nmp-ffi` C-ABI context-pointer + callback-fn pair — no `unsafe`
    /// context juggling needed.
    pub fn register(app: *mut NmpApp, bridge: &mut Box<Self>) {
        if app.is_null() {
            return;
        }
        let tx = bridge.tx.clone();
        // SAFETY: app is non-null (checked above) and owned by the host for
        // the lifetime of this call.
        let app_ref = unsafe { &*app };
        app_ref.set_update_listener(Some(std::sync::Arc::new(move |_bytes: &[u8]| {
            let _ = tx.send(NmpEvent);
        })));
    }
}

pub fn unregister(app: *mut NmpApp) {
    if app.is_null() {
        return;
    }
    // SAFETY: app is non-null (checked above).
    let app_ref = unsafe { &*app };
    app_ref.set_update_listener(None);
}
