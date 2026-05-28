use std::sync::mpsc::{self, Receiver, Sender};

use nmp_ffi::NmpApp;

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

    pub fn register(app: *mut NmpApp, bridge: &mut Box<Self>) {
        let context = bridge.as_mut() as *mut Self as *mut std::ffi::c_void;
        nmp_ffi::nmp_app_set_update_callback(app, context, Some(on_update));
    }
}

pub fn unregister(app: *mut NmpApp) {
    nmp_ffi::nmp_app_set_update_callback(app, std::ptr::null_mut(), None);
}

extern "C" fn on_update(context: *mut std::ffi::c_void, _payload: *const u8, _len: usize) {
    if context.is_null() {
        return;
    }
    let bridge = unsafe { &*(context as *const NmpUpdateBridge) };
    let _ = bridge.tx.send(NmpEvent);
}
